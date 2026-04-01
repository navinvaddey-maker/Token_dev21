-- migrations/0010_user_domain_profile.sql
CREATE TABLE user_domain_profile (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cluster_id    BIGINT NOT NULL,
    cluster_label TEXT,
    top_tokens    JSONB NOT NULL DEFAULT '[]',
    hits          BIGINT NOT NULL DEFAULT 0,
    updated_at    TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, cluster_id)
);
