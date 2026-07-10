-- migrations/0005_create_compression_weights.sql
CREATE TABLE compression_weights (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    entity     TEXT NOT NULL,
    weight     DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    use_case   TEXT NOT NULL DEFAULT 'generic',
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, entity, use_case)
);

CREATE INDEX idx_weights_user_id ON compression_weights (user_id);
