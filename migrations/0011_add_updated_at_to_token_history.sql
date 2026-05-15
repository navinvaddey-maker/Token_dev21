-- Add updated_at to token_history
ALTER TABLE token_history ADD COLUMN updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP;

-- Create a trigger to auto-update the updated_at column
CREATE OR REPLACE FUNCTION update_token_history_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER trg_update_token_history_timestamp
BEFORE UPDATE ON token_history
FOR EACH ROW
EXECUTE FUNCTION update_token_history_timestamp();
