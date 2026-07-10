use std::time::Instant;
use tracing::warn;

use super::types::{RetrievalQuery, SearchResult};
use super::store::RagStore;
use super::embeddings::EmbeddingEngine;

/// RAG Retriever Component
pub struct DocumentRetriever {
    store: RagStore,
    embeddings: EmbeddingEngine,
}

impl DocumentRetriever {
    pub fn new(store: RagStore) -> Self {
        Self {
            store,
            embeddings: EmbeddingEngine::new(),
        }
    }

    /// Retrieve the top-K relevant chunks for a user query.
    ///
    /// 1. Embeds the user query text using the local EmbeddingEngine
    /// 2. Performs a cosine-similarity search against candidate document chunks in SQLite
    /// 3. Logs retrieval metrics (time, count, max score) for analytics
    pub async fn retrieve(
        &self,
        query: &RetrievalQuery,
        history_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        let start = Instant::now();

        // Generate query embedding
        let query_vector = self.embeddings.embed(&query.query_text);

        // Perform cosine similarity search in database
        let results = self.store.search(
            &query_vector,
            &query.user_id,
            query.document_ids.as_deref(),
            query.top_k,
            query.min_similarity,
        )
        .await
        .map_err(|e| format!("Database error during vector search: {}", e))?;

        let duration_ms = start.elapsed().as_millis() as i64;
        let chunks_count = results.len() as i64;
        let top_similarity = results.first().map(|r| r.similarity_score).unwrap_or(0.0);

        // Log retrieval event for analysis
        if let Err(e) = self.store.log_retrieval(
            &query.user_id,
            history_id,
            &query.query_text,
            chunks_count,
            top_similarity,
            duration_ms,
        ).await {
            warn!("Failed to write RAG retrieval log: {}", e);
        }

        Ok(results)
    }
}
