-- migrations/0010_user_domain_profile.sql
CREATE TABLE user_domain_profile (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    cluster_id    INTEGER NOT NULL,
    cluster_label TEXT,
    top_tokens    TEXT NOT NULL DEFAULT '[]',
    hits          INTEGER NOT NULL DEFAULT 0,
    updated_at    DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, cluster_id)
);
