-- Migration 0015: Add Embedding Model Metadata and Tracking
-- ARCH: §8 Vector DB Rules — Embedding model and version must be tracked per row.
-- Prevents silent mixing of vectors from disparate vector spaces (e.g. hash vs neural).

ALTER TABLE rag_chunks ADD COLUMN embedding_model TEXT NOT NULL DEFAULT 'all-MiniLM-L6-v2';
ALTER TABLE rag_chunks ADD COLUMN model_version TEXT NOT NULL DEFAULT '1.0';

ALTER TABLE scenario_index ADD COLUMN embedding_model TEXT NOT NULL DEFAULT 'all-MiniLM-L6-v2';
ALTER TABLE scenario_index ADD COLUMN model_version TEXT NOT NULL DEFAULT '1.0';

-- Indexes to accelerate model-filtered retrieval and re-embedding sweeps
CREATE INDEX IF NOT EXISTS idx_rag_chunks_model ON rag_chunks(embedding_model, model_version);
CREATE INDEX IF NOT EXISTS idx_scenario_index_model ON scenario_index(embedding_model, model_version);
