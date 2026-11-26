//! Local offline-first RAG engine (`src/rag/local.rs`).
//!
//! Implements `ARCHITECTURE.md` §2.1 (Ask RAG Data Source Boundary — Own Data):
//! - Incremental folder indexing using SHA-256 file hashes persisted locally in JSON.
//! - Local-only extraction for `.pdf` (`pdf-extract`) and `.docx` (`zip` + `xml-rs`).
//! - Hybrid search combining `all-MiniLM-L6-v2` ONNX embeddings (`EmbeddingEngine`)
//!   with BM25 lexical scoring via Reciprocal Rank Fusion (RRF, `k = 60.0`).
//! - Strict JSON IPC request/response contracts with zero cloud egress.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::{BufReader, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use xml::reader::{EventReader, XmlEvent};
use zip::ZipArchive;

use super::embeddings::{cosine_similarity, EmbeddingEngine};
use super::types::{DEFAULT_EMBEDDING_MODEL, EMBEDDING_DIM};
use crate::utils::{hash::OryxHasher, math::OryxVectorMath};

/// Maximum supported local document size (50 MiB).
pub const MAX_LOCAL_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Maximum allowed decompressed size for `word/document.xml` (16 MiB) to prevent decompression bombs.
pub const MAX_DOCX_XML_BYTES: u64 = 16 * 1024 * 1024;
/// Default sliding-window chunk size in whitespace words (calibrated for all-MiniLM-L6-v2).
const DEFAULT_CHUNK_WORDS: usize = 200;
/// Default overlap between adjacent chunks in words.
const DEFAULT_CHUNK_OVERLAP: usize = 32;
/// Default index filename stored inside the watched folder.
pub const DEFAULT_INDEX_FILENAME: &str = ".rag_local_index.json";

/// Errors returned by local folder indexing and retrieval operations.
#[derive(Debug, Error)]
pub enum LocalRagError {
    /// Invalid request contract (empty query, unsupported extension, or invalid `top_k`).
    #[error("validation error: {0}")]
    Validation(String),
    /// Filesystem I/O failure when scanning, reading, or persisting the local index.
    #[error("local filesystem error: {0}")]
    Io(#[from] std::io::Error),
    /// Failed to parse or serialize local index JSON or IPC JSON.
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
    /// Failed to extract text from a `.pdf` or `.docx` document.
    #[error("document extraction error ({path}): {reason}")]
    Extraction { path: String, reason: String },
}

fn default_top_k() -> usize {
    5
}

fn default_extensions() -> Vec<String> {
    vec![".pdf".to_string(), ".docx".to_string()]
}

/// Strict JSON IPC request accepted by `POST http://127.0.0.1:49152/retrieve` and `--stdin`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalRetrieveRequest {
    /// Natural-language search query.
    pub query: String,
    /// Maximum number of top chunks to return (clamped to `1..=50`).
    #[serde(default = "default_top_k")]
    pub top_k: usize,
    /// Allowed file extensions (supported: `.pdf`, `.docx`).
    #[serde(default = "default_extensions")]
    pub file_extensions: Vec<String>,
}

/// A single retrieved local document chunk adhering to the strict JSON IPC response contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRetrievedChunk {
    /// Relative filename inside the watched folder.
    pub source_file: String,
    /// Normalized lowercase extension (`.pdf` or `.docx`).
    pub file_extension: String,
    /// 1-based page number (or inferred section page for `.docx`).
    pub page_number: u32,
    /// 0-based chunk index within the document.
    pub chunk_index: usize,
    /// Normalized hybrid RRF relevance score.
    pub score: f32,
    /// Extracted chunk text.
    pub content: String,
}

/// Strict JSON IPC response returned by `POST http://127.0.0.1:49152/retrieve` and `--stdin`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalRetrieveResponse {
    /// Status indicator (`"ok"`).
    pub status: String,
    /// Echoed query string.
    pub query: String,
    /// Ranked top-$k$ retrieved chunks.
    pub chunks: Vec<LocalRetrievedChunk>,
    /// Count of files currently indexed locally.
    pub indexed_files: usize,
    /// Query latency in milliseconds.
    pub latency_ms: u64,
}

