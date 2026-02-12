-- LLM Gateway Database Schema
-- This is the consolidated initial schema migration

-- requests: Per-request logging with token usage and cost tracking
CREATE TABLE IF NOT EXISTS requests (
    request_id TEXT PRIMARY KEY,
    timestamp INTEGER NOT NULL,
    date TEXT NOT NULL,
    hour INTEGER NOT NULL,
    api_key_name TEXT NOT NULL,
    provider TEXT NOT NULL,
    instance TEXT NOT NULL,
    model TEXT NOT NULL,
    endpoint TEXT NOT NULL,
    status TEXT NOT NULL,
    error_type TEXT,
    error_message TEXT,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    cache_creation_input_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_input_tokens INTEGER NOT NULL DEFAULT 0,
    duration_ms INTEGER NOT NULL,
    input_cost REAL NOT NULL DEFAULT 0.0,
    output_cost REAL NOT NULL DEFAULT 0.0,
    cache_write_cost REAL NOT NULL DEFAULT 0.0,
    cache_read_cost REAL NOT NULL DEFAULT 0.0,
    total_cost REAL NOT NULL DEFAULT 0.0,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
);

CREATE INDEX IF NOT EXISTS idx_requests_timestamp ON requests(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_requests_date ON requests(date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_provider_model ON requests(provider, model, date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_api_key ON requests(api_key_name, date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_instance ON requests(instance, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_requests_status ON requests(status, date);
CREATE INDEX IF NOT EXISTS idx_requests_endpoint ON requests(endpoint, date DESC);
CREATE INDEX IF NOT EXISTS idx_requests_timeseries ON requests(provider, date, hour);
CREATE INDEX IF NOT EXISTS idx_requests_total_cost ON requests(total_cost);
CREATE INDEX IF NOT EXISTS idx_requests_model_cost ON requests(model, total_cost);

-- quota_snapshots: Provider instance quota data
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

CREATE INDEX IF NOT EXISTS idx_quota_snapshots_lookup ON quota_snapshots(provider, instance, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_quota_snapshots_cleanup ON quota_snapshots(created_at);
CREATE INDEX IF NOT EXISTS idx_quota_snapshots_time_range ON quota_snapshots(timestamp DESC);

-- model_prices: LLM model pricing data
CREATE TABLE IF NOT EXISTS model_prices (
    model_name TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    input_price REAL NOT NULL,
    output_price REAL NOT NULL,
    cache_write_price REAL,
    cache_read_price REAL,
    currency TEXT NOT NULL DEFAULT 'USD',
    effective_date TEXT NOT NULL,
    notes TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
);

CREATE INDEX IF NOT EXISTS idx_model_prices_provider ON model_prices(provider);
CREATE INDEX IF NOT EXISTS idx_model_prices_effective_date ON model_prices(effective_date DESC);

-- pricing_metadata: Pricing update metadata
CREATE TABLE IF NOT EXISTS pricing_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT OR IGNORE INTO pricing_metadata (key, value, updated_at)
VALUES ('last_pricing_hash', '', strftime('%s', 'now') * 1000);

-- failover_events: Circuit breaker and failover tracking
CREATE TABLE IF NOT EXISTS failover_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    provider TEXT NOT NULL,
    instance TEXT NOT NULL,
    event_type TEXT NOT NULL,
    failure_type TEXT,
    error_message TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    next_retry_secs INTEGER
);

CREATE INDEX IF NOT EXISTS idx_failover_timestamp ON failover_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_failover_instance ON failover_events(instance, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_failover_provider ON failover_events(provider, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_failover_event_type ON failover_events(event_type, timestamp DESC);
