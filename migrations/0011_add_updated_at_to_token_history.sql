-- Add updated_at to token_history
ALTER TABLE token_history ADD COLUMN updated_at DATETIME DEFAULT CURRENT_TIMESTAMP;

-- Create a trigger to auto-update the updated_at column
CREATE TRIGGER IF NOT EXISTS trg_update_token_history_timestamp
AFTER UPDATE ON token_history
FOR EACH ROW
BEGIN
    UPDATE token_history SET updated_at = CURRENT_TIMESTAMP WHERE id = NEW.id;
END;