/// Companion health response returned by `GET http://127.0.0.1:49152/health`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalHealthResponse {
    /// Status indicator (`"ok"`).
    pub status: String,
    /// Absolute or configured path of the watched local folder.
    pub watched_dir: String,
    /// Number of indexed local files.
    pub indexed_files: usize,
    /// Total number of indexed chunks.
    pub indexed_chunks: usize,
    /// Whether the ONNX `all-MiniLM-L6-v2` neural model is active.
    pub neural_embeddings: bool,
    /// Supported file extensions.
    pub extensions: Vec<String>,
}

/// Summary of an incremental folder synchronization pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncSummary {
    /// Number of files newly parsed and embedded.
    pub indexed: usize,
    /// Number of unchanged files skipped via SHA-256 match.
    pub skipped_unchanged: usize,
    /// Number of deleted files removed from the local index.
    pub removed: usize,
    /// Number of files that failed extraction (logged and skipped).
    pub failed: usize,
}

/// Persisted local index schema stored on disk (`.rag_local_index.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalIndexStore {
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// Embedding model identifier (`all-MiniLM-L6-v2`).
    pub embedding_model: String,
    /// Expected vector dimensionality (`384`).
    pub embedding_dim: usize,
    /// Map of relative file path -> indexed file metadata and chunks.
    pub files: BTreeMap<String, IndexedLocalFile>,
}

/// Indexed metadata and chunks for a single local file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedLocalFile {
    /// Relative path inside the watched folder.
    pub source_file: String,
    /// Lowercase file extension (`.pdf` or `.docx`).
    pub file_extension: String,
    /// Hex-encoded SHA-256 digest of raw file bytes.
    pub sha256: String,
    /// Extracted and embedded chunks belonging to this file.
    pub chunks: Vec<IndexedLocalChunk>,
}

/// Stored chunk entry with its L2-normalized embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedLocalChunk {
    /// 0-based chunk index inside the file.
    pub chunk_index: usize,
    /// 1-based page number.
    pub page_number: u32,
    /// Chunk text content.
    pub content: String,
    /// L2-normalized 384-dim embedding vector.
    pub embedding: Vec<f32>,
}

impl Default for LocalIndexStore {
    fn default() -> Self {
        Self {
            schema_version: 1,
            embedding_model: DEFAULT_EMBEDDING_MODEL.to_string(),
            embedding_dim: EMBEDDING_DIM,
            files: BTreeMap::new(),
        }
    }
}

impl LocalIndexStore {
    /// Loads an existing local index JSON file if present, or returns an empty store.
    ///
    /// If the existing index uses a mismatched embedding model or dimension, it is
    /// reset so all documents are cleanly re-embedded under the active contract.
    ///
    /// # Errors
    /// Returns `LocalRagError::Io` or `LocalRagError::Json` if reading or parsing fails.
    pub fn load_or_empty(index_path: &Path) -> Result<Self, LocalRagError> {
        if !index_path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(index_path)?;
        let parsed: Self = serde_json::from_str(&raw)?;
        if parsed.embedding_model != DEFAULT_EMBEDDING_MODEL
            || parsed.embedding_dim != EMBEDDING_DIM
        {
            tracing::warn!(
                found_model = %parsed.embedding_model,
                found_dim = parsed.embedding_dim,
                "Local index embedding contract changed; resetting index for clean re-embedding"
            );
            return Ok(Self::default());
        }
        Ok(parsed)
    }

    /// Atomically persists the local index JSON file using a temp file + rename.
    ///
    /// # Errors
    /// Returns `LocalRagError::Io` or `LocalRagError::Json` on failure.
    pub fn save_atomic(&self, index_path: &Path) -> Result<(), LocalRagError> {
        if let Some(parent) = index_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let json = serde_json::to_string(self)?;
        let tmp_path = index_path.with_extension("json.tmp");
        fs::write(&tmp_path, json.as_bytes())?;
        fs::rename(&tmp_path, index_path)?;
        Ok(())
    }

    /// Total number of chunks currently stored across all indexed files.
    pub fn total_chunks(&self) -> usize {
        self.files.values().map(|f| f.chunks.len()).sum()
    }

