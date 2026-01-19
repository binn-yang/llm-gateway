-- Create requests table for detailed request-level logging
-- This replaces Prometheus metrics with SQLite-based storage

CREATE TABLE IF NOT EXISTS requests (
    -- Primary key and identifiers
    request_id TEXT PRIMARY KEY,              -- UUID for each request

    -- Time dimensions (for efficient time-range queries)
    timestamp INTEGER NOT NULL,               -- Unix millisecond timestamp (request start)
    date TEXT NOT NULL,                       -- YYYY-MM-DD (date partition)
    hour INTEGER NOT NULL,                    -- 0-23 (hour partition)

    -- Request metadata
    api_key_name TEXT NOT NULL,               -- API key friendly name
    provider TEXT NOT NULL,                   -- openai/anthropic/gemini
    instance TEXT NOT NULL,                   -- Instance name from load balancer
    model TEXT NOT NULL,                      -- Model identifier
    endpoint TEXT NOT NULL,                   -- /v1/chat/completions or /v1/messages

    -- Request outcome
    status TEXT NOT NULL,                     -- success / failure / business_error / timeout
    error_type TEXT,                          -- Error category if failed (e.g., "rate_limit", "auth")
    error_message TEXT,                       -- Error message (truncated to 255 chars)

    -- Token usage (0 if streaming/failure)
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,

    -- Performance metrics
    duration_ms INTEGER NOT NULL,             -- Request duration in milliseconds

    -- Metadata
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
);

-- Critical indexes for query performance
CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON requests(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_requests_date ON requests(date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_provider_model ON requests(provider, model, date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_api_key ON requests(api_key_name, date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_instance ON requests(instance, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_requests_status ON requests(status, date);
CREATE INDEX IF NOT EXISTS idx_requests_endpoint ON requests(endpoint, date DESC);

-- Composite index for common time-series queries (provider + date + hour)
CREATE INDEX IF NOT EXISTS idx_requests_timeseries ON requests(provider, date, hour);

-- Update retention policy
INSERT OR REPLACE INTO retention_policy (table_name, ttl_days, last_cleanup) VALUES
('requests', 90, strftime('%s', 'now'));
