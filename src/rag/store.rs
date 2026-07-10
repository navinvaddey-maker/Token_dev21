use sqlx::SqlitePool;
use uuid::Uuid;

use super::embeddings::cosine_similarity;
use super::types::*;

/// SQLite-backed vector store for RAG document chunks.
///
/// Stores embeddings as BLOBs and performs cosine similarity search in Rust
/// (not SQL) for correctness and flexibility. At <10K chunks this is fast enough
/// (sub-100ms). For larger corpora, swap in a dedicated vector index (e.g., usearch).
#[derive(Clone)]
pub struct RagStore {
    pool: SqlitePool,
}

impl RagStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> SqlitePool {
        self.pool.clone()
    }

    // ── Document CRUD ──────────────────────────────────────────────────────

    /// Insert a new document record (status = "processing").
    pub async fn insert_document(&self, doc: &RagDocument) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO rag_documents (id, user_id, filename, domain, content_hash, page_count, chunk_count, status, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(&doc.id)
        .bind(&doc.user_id)
        .bind(&doc.filename)
        .bind(&doc.domain)
        .bind(&doc.content_hash)
        .bind(doc.page_count)
        .bind(doc.chunk_count)
        .bind(doc.status.as_str())
        .bind(doc.created_at)
        .bind(doc.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Update document status and chunk_count after ingestion completes.
    pub async fn update_document_status(
        &self,
        doc_id: &str,
        status: DocumentStatus,
        chunk_count: i64,
        page_count: i64,
        domain: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE rag_documents
               SET status = $1, chunk_count = $2, page_count = $3, domain = $4, updated_at = $5
               WHERE id = $6"#,
        )
        .bind(status.as_str())
        .bind(chunk_count)
        .bind(page_count)
        .bind(domain)
        .bind(chrono::Utc::now())
        .bind(doc_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get a single document by ID, scoped to user.
    pub async fn get_document(
        &self,
        user_id: &str,
        doc_id: &str,
    ) -> Result<Option<RagDocument>, sqlx::Error> {
        let row = sqlx::query_as::<_, RagDocumentRow>(
            "SELECT id, user_id, filename, domain, content_hash, page_count, chunk_count, status, created_at, updated_at
             FROM rag_documents WHERE id = $1 AND user_id = $2",
        )
        .bind(doc_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into()))
    }

    /// List all documents for a user.
    pub async fn list_documents(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RagDocument>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RagDocumentRow>(
            "SELECT id, user_id, filename, domain, content_hash, page_count, chunk_count, status, created_at, updated_at
             FROM rag_documents WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Delete a document and all its chunks (CASCADE in schema).
    pub async fn delete_document(&self, user_id: &str, doc_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM rag_documents WHERE id = $1 AND user_id = $2",
        )
        .bind(doc_id)
        .bind(user_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Admin: delete any document regardless of owning user.
    pub async fn admin_delete_document(&self, doc_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM rag_documents WHERE id = $1")
            .bind(doc_id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Admin: list documents across all users, newest first.
    pub async fn list_all_documents(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<RagDocument>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RagDocumentRow>(
            "SELECT id, user_id, filename, domain, content_hash, page_count, chunk_count, status, created_at, updated_at
             FROM rag_documents ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Check if a document with this content hash already exists for the user.
    pub async fn find_by_content_hash(
        &self,
        user_id: &str,
        content_hash: &str,
    ) -> Result<Option<RagDocument>, sqlx::Error> {
        let row = sqlx::query_as::<_, RagDocumentRow>(
            "SELECT id, user_id, filename, domain, content_hash, page_count, chunk_count, status, created_at, updated_at
             FROM rag_documents WHERE user_id = $1 AND content_hash = $2",
        )
        .bind(user_id)
        .bind(content_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.into()))
    }

    // ── Chunk CRUD ─────────────────────────────────────────────────────────

    /// Insert a batch of chunks for a document.
    pub async fn insert_chunks(&self, chunks: &[RagChunk]) -> Result<(), sqlx::Error> {
        for chunk in chunks {
            let embedding_blob = chunk.embedding.as_ref().map(|e| embedding_to_bytes(e));
            let metadata_json = chunk
                .metadata
                .as_ref()
                .map(|m| serde_json::to_string(m).unwrap_or_default());

            sqlx::query(
                r#"INSERT INTO rag_chunks (id, document_id, chunk_index, content, embedding, metadata, token_count, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            )
            .bind(&chunk.id)
            .bind(&chunk.document_id)
            .bind(chunk.chunk_index)
            .bind(&chunk.content)
            .bind(&embedding_blob)
            .bind(&metadata_json)
            .bind(chunk.token_count)
            .bind(chunk.created_at)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Get chunk count for a document.
    pub async fn get_chunk_count(&self, doc_id: &str) -> Result<i64, sqlx::Error> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM rag_chunks WHERE document_id = $1",
        )
        .bind(doc_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    // ── Vector Search ──────────────────────────────────────────────────────

    /// Retrieve the top-K most similar chunks to the query embedding.
    ///
    /// Strategy: load all candidate chunk embeddings into memory and compute
    /// cosine similarity in Rust. This is optimal for <10K chunks per user.
    /// For larger scale, introduce an ANN index (usearch, hnsw).
    pub async fn search(
        &self,
        query_embedding: &[f32],
        user_id: &str,
        document_ids: Option<&[String]>,
        top_k: usize,
        min_similarity: f32,
    ) -> Result<Vec<SearchResult>, sqlx::Error> {
        // Load candidate chunks with their embeddings + document metadata
        let rows = if let Some(doc_ids) = document_ids {
            if doc_ids.is_empty() {
                return Ok(vec![]);
            }
            // Build dynamic IN clause
            let placeholders: Vec<String> = (0..doc_ids.len()).map(|i| format!("${}", i + 2)).collect();
            let in_clause = placeholders.join(", ");
            let query_str = format!(
                "SELECT c.id, c.document_id, c.chunk_index, c.content, c.embedding, c.metadata, c.token_count, c.created_at,
                        d.filename, d.domain
                 FROM rag_chunks c
                 JOIN rag_documents d ON c.document_id = d.id
                 WHERE d.user_id = $1 AND d.id IN ({}) AND d.status = 'ready' AND c.embedding IS NOT NULL",
                in_clause
            );
            let mut q = sqlx::query_as::<_, ChunkSearchRow>(&query_str).bind(user_id);
            for id in doc_ids {
                q = q.bind(id);
            }
            q.fetch_all(&self.pool).await?
        } else {
            sqlx::query_as::<_, ChunkSearchRow>(
                "SELECT c.id, c.document_id, c.chunk_index, c.content, c.embedding, c.metadata, c.token_count, c.created_at,
                        d.filename, d.domain
                 FROM rag_chunks c
                 JOIN rag_documents d ON c.document_id = d.id
                 WHERE d.user_id = $1 AND d.status = 'ready' AND c.embedding IS NOT NULL",
            )
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?
        };

        // Compute cosine similarity and rank
        let mut results: Vec<SearchResult> = rows
            .into_iter()
            .filter_map(|row| {
                let emb_bytes = row.embedding?;
                let chunk_embedding = bytes_to_embedding(&emb_bytes);
                let score = cosine_similarity(query_embedding, &chunk_embedding);
                if score < min_similarity {
                    return None;
                }

                let metadata: Option<ChunkMetadata> = row.metadata.as_ref().and_then(|m| {
                    serde_json::from_str(m).ok()
                });

                Some(SearchResult {
                    chunk: RagChunk {
                        id: row.id,
                        document_id: row.document_id,
                        chunk_index: row.chunk_index,
                        content: row.content,
                        embedding: None, // Don't return embedding in search results
                        metadata,
                        token_count: row.token_count,
                        created_at: row.created_at,
                    },
                    similarity_score: score,
                    document_filename: row.filename,
                    document_domain: row.domain,
                })
            })
            .collect();

        // Sort by similarity descending, take top_k
        results.sort_by(|a, b| b.similarity_score.partial_cmp(&a.similarity_score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    // ── Retrieval Logging ──────────────────────────────────────────────────

    /// Log a retrieval event for analytics.
    pub async fn log_retrieval(
        &self,
        user_id: &str,
        history_id: Option<&str>,
        query_text: &str,
        chunks_retrieved: i64,
        top_similarity: f32,
        retrieval_time_ms: i64,
    ) -> Result<(), sqlx::Error> {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"INSERT INTO rag_retrieval_logs (id, user_id, history_id, query_text, chunks_retrieved, top_similarity_score, retrieval_time_ms, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(&id)
        .bind(user_id)
        .bind(history_id)
        .bind(query_text)
        .bind(chunks_retrieved)
        .bind(top_similarity)
        .bind(retrieval_time_ms)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

// ── SQLx Row Mapping Types ─────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct RagDocumentRow {
    id: String,
    user_id: String,
    filename: String,
    domain: Option<String>,
    content_hash: String,
    page_count: Option<i64>,
    chunk_count: Option<i64>,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<RagDocumentRow> for RagDocument {
    fn from(row: RagDocumentRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            filename: row.filename,
            domain: row.domain,
            content_hash: row.content_hash,
            page_count: row.page_count,
            chunk_count: row.chunk_count,
            status: DocumentStatus::from_str(&row.status),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct ChunkSearchRow {
    id: String,
    document_id: String,
    chunk_index: i64,
    content: String,
    embedding: Option<Vec<u8>>,
    metadata: Option<String>,
    token_count: Option<i64>,
    created_at: chrono::DateTime<chrono::Utc>,
    filename: String,
    domain: Option<String>,
}