    /// Incrementally synchronizes `watched_dir` into this store using SHA-256 deduplication.
    ///
    /// - Never re-parses or re-embeds files whose SHA-256 hash matches the local index.
    /// - Removes entries for files deleted from `watched_dir`.
    /// - Persists `index_path` atomically only when changes occur.
    ///
    /// # Errors
    /// Returns `LocalRagError::Io` if `watched_dir` cannot be traversed or `index_path` cannot be written.
    pub fn sync_folder(
        &mut self,
        watched_dir: &Path,
        index_path: &Path,
        embeddings: &EmbeddingEngine,
    ) -> Result<SyncSummary, LocalRagError> {
        // ARCH: §2.1 — Ensure watched folder exists locally; raw files never leave this host.
        if !watched_dir.exists() {
            fs::create_dir_all(watched_dir)?;
        }

        let discovered = discover_supported_files(watched_dir)?;
        let discovered_set: HashSet<String> =
            discovered.iter().map(|(rel, _, _)| rel.clone()).collect();

        let mut summary = SyncSummary::default();
        let mut dirty = false;

        // Prune files deleted from disk.
        let stale_keys: Vec<String> = self
            .files
            .keys()
            .filter(|k| !discovered_set.contains(*k))
            .cloned()
            .collect();
        for key in stale_keys {
            self.files.remove(&key);
            summary.removed += 1;
            dirty = true;
        }

        // Process new or modified files.
        for (rel_path, ext, full_path) in discovered {
            let meta = match fs::metadata(&full_path) {
                Ok(m) => m,
                Err(err) => {
                    tracing::warn!(file = %rel_path, error = %err, "Failed to stat local file");
                    summary.failed += 1;
                    continue;
                }
            };
            if meta.len() > MAX_LOCAL_FILE_BYTES {
                tracing::warn!(
                    file = %rel_path,
                    bytes = meta.len(),
                    "Skipping local file exceeding 50 MiB limit"
                );
                summary.failed += 1;
                continue;
            }

            let bytes = match fs::read(&full_path) {
                Ok(b) => b,
                Err(err) => {
                    tracing::warn!(file = %rel_path, error = %err, "Failed to read local file");
                    summary.failed += 1;
                    continue;
                }
            };
            let hash = compute_sha256_hex(&bytes);

            if let Some(existing) = self.files.get(&rel_path) {
                if existing.sha256 == hash {
                    summary.skipped_unchanged += 1;
                    continue;
                }
            }

            let pages = match extract_document_pages(&ext, &bytes, &rel_path) {
                Ok(p) => p,
                Err(err) => {
                    tracing::warn!(file = %rel_path, error = %err, "Document extraction failed");
                    summary.failed += 1;
                    continue;
                }
            };

            let raw_chunks = chunk_pages(&pages, DEFAULT_CHUNK_WORDS, DEFAULT_CHUNK_OVERLAP);
            if raw_chunks.is_empty() {
                // Store an empty chunk list with SHA-256 so empty files aren't re-parsed every poll.
                self.files.insert(
                    rel_path.clone(),
                    IndexedLocalFile {
                        source_file: rel_path,
                        file_extension: ext,
                        sha256: hash,
                        chunks: Vec::new(),
                    },
                );
                summary.indexed += 1;
                dirty = true;
                continue;
            }

            let texts: Vec<String> = raw_chunks.iter().map(|(_, _, c)| c.clone()).collect();
            let vectors = embeddings.embed_batch(&texts);
            let mut indexed_chunks = Vec::with_capacity(raw_chunks.len());
            for ((chunk_index, page_number, content), embedding) in
                raw_chunks.into_iter().zip(vectors.into_iter())
            {
                if embedding.len() != EMBEDDING_DIM {
                    continue;
                }
                indexed_chunks.push(IndexedLocalChunk {
                    chunk_index,
                    page_number,
                    content,
                    embedding,
                });
            }

            self.files.insert(
                rel_path.clone(),
                IndexedLocalFile {
                    source_file: rel_path,
                    file_extension: ext,
                    sha256: hash,
                    chunks: indexed_chunks,
                },
            );
            summary.indexed += 1;
            dirty = true;
        }

        if dirty {
            self.save_atomic(index_path)?;
        }

        Ok(summary)
    }

