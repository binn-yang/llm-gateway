-- Create pricing_metadata table for storing pricing update metadata
-- This table stores metadata like the last pricing file hash

CREATE TABLE IF NOT EXISTS pricing_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Initialize with empty hash
INSERT OR IGNORE INTO pricing_metadata (key, value, updated_at)
VALUES ('last_pricing_hash', '', strftime('%s', 'now') * 1000);
