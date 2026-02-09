-- Create failover_events table for tracking circuit breaker events
-- This table records instance failures, recoveries, and circuit state transitions

CREATE TABLE IF NOT EXISTS failover_events (
    -- Primary key
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Time dimension
    timestamp TEXT NOT NULL,                  -- ISO 8601 timestamp (YYYY-MM-DDTHH:MM:SS.sssZ)

    -- Instance metadata
    provider TEXT NOT NULL,                   -- Provider name (openai/anthropic/gemini)
    instance TEXT NOT NULL,                   -- Instance name

    -- Event information
    event_type TEXT NOT NULL,                 -- 'failure', 'recovery', 'circuit_open', 'circuit_half_open', 'circuit_closed'
    failure_type TEXT,                        -- 'rate_limit', 'transient', 'instance_failure', 'business_error' (nullable)
    error_message TEXT,                       -- Error message (nullable, truncated to 500 chars)

    -- Circuit breaker state
    consecutive_failures INTEGER NOT NULL DEFAULT 0,  -- Current consecutive failure count
    next_retry_secs INTEGER                   -- Seconds until next retry (nullable)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_failover_timestamp ON failover_events(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_failover_instance ON failover_events(instance, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_failover_provider ON failover_events(provider, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_failover_event_type ON failover_events(event_type, timestamp DESC);