    /// Executes hybrid (dense cosine + BM25 lexical via RRF) retrieval over the local index.
    ///
    /// # Errors
    /// Returns `LocalRagError::Validation` if `req.query` is blank or `req.file_extensions`
    /// contains unsupported extensions.
    pub fn retrieve(
        &self,
        req: &LocalRetrieveRequest,
        embeddings: &EmbeddingEngine,
    ) -> Result<LocalRetrieveResponse, LocalRagError> {
        let start = Instant::now();
        let query_trimmed = req.query.trim();
        if query_trimmed.is_empty() {
            return Err(LocalRagError::Validation(
                "query must not be empty".to_string(),
            ));
        }

        let top_k = req.top_k.clamp(1, 50);
        let allowed_exts = normalize_extensions(&req.file_extensions)?;
        let query_vec = embeddings.embed(query_trimmed);
        let query_terms = tokenize_terms(query_trimmed);

        // Collect candidate chunks filtered by requested file extensions.
        struct Candidate<'a> {
            file: &'a IndexedLocalFile,
            chunk: &'a IndexedLocalChunk,
            vector_score: f32,
            lexical_score: f32,
        }

        let mut candidates: Vec<Candidate<'_>> = Vec::new();
        for file in self.files.values() {
            if !allowed_exts.contains(&file.file_extension) {
                continue;
            }
            for chunk in &file.chunks {
                if chunk.embedding.len() != EMBEDDING_DIM {
                    continue;
                }
                let vec_sim = cosine_similarity(&query_vec, &chunk.embedding);
                candidates.push(Candidate {
                    file,
                    chunk,
                    vector_score: vec_sim,
                    lexical_score: 0.0,
                });
            }
        }

        if candidates.is_empty() {
            return Ok(LocalRetrieveResponse {
                status: "ok".to_string(),
                query: query_trimmed.to_string(),
                chunks: Vec::new(),
                indexed_files: self.files.len(),
                latency_ms: start.elapsed().as_millis() as u64,
            });
        }

        // Compute BM25 lexical scores across the candidate pool.
        let doc_tokens: Vec<Vec<String>> = candidates
            .iter()
            .map(|c| tokenize_terms(&c.chunk.content))
            .collect();
        let bm25_scores = score_bm25(&query_terms, &doc_tokens);
        for (cand, bm25) in candidates.iter_mut().zip(bm25_scores.into_iter()) {
            cand.lexical_score = bm25;
        }

        // Build vector rank order (1-based).
        let mut vec_indices: Vec<usize> = (0..candidates.len()).collect();
        vec_indices.sort_by(|&a, &b| {
            candidates[b]
                .vector_score
                .total_cmp(&candidates[a].vector_score)
                .then_with(|| {
                    candidates[a]
                        .file
                        .source_file
                        .cmp(&candidates[b].file.source_file)
                })
                .then_with(|| {
                    candidates[a]
                        .chunk
                        .chunk_index
                        .cmp(&candidates[b].chunk.chunk_index)
                })
        });
        let mut vec_ranks = vec![usize::MAX; candidates.len()];
        for (rank_idx, &cand_idx) in vec_indices.iter().enumerate() {
            vec_ranks[cand_idx] = rank_idx + 1;
        }

        // Build lexical rank order (1-based, only for positive lexical matches).
        let mut lex_indices: Vec<usize> = (0..candidates.len())
            .filter(|&i| candidates[i].lexical_score > 0.0)
            .collect();
        lex_indices.sort_by(|&a, &b| {
            candidates[b]
                .lexical_score
                .total_cmp(&candidates[a].lexical_score)
                .then_with(|| {
                    candidates[a]
                        .file
                        .source_file
                        .cmp(&candidates[b].file.source_file)
                })
                .then_with(|| {
                    candidates[a]
                        .chunk
                        .chunk_index
                        .cmp(&candidates[b].chunk.chunk_index)
                })
        });
        let mut lex_ranks = vec![None; candidates.len()];
        for (rank_idx, &cand_idx) in lex_indices.iter().enumerate() {
            lex_ranks[cand_idx] = Some(rank_idx + 1);
        }

