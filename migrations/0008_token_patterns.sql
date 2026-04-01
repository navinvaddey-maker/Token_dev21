-- migrations/0008_token_patterns.sql
CREATE TABLE token_patterns (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    pattern    TEXT NOT NULL,
    category   TEXT NOT NULL,
    hits       BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);
