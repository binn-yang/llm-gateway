-- Configuration Management Tables
-- This migration adds support for database-based configuration management

-- ============================================================================
-- 1. API Keys Table
-- ============================================================================
-- Stores API keys with SHA256 hash for secure authentication
CREATE TABLE IF NOT EXISTS api_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Key information (secure storage)
    key_hash TEXT NOT NULL UNIQUE,           -- SHA256 hash for verification
    key_prefix TEXT NOT NULL,                -- First 8 chars for display (e.g., "sk-gatew")
    name TEXT NOT NULL UNIQUE,               -- Friendly name
    enabled INTEGER NOT NULL DEFAULT 1,      -- 1=enabled, 0=disabled

    -- Metadata
    description TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    last_used_at INTEGER,                    -- Last authentication timestamp

    -- Soft delete
    deleted_at INTEGER                       -- NULL=active, timestamp=deleted
);

CREATE INDEX idx_api_keys_hash ON api_keys(key_hash) WHERE deleted_at IS NULL;
CREATE INDEX idx_api_keys_enabled ON api_keys(enabled, deleted_at);
CREATE INDEX idx_api_keys_name ON api_keys(name) WHERE deleted_at IS NULL;

-- ============================================================================
-- 2. Routing Rules Table
-- ============================================================================
-- Stores model prefix to provider routing rules
CREATE TABLE IF NOT EXISTS routing_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Rule definition
    prefix TEXT NOT NULL UNIQUE,             -- Model prefix (e.g., "gpt-", "claude-")
    provider TEXT NOT NULL,                  -- Target provider: openai/anthropic/gemini
    priority INTEGER NOT NULL DEFAULT 100,   -- Lower number = higher priority
    enabled INTEGER NOT NULL DEFAULT 1,      -- 1=enabled, 0=disabled

    -- Metadata
    description TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),

    -- Soft delete
    deleted_at INTEGER
);

CREATE INDEX idx_routing_rules_priority ON routing_rules(priority ASC)
    WHERE enabled = 1 AND deleted_at IS NULL;
CREATE INDEX idx_routing_rules_prefix ON routing_rules(prefix) WHERE deleted_at IS NULL;

-- ============================================================================
-- 3. Routing Global Config Table (Singleton)
-- ============================================================================
-- Stores global routing configuration (discovery, default provider, etc.)
CREATE TABLE IF NOT EXISTS routing_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),   -- Enforce singleton

    -- Global routing settings
    default_provider TEXT,                   -- Fallback provider when no rule matches

    -- Discovery settings
    discovery_enabled INTEGER NOT NULL DEFAULT 1,
    discovery_cache_ttl_seconds INTEGER NOT NULL DEFAULT 3600,
    discovery_refresh_on_startup INTEGER NOT NULL DEFAULT 1,
    discovery_providers_with_listing TEXT NOT NULL DEFAULT '["openai"]',  -- JSON array

    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000)
);

-- Insert default row
INSERT OR IGNORE INTO routing_config (id, default_provider)
VALUES (1, 'openai');

-- ============================================================================
-- 4. Provider Instances Table
-- ============================================================================
-- Stores provider instance configurations with load balancing settings
CREATE TABLE IF NOT EXISTS provider_instances (
    id INTEGER PRIMARY KEY AUTOINCREMENT,

    -- Provider identity
    provider TEXT NOT NULL,                  -- openai/anthropic/gemini
    name TEXT NOT NULL,                      -- Instance name (unique within provider)
    enabled INTEGER NOT NULL DEFAULT 1,      -- 1=enabled, 0=disabled

    -- Connection config
    api_key_encrypted TEXT NOT NULL,         -- API key (SHA256 hash)
    base_url TEXT NOT NULL,
    timeout_seconds INTEGER NOT NULL DEFAULT 300,

    -- Load balancing config
    priority INTEGER NOT NULL DEFAULT 1,     -- Lower = higher priority
    weight INTEGER NOT NULL DEFAULT 100,     -- Weight for random selection
    failure_timeout_seconds INTEGER NOT NULL DEFAULT 60,

    -- Provider-specific config (JSON)
    extra_config TEXT,                       -- Anthropic: {"api_version": "...", "cache": {...}}

    -- Metadata
    description TEXT,
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    last_health_check_at INTEGER,
    health_status TEXT DEFAULT 'unknown',    -- unknown/healthy/unhealthy

    -- Soft delete
    deleted_at INTEGER
);

-- Unique constraint: provider + name must be unique for active records
CREATE UNIQUE INDEX idx_provider_instances_unique_name ON provider_instances(provider, name)
    WHERE deleted_at IS NULL;

CREATE INDEX idx_provider_instances_enabled ON provider_instances(provider, enabled, deleted_at);
CREATE INDEX idx_provider_instances_priority ON provider_instances(provider, priority ASC)
    WHERE enabled = 1 AND deleted_at IS NULL;
CREATE INDEX idx_provider_instances_provider_name ON provider_instances(provider, name)
    WHERE deleted_at IS NULL;

-- ============================================================================
-- Trigger: Update updated_at timestamp automatically
-- ============================================================================

-- api_keys
CREATE TRIGGER IF NOT EXISTS update_api_keys_timestamp
AFTER UPDATE ON api_keys
FOR EACH ROW
BEGIN
    UPDATE api_keys SET updated_at = strftime('%s', 'now') * 1000
    WHERE id = NEW.id;
END;

-- routing_rules
CREATE TRIGGER IF NOT EXISTS update_routing_rules_timestamp
AFTER UPDATE ON routing_rules
FOR EACH ROW
BEGIN
    UPDATE routing_rules SET updated_at = strftime('%s', 'now') * 1000
    WHERE id = NEW.id;
END;

-- routing_config
CREATE TRIGGER IF NOT EXISTS update_routing_config_timestamp
AFTER UPDATE ON routing_config
FOR EACH ROW
BEGIN
    UPDATE routing_config SET updated_at = strftime('%s', 'now') * 1000
    WHERE id = NEW.id;
END;

-- provider_instances
CREATE TRIGGER IF NOT EXISTS update_provider_instances_timestamp
AFTER UPDATE ON provider_instances
FOR EACH ROW
BEGIN
    UPDATE provider_instances SET updated_at = strftime('%s', 'now') * 1000
    WHERE id = NEW.id;
END;
