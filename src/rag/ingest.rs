//! Document ingestion pipeline for token_compress_engine RAG.
//!
//! Implements PDF document processing, text extraction, chunking, and embedding
//! storage in accordance with `ARCHITECTURE.md §3.1`.

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

use super::embeddings::EmbeddingEngine;
use super::store::RagStore;
use super::types::{
    ChunkMetadata, ChunkingConfig, DocumentStatus, IngestionResult, RagChunk, RagDocument,
    DEFAULT_EMBEDDING_MODEL, DEFAULT_MODEL_VERSION,
};
use crate::npae::aggressive::config::{ConfigHandle, UnifiedConfig};

/// RAII drop guard guaranteeing that a temporary file is deleted on exit, return, or panic.
struct TempFileGuard {
    path: std::path::PathBuf,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Sanitizes an uploaded document filename to prevent path traversal and script injection.
///
/// Extracts the basename via `Path::file_name`, strips non-whitelisted characters
/// (keeping only `[A-Za-z0-9._-]`), caps the result at 128 characters (preserving `.pdf` if present),
/// and falls back to `"document.pdf"` if empty.
///
/// # Panics
/// Never panics.
pub fn sanitize_upload_filename(raw: &str) -> String {
    let basename = Path::new(raw)
        .file_name()
        .and_then(|os| os.to_str())
        .unwrap_or(raw);
    let filtered: String = basename
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect();
    let trimmed = filtered.trim_matches(|c| c == ' ');
    let capped = if trimmed.len() > 128 {
        if trimmed.to_ascii_lowercase().ends_with(".pdf") {
            let prefix = &trimmed[..124];
            format!("{prefix}.pdf")
        } else {
            trimmed[..128].to_string()
        }
    } else {
        trimmed.to_string()
    };
    if capped.is_empty() || capped == "." || capped == ".." {
        "document.pdf".to_string()
    } else {
        capped
    }
}

/// Validates an uploaded PDF payload against size, name, MIME type, and magic bytes contracts.
///
/// # Arguments
/// - `content_type`: Optional HTTP `Content-Type` header (must be `application/pdf` or `application/octet-stream` if provided).
/// - `filename`: Client-supplied filename to sanitize and check for `.pdf` extension.
/// - `bytes`: Raw file bytes.
/// - `max_bytes`: Maximum allowed payload size in bytes.
///
/// # Errors
/// Returns `Err` with human-readable diagnostic if validation fails.
///
/// # Panics
/// Never panics.
pub fn validate_pdf_upload(
    content_type: Option<&str>,
    filename: &str,
    bytes: &[u8],
    max_bytes: usize,
) -> Result<String, String> {
    if bytes.is_empty() {
        return Err("Uploaded PDF byte buffer must not be empty".to_string());
    }
    if bytes.len() > max_bytes {
        return Err(format!(
            "File size ({} bytes) exceeds maximum allowed size ({} bytes)",
            bytes.len(),
            max_bytes
        ));
    }
    let sanitized = sanitize_upload_filename(filename);
    if !sanitized.to_ascii_lowercase().ends_with(".pdf") {
        return Err(format!(
            "Filename must end with .pdf extension (got '{}')",
            sanitized
        ));
    }
    if let Some(ct) = content_type {
        let ct_clean = ct
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !ct_clean.is_empty()
            && ct_clean != "application/pdf"
            && ct_clean != "application/octet-stream"
        {
            return Err(format!(
                "Invalid Content-Type '{}'; only application/pdf or application/octet-stream are allowed",
                ct
            ));
        }
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("Invalid PDF magic bytes: file must begin with '%PDF-'".to_string());
    }
    Ok(sanitized)
}

/// PDF Ingestion Pipeline
pub struct DocumentIngester {
    store: RagStore,
    embeddings: EmbeddingEngine,
    config_handle: Option<Arc<ConfigHandle>>,
}

impl DocumentIngester {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            store: RagStore::new(pool),
            embeddings: EmbeddingEngine::new(),
            config_handle: None,
        }
    }

    pub fn with_config(pool: SqlitePool, config_handle: Arc<ConfigHandle>) -> Self {
        Self {
            store: RagStore::new(pool),
            embeddings: EmbeddingEngine::new(),
            config_handle: Some(config_handle),
        }
    }

    /// Ingest a PDF document from bytes defaulting to `'default'` tenant.
    ///
    /// # Errors
    /// Returns `Err` if file constraints fail or database writes fail.
    pub async fn ingest_pdf(
        &self,
        user_id: &str,
        filename: &str,
        bytes: &[u8],
        config: &ChunkingConfig,
    ) -> Result<IngestionResult, String> {
        self.ingest_pdf_scoped("default", user_id, filename, bytes, config)
            .await
    }

    /// Ingest a PDF document from bytes scoped to `tenant_id` and `user_id`.
    ///
    /// 1. Verifies file constraints (size, magic bytes, page count)
    /// 2. Extracts text page-by-page (via OS temp file with RAII cleanup)
    /// 3. Computes content hash to deduplicate within `(tenant_id, user_id)`
    /// 4. Auto-detects domain taxonomy from in-memory `ConfigHandle`
    /// 5. Chunks text with overlap
    /// 6. Generates local embeddings
    /// 7. Persists document + chunks to the DB transactionally
    ///
    /// # Errors
    /// Returns `Err` if validation, extraction, or DB persistence fails.
    pub async fn ingest_pdf_scoped(
        &self,
        tenant_id: &str,
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

        if bytes.is_empty() {
            return Err("Uploaded PDF byte buffer must not be empty".to_string());
        }

        // Validate magic bytes
        if !bytes.starts_with(b"%PDF-") {
            return Err("Invalid PDF magic bytes: file must begin with '%PDF-'".to_string());
        }

        let sanitized_filename = sanitize_upload_filename(filename);

        // Calculate SHA-256 content hash
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let content_hash = format!("{:x}", hasher.finalize());

        // Check if document already exists for this tenant and user
        if let Ok(Some(existing_doc)) = self
            .store
            .find_by_content_hash_scoped(tenant_id, user_id, &content_hash)
            .await
        {
            if existing_doc.status != DocumentStatus::Failed {
                info!(
                    "Document already exists with hash {}, skipping ingestion",
                    content_hash
                );
                return Ok(IngestionResult {
                    document_id: existing_doc.id,
                    chunks_created: existing_doc.chunk_count.unwrap_or(0) as usize,
                    detected_domain: existing_doc.domain,
                    page_count: existing_doc.page_count.unwrap_or(0) as usize,
                    status: existing_doc.status,
                    error: None,
                });
            }

            info!(
                "Document with hash {} was previously failed; cleaning up failed record to allow re-ingestion",
                content_hash
            );
            let _ = self
                .store
                .delete_document_scoped(tenant_id, user_id, &existing_doc.id)
                .await;
        }

        // Insert initial document record in processing state
        let doc_id = Uuid::new_v4().to_string();
        let initial_doc = RagDocument {
            id: doc_id.clone(),
            user_id: user_id.to_string(),
            filename: sanitized_filename,
            domain: None,
            content_hash: content_hash.clone(),
            page_count: None,
            chunk_count: None,
            status: DocumentStatus::Processing,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        if let Err(e) = self
            .store
            .insert_document_with_tenant(&initial_doc, tenant_id)
            .await
        {
            return Err(format!("Failed to insert document metadata: {}", e));
        }

        // Extract text and process (offload heavy extraction/embedding to blocking thread pool)
        let doc_id_clone = doc_id.clone();
        let bytes_vec = bytes.to_vec();
        let config_clone = config.clone();
        let embeddings_engine = self.embeddings;
        let unified_cfg_opt = self.config_handle.as_ref().map(|h| h.read_arc());
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelled_worker = Arc::clone(&cancelled);

        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_secs(55),
            tokio::task::spawn_blocking(move || {
                // Write PDF bytes to temp file in OS temp directory with guaranteed cleanup (never CWD!)
                let temp_path =
                    std::env::temp_dir().join(format!("tce_upload_{}.pdf", Uuid::new_v4()));
                let _guard = TempFileGuard {
                    path: temp_path.clone(),
                };

                let mut file = match File::create(&temp_path) {
                    Ok(f) => f,
                    Err(e) => {
                        return Err(format!("Failed to create temp PDF file: {}", e));
                    }
                };

                if let Err(e) = file.write_all(&bytes_vec) {
                    return Err(format!("Failed to write to temp PDF file: {}", e));
                }

                // Extract pages
                let pages = match pdf_extract::extract_text_by_pages(&temp_path) {
                    Ok(p) => p,
                    Err(e) => {
                        return Err(format!("Failed to extract text from PDF: {:?}", e));
                    }
                };

                // Explicit cleanup of temp file before processing extracted pages
                drop(_guard);

                if pages.is_empty() {
                    return Err("PDF document contains no text".to_string());
                }

                // Enforce DoS protection: cap maximum extracted text size before chunking & embedding
                const MAX_EXTRACTED_TEXT_BYTES: usize = 20 * 1024 * 1024;
                let total_text_bytes: usize = pages.iter().map(|p| p.len()).sum();
                if total_text_bytes > MAX_EXTRACTED_TEXT_BYTES {
                    return Err(format!(
                        "Extracted PDF text size ({} bytes) exceeds maximum allowed limit of {} bytes",
                        total_text_bytes, MAX_EXTRACTED_TEXT_BYTES
                    ));
                }

                if pages.len() > config_clone.max_pages {
                    return Err(format!(
                        "PDF page count {} exceeds maximum allowed page count {}",
                        pages.len(),
                        config_clone.max_pages
                    ));
                }

                // Auto-detect domain based on the first two pages
                let first_two_pages_text =
                    pages.iter().take(2).cloned().collect::<Vec<_>>().join("\n");
                let detected_domain =
                    detect_domain(&first_two_pages_text, unified_cfg_opt.as_deref());

                // Chunk the text and embed in bounded batches with cooperative cancellation
                let mut chunks = chunk_text(&pages, &config_clone);
                const EMBED_BATCH_SIZE: usize = 32;
                for batch in chunks.chunks_mut(EMBED_BATCH_SIZE) {
                    if cancelled_worker.load(std::sync::atomic::Ordering::Relaxed) {
                        return Err("PDF ingestion cancelled due to timeout".to_string());
                    }
                    let texts: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();
                    let vectors = embeddings_engine.embed_batch(&texts);
                    for (chunk, vector) in batch.iter_mut().zip(vectors.into_iter()) {
                        chunk.document_id = doc_id_clone.clone();
                        chunk.embedding = Some(vector);
                    }
                }

                Ok((pages.len(), chunks, detected_domain))
            }),
        )
        .await;

        let result = match timeout_result {
            Ok(join_res) => join_res,
            Err(_) => {
                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = self
                    .store
                    .update_document_status(&doc_id, DocumentStatus::Failed, 0, 0, None)
                    .await;
                return Err("PDF extraction timed out".to_string());
            }
        };

        match result {
            Ok(Ok((page_count, chunks, detected_domain))) => {
                let chunk_count = chunks.len();

                // Save all chunks to the database
                if let Err(e) = self.store.insert_chunks(&chunks).await {
                    error!("Failed to store chunks for document {}: {}", doc_id, e);
                    let _ = self
                        .store
                        .update_document_status(&doc_id, DocumentStatus::Failed, 0, 0, None)
                        .await;
                    return Err(format!("Database error writing chunks: {}", e));
                }

                // Update document to Ready with statistics
                if let Err(e) = self
                    .store
                    .update_document_status(
                        &doc_id,
                        DocumentStatus::Ready,
                        chunk_count as i64,
                        page_count as i64,
                        detected_domain.as_deref(),
                    )
                    .await
                {
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
                let _ = self
                    .store
                    .update_document_status(&doc_id, DocumentStatus::Failed, 0, 0, None)
                    .await;
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
                let _ = self
                    .store
                    .update_document_status(&doc_id, DocumentStatus::Failed, 0, 0, None)
                    .await;
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

/// Auto-detects domain from text against unified domain taxonomy.
///
/// Uses in-memory `config` if provided without reading from disk (GAP-37);
/// falls back to loading `config/unified.json` from disk if `config` is `None`.
///
/// # Panics
/// Never panics.
pub fn detect_domain(text: &str, config: Option<&UnifiedConfig>) -> Option<String> {
    let text_lower = text.to_lowercase();
    let mut best_domain = None;
    let mut max_matches = 0;

    if let Some(cfg) = config {
        for dt in &cfg.domain_taxonomy {
            let mut matches = 0;
            for kw in &dt.keywords {
                let kw_lower = kw.to_lowercase();
                if text_lower.contains(&kw_lower) {
                    matches += 1;
                }
            }
            if matches > max_matches && matches > 0 {
                max_matches = matches;
                best_domain = Some(dt.domain.clone());
            }
        }
        return best_domain;
    }

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
                embedding_model: DEFAULT_EMBEDDING_MODEL.to_string(),
                model_version: DEFAULT_MODEL_VERSION.to_string(),
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
        if trimmed.len() < 60
            && (trimmed.chars().next()?.is_uppercase() || trimmed.to_uppercase() == trimmed)
        {
            return Some(trimmed.to_string());
        }
        break;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npae::aggressive::config::DomainTaxonomy;

    #[test]
    fn test_sanitize_upload_filename() {
        assert_eq!(
            sanitize_upload_filename("../../etc/passwd.pdf"),
            "passwd.pdf"
        );
        assert_eq!(
            sanitize_upload_filename("upload<script>alert(1)</script>.pdf"),
            "script.pdf"
        );
        assert_eq!(
            sanitize_upload_filename("upload<script>.pdf"),
            "uploadscript.pdf"
        );
        assert_eq!(sanitize_upload_filename(""), "document.pdf");
        assert_eq!(sanitize_upload_filename(".."), "document.pdf");
        let long_name = format!("{}.pdf", "a".repeat(200));
        let sanitized = sanitize_upload_filename(&long_name);
        assert!(sanitized.len() <= 128);
        assert!(sanitized.ends_with(".pdf"));
    }

    #[test]
    fn test_validate_pdf_upload() {
        let valid_bytes = b"%PDF-1.7 valid content";
        let res = validate_pdf_upload(
            Some("application/pdf"),
            "test_document.pdf",
            valid_bytes,
            1024 * 1024,
        );
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "test_document.pdf");

        // Reject empty
        assert!(validate_pdf_upload(None, "doc.pdf", b"", 1024).is_err());

        // Reject non-PDF magic bytes
        assert!(validate_pdf_upload(None, "doc.pdf", b"NOT_PDF", 1024).is_err());

        // Reject non-PDF extension
        assert!(validate_pdf_upload(None, "doc.exe", valid_bytes, 1024).is_err());

        // Reject invalid MIME type
        assert!(validate_pdf_upload(Some("text/html"), "doc.pdf", valid_bytes, 1024).is_err());
        assert!(validate_pdf_upload(Some("image/png"), "doc.pdf", valid_bytes, 1024).is_err());

        // Allow application/octet-stream
        assert!(validate_pdf_upload(
            Some("application/octet-stream"),
            "doc.pdf",
            valid_bytes,
            1024
        )
        .is_ok());
    }

    #[test]
    fn test_detect_domain_in_memory() {
        let taxonomy = vec![DomainTaxonomy {
            domain: "pharma".to_string(),
            keywords: vec!["clinical trial".to_string(), "fda approval".to_string()],
            ..Default::default()
        }];
        let config = UnifiedConfig {
            domain_taxonomy: taxonomy,
            roles: vec![],
            constraints: vec![],
        };
        let domain = detect_domain(
            "This protocol outlines a clinical trial for Phase II.",
            Some(&config),
        );
        assert_eq!(domain.as_deref(), Some("pharma"));
    }
}
