-- ====================================================================
-- LLM Gateway Time-Series Monitoring Tables
-- ====================================================================
-- This migration creates tables for storing Prometheus metrics
-- in SQLite for time-series queries and historical analysis.

-- ====================================================================
-- 1. Token Usage Time Series Table
-- ====================================================================
-- Stores token usage metrics per API key, provider, model, and instance.
-- Minute-level granularity with automatic aggregation to hourly.

CREATE TABLE IF NOT EXISTS token_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Time dimensions
    timestamp INTEGER NOT NULL,  -- Unix millisecond timestamp (minute-aligned)
    date TEXT NOT NULL,          -- YYYY-MM-DD (for date filtering)
    hour INTEGER NOT NULL,       -- 0-23 (for hourly aggregation)

    -- Dimension labels
    api_key TEXT NOT NULL,       -- API key name (not the actual key)
    provider TEXT NOT NULL,      -- openai/anthropic/gemini
    model TEXT NOT NULL,         -- Model identifier
    instance TEXT,               -- Instance name (optional)

    -- Metrics data
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    request_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    error_count INTEGER NOT NULL DEFAULT 0,
    avg_duration_ms INTEGER,     -- Average latency in milliseconds

    -- Metadata
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),

    -- Unique constraint to prevent duplicate inserts
    UNIQUE(timestamp, api_key, provider, model, instance)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_token_usage_timestamp ON token_usage(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_token_usage_date ON token_usage(date DESC);
CREATE INDEX IF NOT EXISTS idx_token_usage_dimensions ON token_usage(date, provider, model);
CREATE INDEX IF NOT EXISTS idx_token_usage_api_key ON token_usage(api_key, date DESC);
CREATE INDEX IF NOT EXISTS idx_token_usage_instance ON token_usage(instance, timestamp DESC);

-- ====================================================================
-- 2. Instance Health Time Series Table
-- ====================================================================
-- Stores health status changes for provider instances.
-- Tracks failover events and recovery patterns.

CREATE TABLE IF NOT EXISTS instance_health (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Time dimensions
    timestamp INTEGER NOT NULL,  -- Unix millisecond timestamp (minute-aligned)
    date TEXT NOT NULL,          -- YYYY-MM-DD
    hour INTEGER NOT NULL,       -- 0-23

    -- Dimension labels
    provider TEXT NOT NULL,      -- openai/anthropic/gemini
    instance TEXT NOT NULL,      -- Instance name

    -- Health status (final status for this minute)
    health_status TEXT NOT NULL, -- 'healthy' / 'unhealthy'

    -- Statistics for this minute
    request_count INTEGER NOT NULL DEFAULT 0,
    success_count INTEGER NOT NULL DEFAULT 0,
    failure_count INTEGER NOT NULL DEFAULT 0,
    failover_count INTEGER NOT NULL DEFAULT 0,  -- Failover events this minute

    -- Metadata
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),

    UNIQUE(timestamp, provider, instance)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_instance_health_timestamp ON instance_health(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_instance_health_date ON instance_health(date DESC);
CREATE INDEX IF NOT EXISTS idx_instance_health_instance ON instance_health(instance, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_instance_health_status ON instance_health(health_status, date);

-- ====================================================================
-- 3. Hourly Aggregated Metrics Table
-- ====================================================================
-- Pre-aggregated hourly metrics for long-term trend analysis.
-- Automatically populated by background aggregation task.

CREATE TABLE IF NOT EXISTS hourly_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Time dimensions
    date TEXT NOT NULL,          -- YYYY-MM-DD
    hour INTEGER NOT NULL,       -- 0-23
    timestamp INTEGER NOT NULL,  -- Start of hour timestamp

    -- Dimension labels
    api_key TEXT NOT NULL,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    instance TEXT,

    -- Aggregated metrics
    total_input_tokens INTEGER NOT NULL DEFAULT 0,
    total_output_tokens INTEGER NOT NULL DEFAULT 0,
    total_requests INTEGER NOT NULL DEFAULT 0,
    total_success INTEGER NOT NULL DEFAULT 0,
    total_errors INTEGER NOT NULL DEFAULT 0,
    p50_duration_ms INTEGER,
    p95_duration_ms INTEGER,
    p99_duration_ms INTEGER,
    max_duration_ms INTEGER,

    UNIQUE(date, hour, api_key, provider, model, instance)
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_hourly_metrics_date_hour ON hourly_metrics(date DESC, hour DESC);
CREATE INDEX IF NOT EXISTS idx_hourly_metrics_dimensions ON hourly_metrics(provider, model, date DESC);

-- ====================================================================
-- 4. Retention Policy Updates
-- ====================================================================
-- Add entries for new tables to retention policy table.

-- Create retention_policy table if it doesn't exist
CREATE TABLE IF NOT EXISTS retention_policy (
    table_name TEXT PRIMARY KEY,
    ttl_days INTEGER NOT NULL,
    last_cleanup INTEGER NOT NULL  -- Unix timestamp in seconds
);

-- Insert retention policies for new tables
INSERT OR REPLACE INTO retention_policy (table_name, ttl_days, last_cleanup) VALUES
('token_usage', 90, strftime('%s', 'now')),
('instance_health', 90, strftime('%s', 'now')),
('hourly_metrics', 365, strftime('%s', 'now'));

-- Note: WAL mode and other PRAGMA settings are configured in the application code
-- when establishing the database connection, not in migrations.
