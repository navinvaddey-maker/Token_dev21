//! Unified Deterministic Embedding Engine
//! 
//! Projects text into a 384-dimensional vector space using deterministic FNV-1a feature hashing
//! and TF-IDF term weighting. This engine is shared across RAG document storage/retrieval,
//! ORY semantic classification, and prompt reconstruction to ensure vector index compatibility.

pub use crate::rag::types::EMBEDDING_DIM;

/// Projects a string into a normalized 384-dimensional vector using the shared `EmbeddingEngine`.
/// Guaranteed cross-session determinism, zero external model dependencies, and 100% vector space compatibility.
pub fn embed_text(text: &str) -> Vec<f32> {
    crate::rag::embeddings::EmbeddingEngine::new().embed(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embed_text_dim_and_determinism() {
        let v1 = embed_text("software backend development");
        let v2 = embed_text("software backend development");
        assert_eq!(v1.len(), EMBEDDING_DIM);
        assert_eq!(v1, v2);
    }
}
