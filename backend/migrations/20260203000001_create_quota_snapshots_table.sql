-- Create quota_snapshots table for storing provider instance quota data
CREATE TABLE IF NOT EXISTS quota_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    instance TEXT NOT NULL,
    auth_mode TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('success', 'error', 'unavailable')),
    error_message TEXT,
    quota_data TEXT NOT NULL,
    created_at INTEGER NOT NULL,

    UNIQUE(provider, instance, timestamp)
);

-- Index for looking up latest snapshots by provider and instance
CREATE INDEX IF NOT EXISTS idx_quota_snapshots_lookup
    ON quota_snapshots(provider, instance, timestamp DESC);

-- Index for cleanup queries by created_at
CREATE INDEX IF NOT EXISTS idx_quota_snapshots_cleanup
    ON quota_snapshots(created_at);

-- Index for time-range queries
CREATE INDEX IF NOT EXISTS idx_quota_snapshots_time_range
    ON quota_snapshots(timestamp DESC);
