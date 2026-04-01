-- migrations/0002_create_token_history.sql
CREATE TABLE token_history (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    original_prompt  TEXT NOT NULL,
    optimized_prompt TEXT NOT NULL,
    tokens_saved     BIGINT NOT NULL DEFAULT 0,
    token_original   BIGINT NOT NULL DEFAULT 0,
    token_final      BIGINT NOT NULL DEFAULT 0,
    use_case         TEXT NOT NULL DEFAULT 'generic',
    mode             TEXT NOT NULL DEFAULT 'balanced',
    engine_version   TEXT NOT NULL DEFAULT '1.0.0',
    principle_logs   JSONB NOT NULL DEFAULT '[]',
    warnings         JSONB NOT NULL DEFAULT '[]',
    created_at       TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_token_history_user_id    ON token_history (user_id);
CREATE INDEX idx_token_history_created_at ON token_history (created_at DESC);
