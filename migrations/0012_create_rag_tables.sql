-- Migration: Create RAG Pipeline Tables

CREATE TABLE IF NOT EXISTS rag_documents (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    filename TEXT NOT NULL,
    domain TEXT,
    content_hash TEXT NOT NULL,
    page_count INTEGER,
    chunk_count INTEGER,
    status TEXT NOT NULL DEFAULT 'processing',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS rag_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES rag_documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,
    metadata TEXT,
    token_count INTEGER,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS rag_retrieval_logs (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    history_id TEXT,
    query_text TEXT NOT NULL,
    chunks_retrieved INTEGER,
    top_similarity_score REAL,
    retrieval_time_ms INTEGER,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_rag_documents_user ON rag_documents(user_id);
CREATE INDEX IF NOT EXISTS idx_rag_documents_hash ON rag_documents(user_id, content_hash);
CREATE INDEX IF NOT EXISTS idx_rag_chunks_doc ON rag_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_rag_retrieval_logs_user ON rag_retrieval_logs(user_id);
