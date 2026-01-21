-- Drop unused tables from legacy Prometheus-based metrics system
-- These tables were replaced by the requests table which provides
-- per-request granularity and serves all time-series query needs.

-- Drop unused time-series tables
DROP TABLE IF EXISTS token_usage;
DROP TABLE IF EXISTS hourly_metrics;
DROP TABLE IF EXISTS instance_health;
DROP TABLE IF EXISTS retention_policy;

-- Drop associated indexes (if they still exist)
DROP INDEX IF EXISTS idx_token_usage_timestamp;
DROP INDEX IF EXISTS idx_token_usage_date;
DROP INDEX IF EXISTS idx_token_usage_dimensions;
DROP INDEX IF EXISTS idx_token_usage_api_key;
DROP INDEX IF EXISTS idx_token_usage_instance;

DROP INDEX IF EXISTS idx_instance_health_timestamp;
DROP INDEX IF EXISTS idx_instance_health_date;
DROP INDEX IF EXISTS idx_instance_health_instance;
DROP INDEX IF EXISTS idx_instance_health_status;

DROP INDEX IF EXISTS idx_hourly_metrics_date_hour;
DROP INDEX IF EXISTS idx_hourly_metrics_dimensions;
