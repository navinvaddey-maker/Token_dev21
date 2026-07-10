-- migrations/0007_user_prompts.sql
CREATE TABLE user_prompts (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    raw_text    TEXT NOT NULL,
    schema_json TEXT,
    token_count INTEGER,
    created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
