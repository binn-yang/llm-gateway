-- Create model_prices table for storing pricing data
-- This table stores pricing information for different LLM models

CREATE TABLE IF NOT EXISTS model_prices (
    -- Primary key
    model_name TEXT PRIMARY KEY,

    -- Provider information
    provider TEXT NOT NULL,  -- openai/anthropic/gemini

    -- Pricing (USD per 1M tokens)
    input_price REAL NOT NULL,
    output_price REAL NOT NULL,
    cache_write_price REAL,  -- Optional, for Anthropic prompt caching
    cache_read_price REAL,   -- Optional, for Anthropic prompt caching

    -- Metadata
    currency TEXT NOT NULL DEFAULT 'USD',
    effective_date TEXT NOT NULL,  -- ISO 8601 format
    notes TEXT,

    -- Timestamps
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
);

-- Indexes for efficient queries
CREATE INDEX IF NOT EXISTS idx_model_prices_provider ON model_prices(provider);
CREATE INDEX IF NOT EXISTS idx_model_prices_effective_date ON model_prices(effective_date DESC);