        // Fuse vector + lexical ranks via Reciprocal Rank Fusion (RRF) and filter low-signal noise.
        let mut scored: Vec<(usize, f32)> = Vec::new();
        for (idx, cand) in candidates.iter().enumerate() {
            // Require either lexical overlap or meaningful semantic similarity.
            if cand.lexical_score <= 0.0 && cand.vector_score < 0.12 {
                continue;
            }
            let combined = OryxVectorMath::compute_normalized_rrf_score(
                Some(vec_ranks[idx]),
                lex_ranks[idx],
                OryxVectorMath::RRF_SMOOTHING_K,
            );
            scored.push((idx, combined));
        }

        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| {
                    candidates[a.0]
                        .file
                        .source_file
                        .cmp(&candidates[b.0].file.source_file)
                })
                .then_with(|| {
                    candidates[a.0]
                        .chunk
                        .chunk_index
                        .cmp(&candidates[b.0].chunk.chunk_index)
                })
        });

        let chunks = scored
            .into_iter()
            .take(top_k)
            .map(|(idx, score)| {
                let c = &candidates[idx];
                LocalRetrievedChunk {
                    source_file: c.file.source_file.clone(),
                    file_extension: c.file.file_extension.clone(),
                    page_number: c.chunk.page_number,
                    chunk_index: c.chunk.chunk_index,
                    score: (score * 10_000.0).round() / 10_000.0,
                    content: c.chunk.content.clone(),
                }
            })
            .collect();

        Ok(LocalRetrieveResponse {
            status: "ok".to_string(),
            query: query_trimmed.to_string(),
            chunks,
            indexed_files: self.files.len(),
            latency_ms: start.elapsed().as_millis() as u64,
        })
    }
}

/// Computes SHA-256 hex digest for raw file bytes.
pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    OryxHasher::compute_sha256_hex_digest(bytes)
}

/// Normalizes and validates requested file extensions (`.pdf`, `.docx`).
fn normalize_extensions(exts: &[String]) -> Result<HashSet<String>, LocalRagError> {
    if exts.is_empty() {
        return Ok(default_extensions().into_iter().collect());
    }
    let mut out = HashSet::new();
    for raw in exts {
        let lower = raw.trim().to_ascii_lowercase();
        let normalized = if lower.starts_with('.') {
            lower
        } else {
            format!(".{lower}")
        };
        match normalized.as_str() {
            ".pdf" | ".docx" => {
                out.insert(normalized);
            }
            other => {
                return Err(LocalRagError::Validation(format!(
                    "unsupported file extension '{other}'; allowed: .pdf, .docx"
                )));
            }
        }
    }
    Ok(out)
}

/// Recursively discovers `.pdf` and `.docx` files inside `root`, skipping hidden entries.
fn discover_supported_files(root: &Path) -> Result<Vec<(String, String, PathBuf)>, LocalRagError> {
    let mut results = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)?;
        for entry_res in entries {
            let entry = entry_res?;
            let path = entry.path();
            let file_name = entry.file_name();
            let name_lossy = file_name.to_string_lossy();
            // Skip hidden files/folders (including `.rag_local_index.json` and temp files).
            if name_lossy.starts_with('.') {
                continue;
            }
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let ext = match path.extension().and_then(|e| e.to_str()) {
                Some(e) => format!(".{}", e.to_ascii_lowercase()),
                None => continue,
            };
            if ext != ".pdf" && ext != ".docx" {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            results.push((rel, ext, path));
        }
    }

    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// Extracts text pages from `.pdf` or `.docx` bytes completely offline.
///
/// # Errors
/// Returns `LocalRagError::Extraction` if parsing fails.
pub fn extract_document_pages(
    ext: &str,
    bytes: &[u8],
    display_path: &str,
) -> Result<Vec<String>, LocalRagError> {
    match ext {
        ".pdf" => extract_pdf_pages(bytes, display_path),
        ".docx" => extract_docx_pages(bytes, display_path),
        other => Err(LocalRagError::Extraction {
            path: display_path.to_string(),
            reason: format!("unsupported extension '{other}'"),
        }),
    }
}

/// RAII drop guard for local temp files ensuring cleanup on return, error, or unwind.
struct LocalTempFileGuard {
    path: std::path::PathBuf,
}

