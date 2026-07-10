-- migrations/0006_add_version_history.sql
CREATE TABLE engine_version_history (
    id              TEXT PRIMARY KEY,
    version         TEXT NOT NULL,
    description     TEXT NOT NULL,
    breaking_change INTEGER NOT NULL DEFAULT 0,
    migrated_at     DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT INTO engine_version_history (id, version, description)
VALUES (lower(hex(randomblob(16))), '1.0.0', 'Initial engine — CRISP + 6 principles + feedback detector');
