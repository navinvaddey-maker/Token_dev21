-- migrations/0007_user_prompts.sql
CREATE TABLE user_prompts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    raw_text    TEXT NOT NULL,
    schema_json JSONB,
    token_count BIGINT,
    created_at  TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);
