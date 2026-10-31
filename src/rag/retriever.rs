use std::time::Instant;
use tracing::warn;

use super::embeddings::EmbeddingEngine;
use super::store::RagStore;
use super::types::{RetrievalQuery, SearchResult};

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
        self.retrieve_filtered(query, history_id, &|_: &SearchResult| true)
            .await
    }

    /// Retrieve top-K relevant chunks matching query and passing a custom filter predicate.
    ///
    /// The predicate is applied before truncation and sorting, preventing out-of-namespace chunks
    /// from crowding out valid documents.
    pub async fn retrieve_filtered<F>(
        &self,
        query: &RetrievalQuery,
        history_id: Option<&str>,
        filter: &F,
    ) -> Result<Vec<SearchResult>, String>
    where
        F: Fn(&SearchResult) -> bool + Sync,
    {
        let start = Instant::now();

        // ARCH: §6 Async Rules — CPU-bound ONNX neural inference must not run on async Tokio worker threads.
        let embeddings_engine = self.embeddings;
        let query_text_cloned = query.query_text.clone();
        let query_vector =
            tokio::task::spawn_blocking(move || embeddings_engine.embed(&query_text_cloned))
                .await
                .map_err(|e| format!("Worker thread error generating query embedding: {}", e))?;

        // Perform hybrid cosine similarity + BM25 search in database with pre-truncation filtering
        let results = self
            .store
            .search_hybrid_filtered(
                &query.query_text,
                &query_vector,
                &query.user_id,
                query.document_ids.as_deref(),
                query.top_k,
                query.min_similarity,
                filter,
            )
            .await
            .map_err(|e| format!("Database error during hybrid search: {}", e))?;

        let duration_ms = start.elapsed().as_millis() as i64;
        let chunks_count = results.len() as i64;
        let top_similarity = results.first().map(|r| r.similarity_score).unwrap_or(0.0);

        // Log retrieval event for analysis
        if let Err(e) = self
            .store
            .log_retrieval(
                &query.user_id,
                history_id,
                &query.query_text,
                chunks_count,
                top_similarity,
                duration_ms,
            )
            .await
        {
            warn!("Failed to write RAG retrieval log: {}", e);
        }

        Ok(results)
    }
}
