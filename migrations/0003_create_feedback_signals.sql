-- migrations/0003_create_feedback_signals.sql
CREATE TABLE feedback_signals (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    history_id   TEXT NOT NULL REFERENCES token_history(id) ON DELETE CASCADE,
    signal_type  TEXT NOT NULL,
    signal_layer INTEGER NOT NULL,
    value        DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    meta         TEXT NOT NULL DEFAULT '{}',
    detected_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_feedback_user_id     ON feedback_signals (user_id);
CREATE INDEX idx_feedback_signal_type ON feedback_signals (signal_type);
