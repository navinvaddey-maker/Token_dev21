use std::fs::File;
use std::io::Write;
use std::path::Path;
use sha2::{Sha256, Digest};
use uuid::Uuid;
use sqlx::SqlitePool;
use tracing::{info, error};

use super::types::{ChunkingConfig, DocumentStatus, IngestionResult, RagChunk, RagDocument, ChunkMetadata};
use super::store::RagStore;
use super::embeddings::EmbeddingEngine;

/// PDF Ingestion Pipeline
#[allow(dead_code)]
pub struct DocumentIngester {
    store: RagStore,
    embeddings: EmbeddingEngine,
}

impl DocumentIngester {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            store: RagStore::new(pool),
            embeddings: EmbeddingEngine::new(),
        }
    }

    /// Ingest a PDF document from bytes.
    ///
    /// 1. Verifies file constraints (size, page count)
    /// 2. Extracts text page-by-page (via temp file)
    /// 3. Computes content hash to deduplicate
    /// 4. Auto-detects domain taxonomy
    /// 5. Chunks text with overlap
    /// 6. Generates local embeddings
    /// 7. Persists document + chunks to the DB
    pub async fn ingest_pdf(
        &self,
        user_id: &str,
        filename: &str,
        bytes: &[u8],
        config: &ChunkingConfig,
    ) -> Result<IngestionResult, String> {
        // Validate file size
        if bytes.len() > config.max_file_size_bytes {
            return Err(format!(
                "File size {} exceeds limit of {} bytes",
                bytes.len(),
                config.max_file_size_bytes
            ));
        }

        // Calculate SHA-256 content hash
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let content_hash = format!("{:x}", hasher.finalize());

        // Check if document already exists for this user
        if let Ok(Some(existing_doc)) = self.store.find_by_content_hash(user_id, &content_hash).await {
            info!("Document already exists with hash {}, skipping ingestion", content_hash);
            return Ok(IngestionResult {
                document_id: existing_doc.id,
                chunks_created: existing_doc.chunk_count.unwrap_or(0) as usize,
                detected_domain: existing_doc.domain,
                page_count: existing_doc.page_count.unwrap_or(0) as usize,
                status: existing_doc.status,
                error: None,
            });
        }

        // Insert initial document record in processing state
        let doc_id = Uuid::new_v4().to_string();
        let initial_doc = RagDocument {
            id: doc_id.clone(),
            user_id: user_id.to_string(),
            filename: filename.to_string(),
            domain: None,
            content_hash: content_hash.clone(),
            page_count: None,
            chunk_count: None,
            status: DocumentStatus::Processing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        if let Err(e) = self.store.insert_document(&initial_doc).await {
            return Err(format!("Failed to insert document metadata: {}", e));
        }

        // Extract text and process (offload heavy extraction/embedding to blocking thread pool)
        let doc_id_clone = doc_id.clone();
        let bytes_vec = bytes.to_vec();
        let _filename_str = filename.to_string();
        let config_clone = config.clone();
        let store_clone = self.store.pool(); // clone the database pool
        let embeddings_engine = EmbeddingEngine::new();

        let result = tokio::task::spawn_blocking(move || {
            let _temp_store = RagStore::new(store_clone);
            
            // Write PDF bytes to temp file inside workspace
            let temp_filename = format!("temp_upload_{}.pdf", Uuid::new_v4());
            let temp_path = Path::new(&temp_filename);
            
            let mut file = match File::create(temp_path) {
                Ok(f) => f,
                Err(e) => {
                    return Err(format!("Failed to create temp PDF file: {}", e));
                }
            };

            if let Err(e) = file.write_all(&bytes_vec) {
                let _ = std::fs::remove_file(temp_path);
                return Err(format!("Failed to write to temp PDF file: {}", e));
            }

            // Extract pages
            let pages = match pdf_extract::extract_text_by_pages(temp_path) {
                Ok(p) => p,
                Err(e) => {
                    let _ = std::fs::remove_file(temp_path);
                    return Err(format!("Failed to extract text from PDF: {:?}", e));
                }
            };

            // Clean up temp file immediately
            let _ = std::fs::remove_file(temp_path);

            if pages.is_empty() {
                return Err("PDF document contains no text".to_string());
            }

            if pages.len() > config_clone.max_pages {
                return Err(format!(
                    "PDF page count {} exceeds maximum allowed page count {}",
                    pages.len(),
                    config_clone.max_pages
                ));
            }

            // Auto-detect domain based on the first two pages
            let first_two_pages_text = pages.iter().take(2).cloned().collect::<Vec<_>>().join("\n");
            let detected_domain = detect_domain(&first_two_pages_text);

            // Chunk the text
            let mut chunks = chunk_text(&pages, &config_clone);
            for chunk in &mut chunks {
                chunk.document_id = doc_id_clone.clone();
                // Generate embeddings for chunk content
                let vector = embeddings_engine.embed(&chunk.content);
                chunk.embedding = Some(vector);
            }

            Ok((pages.len(), chunks, detected_domain))
        })
        .await;

        match result {
            Ok(Ok((page_count, chunks, detected_domain))) => {
                let chunk_count = chunks.len();
                
                // Save all chunks to the database
                if let Err(e) = self.store.insert_chunks(&chunks).await {
                    error!("Failed to store chunks for document {}: {}", doc_id, e);
                    let _ = self.store.update_document_status(
                        &doc_id,
                        DocumentStatus::Failed,
                        0,
                        0,
                        None,
                    ).await;
                    return Err(format!("Database error writing chunks: {}", e));
                }

                // Update document to Ready with statistics
                if let Err(e) = self.store.update_document_status(
                    &doc_id,
                    DocumentStatus::Ready,
                    chunk_count as i64,
                    page_count as i64,
                    detected_domain.as_deref(),
                ).await {
                    error!("Failed to update document status for {}: {}", doc_id, e);
                    return Err(format!("Database error updating document metadata: {}", e));
                }

                Ok(IngestionResult {
                    document_id: doc_id,
                    chunks_created: chunk_count,
                    detected_domain: detected_domain.clone(),
                    page_count,
                    status: DocumentStatus::Ready,
                    error: None,
                })
            }
            Ok(Err(err_msg)) => {
                error!("Ingestion process failed for {}: {}", doc_id, err_msg);
                let _ = self.store.update_document_status(
                    &doc_id,
                    DocumentStatus::Failed,
                    0,
                    0,
                    None,
                ).await;
                Ok(IngestionResult {
                    document_id: doc_id,
                    chunks_created: 0,
                    detected_domain: None,
                    page_count: 0,
                    status: DocumentStatus::Failed,
                    error: Some(err_msg),
                })
            }
            Err(join_err) => {
                error!("Ingestion task panicked for {}: {:?}", doc_id, join_err);
                let _ = self.store.update_document_status(
                    &doc_id,
                    DocumentStatus::Failed,
                    0,
                    0,
                    None,
                ).await;
                Ok(IngestionResult {
                    document_id: doc_id,
                    chunks_created: 0,
                    detected_domain: None,
                    page_count: 0,
                    status: DocumentStatus::Failed,
                    error: Some("Internal task join error".to_string()),
                })
            }
        }
    }
}