impl Drop for LocalTempFileGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Extracts per-page text from PDF bytes using `pdf_extract::extract_text_by_pages_from_mem`
/// (falling back to a workspace temp file if needed).
fn extract_pdf_pages(bytes: &[u8], display_path: &str) -> Result<Vec<String>, LocalRagError> {
    let tmp_name = format!(".rag_local_tmp_{}.pdf", uuid::Uuid::new_v4());
    let tmp_path = std::env::temp_dir().join(tmp_name);
    let _guard = LocalTempFileGuard {
        path: tmp_path.clone(),
    };
    fs::write(&tmp_path, bytes).map_err(|e| LocalRagError::Extraction {
        path: display_path.to_string(),
        reason: e.to_string(),
    })?;
    let extracted = pdf_extract::extract_text_by_pages(&tmp_path);
    drop(_guard);

    let pages = extracted.map_err(|e| LocalRagError::Extraction {
        path: display_path.to_string(),
        reason: format!("{e:?}"),
    })?;
    Ok(pages
        .into_iter()
        .map(|p| normalize_whitespace(&p))
        .filter(|p| !p.is_empty())
        .collect())
}

/// Extracts logical pages from a `.docx` ZIP archive by parsing `word/document.xml`.
///
/// Respects explicit `<w:br w:type="page"/>` breaks and paragraph `<w:p>` boundaries.
///
/// # Errors
/// Returns `LocalRagError::Extraction` if the archive or XML is invalid.
pub fn extract_docx_pages(bytes: &[u8], display_path: &str) -> Result<Vec<String>, LocalRagError> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| LocalRagError::Extraction {
        path: display_path.to_string(),
        reason: format!("invalid docx zip archive: {e}"),
    })?;

    let mut xml_bytes = Vec::new();
    {
        let doc_entry =
            archive
                .by_name("word/document.xml")
                .map_err(|e| LocalRagError::Extraction {
                    path: display_path.to_string(),
                    reason: format!("missing word/document.xml: {e}"),
                })?;
        doc_entry
            .take(MAX_DOCX_XML_BYTES + 1)
            .read_to_end(&mut xml_bytes)
            .map_err(|e| LocalRagError::Extraction {
                path: display_path.to_string(),
                reason: format!("failed reading word/document.xml: {e}"),
            })?;
        if xml_bytes.len() > MAX_DOCX_XML_BYTES as usize {
            return Err(LocalRagError::Extraction {
                path: display_path.to_string(),
                reason: format!(
                    "decompressed word/document.xml exceeds maximum allowed size of {} bytes (potential zip bomb)",
                    MAX_DOCX_XML_BYTES
                ),
            });
        }
    }

    let reader = EventReader::new(BufReader::new(Cursor::new(xml_bytes)));
    let mut pages: Vec<String> = Vec::new();
    let mut current_page = String::new();
    let mut in_text_elem = false;

    for event in reader {
        match event {
            Ok(XmlEvent::StartElement {
                name, attributes, ..
            }) => {
                let local = name.local_name.as_str();
                if local == "t" || local == "delText" {
                    in_text_elem = true;
                } else if local == "tab" {
                    current_page.push(' ');
                } else if local == "br" {
                    let is_page_break = attributes
                        .iter()
                        .any(|a| a.name.local_name == "type" && a.value == "page");
                    if is_page_break {
                        let trimmed = normalize_whitespace(&current_page);
                        if !trimmed.is_empty() {
                            pages.push(trimmed);
                        }
                        current_page.clear();
                    } else {
                        current_page.push('\n');
                    }
                }
            }
            Ok(XmlEvent::EndElement { name }) => {
                let local = name.local_name.as_str();
                if local == "t" || local == "delText" {
                    in_text_elem = false;
                } else if local == "p" || local == "tr" {
                    current_page.push('\n');
                }
            }
            Ok(XmlEvent::Characters(text)) if in_text_elem => {
                current_page.push_str(&text);
            }
            Err(e) => {
                return Err(LocalRagError::Extraction {
                    path: display_path.to_string(),
                    reason: format!("XML parse error: {e}"),
                });
            }
            _ => {}
        }
    }

    let tail = normalize_whitespace(&current_page);
    if !tail.is_empty() {
        pages.push(tail);
    }
    Ok(pages)
}

