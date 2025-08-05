-- Add optional provider-specific JSON config columns to tools
-- Stored as TEXT (JSON payload). Safe to run on existing data.

ALTER TABLE tools ADD COLUMN mcp TEXT;
ALTER TABLE tools ADD COLUMN restate TEXT;

-- Note: SQLite/D1 stores JSON as TEXT. These columns are optional and default to NULL.
