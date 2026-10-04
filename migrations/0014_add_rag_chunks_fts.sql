-- Migration 0014: Add SQLite FTS5 for RAG Chunks Lexical BM25 Search
CREATE VIRTUAL TABLE IF NOT EXISTS rag_chunks_fts USING fts5(
    chunk_id UNINDEXED,
    document_id UNINDEXED,
    content,
    tokenize = 'porter unicode61'
);

-- Backfill any existing chunks
INSERT INTO rag_chunks_fts(chunk_id, document_id, content)
SELECT id, document_id, content FROM rag_chunks
WHERE id NOT IN (SELECT chunk_id FROM rag_chunks_fts);

-- Trigger: keep FTS5 synchronized on chunk insert
CREATE TRIGGER IF NOT EXISTS trg_rag_chunks_fts_ai AFTER INSERT ON rag_chunks
BEGIN
    INSERT INTO rag_chunks_fts(chunk_id, document_id, content)
    VALUES (new.id, new.document_id, new.content);
END;

-- Trigger: keep FTS5 synchronized on chunk delete
CREATE TRIGGER IF NOT EXISTS trg_rag_chunks_fts_ad AFTER DELETE ON rag_chunks
BEGIN
    DELETE FROM rag_chunks_fts WHERE chunk_id = old.id;
END;

-- Trigger: keep FTS5 synchronized on chunk content update
CREATE TRIGGER IF NOT EXISTS trg_rag_chunks_fts_au AFTER UPDATE OF content ON rag_chunks
BEGIN
    DELETE FROM rag_chunks_fts WHERE chunk_id = old.id;
    INSERT INTO rag_chunks_fts(chunk_id, document_id, content)
    VALUES (new.id, new.document_id, new.content);
END;
