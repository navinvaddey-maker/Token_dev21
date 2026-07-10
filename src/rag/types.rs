use serde::{Deserialize, Serialize};

// ── Document ───────────────────────────────────────────────────────────────

/// Represents an uploaded PDF document in the RAG system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocument {
    pub id: String,
    pub user_id: String,
    pub filename: String,
    pub domain: Option<String>,
    pub content_hash: String,
    pub page_count: Option<i64>,
    pub chunk_count: Option<i64>,
    pub status: DocumentStatus,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Processing,
    Ready,
    Failed,
}

impl DocumentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Processing => "processing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            _ => Self::Processing,
        }
    }
}

// ── Chunk ──────────────────────────────────────────────────────────────────

/// A chunk of text extracted from a document, with its embedding vector.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagChunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: i64,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: Option<ChunkMetadata>,
    pub token_count: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Metadata attached to each chunk for provenance tracking.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChunkMetadata {
    pub page_number: Option<i64>,
    pub section_title: Option<String>,
    pub char_offset_start: Option<usize>,
    pub char_offset_end: Option<usize>,
}

// ── Embedding ──────────────────────────────────────────────────────────────

/// The dimensionality of embeddings produced by the local model.
/// Using 384 for MiniLM-L6-v2 compatibility.
pub const EMBEDDING_DIM: usize = 384;

/// Serializes a Vec<f32> embedding to bytes for SQLite BLOB storage.
pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|f| f.to_le_bytes())
        .collect()
}

/// Deserializes bytes from SQLite BLOB back to Vec<f32>.
pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().expect("chunk must be 4 bytes");
            f32::from_le_bytes(arr)
        })
        .collect()
}

// ── Search ─────────────────────────────────────────────────────────────────

/// Result of a RAG retrieval query — a chunk with its similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk: RagChunk,
    pub similarity_score: f32,
    pub document_filename: String,
    pub document_domain: Option<String>,
}

/// Parameters for a RAG retrieval query.
#[derive(Debug, Clone)]
pub struct RetrievalQuery {
    pub query_text: String,
    pub user_id: String,
    pub document_ids: Option<Vec<String>>,
    pub top_k: usize,
    pub min_similarity: f32,
}

impl Default for RetrievalQuery {
    fn default() -> Self {
        Self {
            query_text: String::new(),
            user_id: String::new(),
            document_ids: None,
            top_k: 5,
            min_similarity: 0.3,
        }
    }
}

// ── Ingestion ──────────────────────────────────────────────────────────────

/// Configuration for the PDF chunking strategy.
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    pub chunk_size_tokens: usize,
    pub overlap_tokens: usize,
    pub max_pages: usize,
    pub max_file_size_bytes: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            chunk_size_tokens: 512,
            overlap_tokens: 64,
            max_pages: 500,
            max_file_size_bytes: 50 * 1024 * 1024, // 50 MB
        }
    }
}

/// Result of the ingestion pipeline for a single document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionResult {
    pub document_id: String,
    pub chunks_created: usize,
    pub detected_domain: Option<String>,
    pub page_count: usize,
    pub status: DocumentStatus,
    pub error: Option<String>,
}

// ── API Request/Response types ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct RagSearchQuery {
    pub query: String,
    pub top_k: Option<usize>,
    pub document_ids: Option<String>, // comma-separated
}

#[derive(Debug, Serialize)]
pub struct RagUploadResponse {
    pub document_id: String,
    pub filename: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct RagDocumentDetail {
    pub document: RagDocument,
    pub chunk_count: i64,
}