fn normalize_whitespace(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Splits extracted pages into sliding-window chunks `(chunk_index, page_number, content)`.
fn chunk_pages(
    pages: &[String],
    chunk_words: usize,
    overlap_words: usize,
) -> Vec<(usize, u32, String)> {
    let mut out = Vec::new();
    let mut chunk_index = 0usize;
    let step = chunk_words.saturating_sub(overlap_words).max(1);

    for (page_idx, page_text) in pages.iter().enumerate() {
        let words: Vec<&str> = page_text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let page_num = (page_idx + 1) as u32;
        let mut i = 0usize;
        while i < words.len() {
            let end = (i + chunk_words).min(words.len());
            let content = words[i..end].join(" ");
            out.push((chunk_index, page_num, content));
            chunk_index += 1;
            if end == words.len() {
                break;
            }
            i += step;
        }
    }

    out
}

fn tokenize_terms(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

/// Computes BM25 lexical relevance scores for each tokenized candidate document.
fn score_bm25(query_terms: &[String], docs: &[Vec<String>]) -> Vec<f32> {
    let n = docs.len();
    if n == 0 || query_terms.is_empty() {
        return vec![0.0; n];
    }

    let avg_dl = docs.iter().map(|d| d.len() as f32).sum::<f32>() / (n as f32);
    let avg_dl = avg_dl.max(1.0);
    let k1: f32 = 1.5;
    let b: f32 = 0.75;

    // Deduplicate query terms so repeated query words don't inflate IDF.
    let mut unique_query: Vec<&str> = query_terms.iter().map(|s| s.as_str()).collect();
    unique_query.sort_unstable();
    unique_query.dedup();

    // Document frequency per unique query term.
    let mut df: HashMap<&str, usize> = HashMap::new();
    for &qt in &unique_query {
        let count = docs
            .iter()
            .filter(|doc| doc.iter().any(|t| t == qt))
            .count();
        df.insert(qt, count);
    }

    let mut scores = vec![0.0f32; n];
    for (idx, doc) in docs.iter().enumerate() {
        if doc.is_empty() {
            continue;
        }
        let mut tf: HashMap<&str, usize> = HashMap::new();
        for t in doc {
            *tf.entry(t.as_str()).or_default() += 1;
        }
        let dl = doc.len() as f32;
        let mut score = 0.0f32;
        for &qt in &unique_query {
            let term_freq = *tf.get(qt).unwrap_or(&0) as f32;
            if term_freq <= 0.0 {
                continue;
            }
            let doc_freq = *df.get(qt).unwrap_or(&0) as f32;
            let idf = (((n as f32) - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln();
            let norm_tf =
                (term_freq * (k1 + 1.0)) / (term_freq + k1 * (1.0 - b + b * (dl / avg_dl)));
            score += idf * norm_tf;
        }
        scores[idx] = score;
    }

    scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn build_test_docx(paragraphs: &[&str]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buf);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            writer
                .start_file("word/document.xml", options)
                .expect("start_file must succeed");
            let mut xml = String::from(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
            );
            for (idx, para) in paragraphs.iter().enumerate() {
                if idx > 0 {
                    xml.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
                }
                xml.push_str("<w:p><w:r><w:t>");
                xml.push_str(para);
                xml.push_str("</w:t></w:r></w:p>");
            }
            xml.push_str("</w:body></w:document>");
            writer
                .write_all(xml.as_bytes())
                .expect("write_all must succeed");
            writer.finish().expect("finish must succeed");
        }
        buf.into_inner()
    }

    #[test]
    fn docx_extraction_splits_pages_and_reads_text() {
        let bytes = build_test_docx(&[
            "Indemnification cap is limited to twelve months of fees.",
            "Termination requires thirty days prior written notice.",
        ]);
        let pages = extract_docx_pages(&bytes, "contract.docx").expect("docx extract succeeds");
        assert_eq!(pages.len(), 2);
        assert!(pages[0].contains("Indemnification cap"));
        assert!(pages[1].contains("Termination requires thirty days"));
    }

    #[test]
    fn extract_docx_pages_rejects_oversized_xml_zip_bomb() {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buf);
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            writer
                .start_file("word/document.xml", options)
                .expect("start_file succeeds");
            let chunk = vec![b'x'; 1024 * 1024]; // 1 MiB chunk of compressible data
                                                 // Write 17 MiB, exceeding MAX_DOCX_XML_BYTES (16 MiB)
            for _ in 0..17 {
                writer.write_all(&chunk).expect("write_all succeeds");
            }
            writer.finish().expect("finish succeeds");
        }
        let bytes = buf.into_inner();
        let res = extract_docx_pages(&bytes, "zip_bomb.docx");
        assert!(res.is_err(), "Oversized docx xml must be rejected");
        match res {
            Err(LocalRagError::Extraction { reason, .. }) => {
                assert!(
                    reason.contains("potential zip bomb")
                        || reason.contains("exceeds maximum allowed size"),
                    "Error reason must flag zip bomb: {reason}"
                );
            }
            _ => panic!("Expected LocalRagError::Extraction"),
        }
    }

    #[test]
    fn incremental_sync_skips_unchanged_sha256_and_prunes_deleted() {
        let temp_dir =
            std::env::temp_dir().join(format!("local_rag_test_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("create temp_dir");
        let index_path = temp_dir.join(DEFAULT_INDEX_FILENAME);
        let docx_path = temp_dir.join("policy.docx");
        let docx_bytes = build_test_docx(&[
            "Employee travel reimbursement cap is 180 USD per day for hotel lodging.",
        ]);
        fs::write(&docx_path, &docx_bytes).expect("write docx");

        let engine = EmbeddingEngine::new();
        let mut store = LocalIndexStore::load_or_empty(&index_path).expect("load empty");

        let s1 = store
            .sync_folder(&temp_dir, &index_path, &engine)
            .expect("initial sync");
        assert_eq!(s1.indexed, 1);
        assert_eq!(s1.skipped_unchanged, 0);
        assert!(index_path.exists());

        // Second sync with unchanged file must skip parsing & embedding.
        let s2 = store
            .sync_folder(&temp_dir, &index_path, &engine)
            .expect("second sync");
        assert_eq!(s2.indexed, 0);
        assert_eq!(s2.skipped_unchanged, 1);

        // Retrieval matches local docx chunk and enforces strict schema fields.
        let req = LocalRetrieveRequest {
            query: "hotel travel reimbursement cap".to_string(),
            top_k: 5,
            file_extensions: vec![".pdf".to_string(), ".docx".to_string()],
        };
        let resp = store.retrieve(&req, &engine).expect("retrieve succeeds");
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.indexed_files, 1);
        assert_eq!(resp.chunks.len(), 1);
        assert_eq!(resp.chunks[0].source_file, "policy.docx");
        assert_eq!(resp.chunks[0].file_extension, ".docx");
        assert_eq!(resp.chunks[0].page_number, 1);

        // Extension filter excludes .docx when only .pdf is requested.
        let pdf_only_req = LocalRetrieveRequest {
            query: "hotel travel reimbursement cap".to_string(),
            top_k: 5,
            file_extensions: vec![".pdf".to_string()],
        };
        let pdf_only_resp = store
            .retrieve(&pdf_only_req, &engine)
            .expect("pdf-only retrieve succeeds");
        assert!(pdf_only_resp.chunks.is_empty());

        // Delete file and confirm pruning.
        fs::remove_file(&docx_path).expect("remove docx");
        let s3 = store
            .sync_folder(&temp_dir, &index_path, &engine)
            .expect("prune sync");
        assert_eq!(s3.removed, 1);
        assert_eq!(store.files.len(), 0);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn strict_request_schema_rejects_unknown_fields_and_bad_extensions() {
        let bad_json = r#"{"query":"hi","top_k":3,"unknown_field":1}"#;
        assert!(serde_json::from_str::<LocalRetrieveRequest>(bad_json).is_err());

        let engine = EmbeddingEngine::new();
        let store = LocalIndexStore::default();
        let bad_ext_req = LocalRetrieveRequest {
            query: "test".to_string(),
            top_k: 5,
            file_extensions: vec![".exe".to_string()],
        };
        assert!(store.retrieve(&bad_ext_req, &engine).is_err());
    }
}