/// Simple helper to detect domain from first two pages text
fn detect_domain(text: &str) -> Option<String> {
    let config_path = "config/unified.json";
    let config_data = match std::fs::read_to_string(config_path) {
        Ok(data) => data,
        Err(_) => return None,
    };

    #[derive(serde::Deserialize)]
    struct MiniTaxonomy {
        domain_taxonomy: Vec<MiniDomainTaxonomy>,
    }
    #[derive(serde::Deserialize)]
    struct MiniDomainTaxonomy {
        domain: String,
        keywords: Vec<String>,
    }

    let taxonomy: MiniTaxonomy = match serde_json::from_str(&config_data) {
        Ok(t) => t,
        Err(_) => return None,
    };

    let text_lower = text.to_lowercase();
    let mut best_domain = None;
    let mut max_matches = 0;

    for dt in taxonomy.domain_taxonomy {
        let mut matches = 0;
        for kw in &dt.keywords {
            let kw_lower = kw.to_lowercase();
            if text_lower.contains(&kw_lower) {
                matches += 1;
            }
        }
        if matches > max_matches && matches > 0 {
            max_matches = matches;
            best_domain = Some(dt.domain);
        }
    }

    best_domain
}

/// Chunking function using standard word splitting and sliding window
fn chunk_text(pages: &[String], config: &ChunkingConfig) -> Vec<RagChunk> {
    let mut chunks = Vec::new();
    let mut chunk_index = 0;

    for (page_idx, page_text) in pages.iter().enumerate() {
        let page_num = (page_idx + 1) as i64;
        
        // Split page text into words with start/end character offsets
        let mut words = Vec::new();
        let mut start_idx = None;
        for (i, c) in page_text.char_indices() {
            if c.is_whitespace() {
                if let Some(start) = start_idx {
                    words.push((start, i, &page_text[start..i]));
                    start_idx = None;
                }
            } else if start_idx.is_none() {
                start_idx = Some(i);
            }
        }
        if let Some(start) = start_idx {
            words.push((start, page_text.len(), &page_text[start..]));
        }

        if words.is_empty() {
            continue;
        }

        let chunk_size = config.chunk_size_tokens;
        let overlap = config.overlap_tokens;
        
        let mut i = 0;
        while i < words.len() {
            let end = std::cmp::min(i + chunk_size, words.len());
            let chunk_words = &words[i..end];
            
            let chunk_content = chunk_words
                .iter()
                .map(|(_, _, w)| *w)
                .collect::<Vec<_>>()
                .join(" ");
                
            let char_offset_start = chunk_words.first().map(|(s, _, _)| *s).unwrap_or(0);
            let char_offset_end = chunk_words.last().map(|(_, e, _)| *e).unwrap_or(0);
            
            let section_title = detect_section_title(&chunk_content);
            
            let metadata = ChunkMetadata {
                page_number: Some(page_num),
                section_title,
                char_offset_start: Some(char_offset_start),
                char_offset_end: Some(char_offset_end),
            };
            
            let chunk = RagChunk {
                id: Uuid::new_v4().to_string(),
                document_id: String::new(), // Filled by caller
                chunk_index: chunk_index as i64,
                content: chunk_content,
                embedding: None, // Filled by caller
                metadata: Some(metadata),
                token_count: Some(chunk_words.len() as i64),
                created_at: chrono::Utc::now(),
            };
            
            chunks.push(chunk);
            chunk_index += 1;
            
            if end == words.len() {
                break;
            }
            
            if chunk_size > overlap {
                i += chunk_size - overlap;
            } else {
                i += 1;
            }
        }
    }

    chunks
}

/// Simple heuristic to extract section header from chunk content
fn detect_section_title(text: &str) -> Option<String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() < 60 && (trimmed.chars().next()?.is_uppercase() || trimmed.to_uppercase() == trimmed) {
            return Some(trimmed.to_string());
        }
        break;
    }
    None
}
