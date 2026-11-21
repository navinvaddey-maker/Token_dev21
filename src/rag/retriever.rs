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

    /// Retrieve the top-K relevant chunks for a user query under default tenant.
    pub async fn retrieve(
        &self,
        query: &RetrievalQuery,
        history_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        self.retrieve_scoped("default", query, history_id).await
    }

    /// Retrieve the top-K relevant chunks for a user query scoped to `tenant_id`.
    ///
    /// # Errors
    /// Returns `Err` if embedding generation or database search fails.
    pub async fn retrieve_scoped(
        &self,
        tenant_id: &str,
        query: &RetrievalQuery,
        history_id: Option<&str>,
    ) -> Result<Vec<SearchResult>, String> {
        self.retrieve_filtered_scoped(tenant_id, query, history_id, &|_: &SearchResult| true)
            .await
    }

    /// Retrieve top-K relevant chunks matching query and passing a custom filter predicate.
    pub async fn retrieve_filtered<F>(
        &self,
        query: &RetrievalQuery,
        history_id: Option<&str>,
        filter: &F,
    ) -> Result<Vec<SearchResult>, String>
    where
        F: Fn(&SearchResult) -> bool + Sync,
    {
        self.retrieve_filtered_scoped("default", query, history_id, filter)
            .await
    }

    /// Retrieve top-K relevant chunks scoped to `tenant_id` and matching `filter`.
    ///
    /// The predicate is applied before truncation and sorting, preventing out-of-namespace chunks
    /// from crowding out valid documents.
    ///
    /// # Errors
    /// Returns `Err` if query embedding or hybrid database retrieval fails.
    pub async fn retrieve_filtered_scoped<F>(
        &self,
        tenant_id: &str,
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
        let results = match self
            .store
            .search_hybrid_filtered_scoped(
                tenant_id,
                &query.query_text,
                &query_vector,
                &query.user_id,
                query.document_ids.as_deref(),
                query.top_k,
                query.min_similarity,
                filter,
            )
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(tenant_id = %tenant_id, error = %e, "Hybrid RAG search failed");
                return Err("Retrieval storage error".to_string());
            }
        };

        let duration_ms = start.elapsed().as_millis() as i64;
        let chunks_count = results.len() as i64;
        let top_similarity = results.first().map(|r| r.similarity_score).unwrap_or(0.0);

        // Log retrieval event for analysis
        if let Err(e) = self
            .store
            .log_retrieval_scoped(
                tenant_id,
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
