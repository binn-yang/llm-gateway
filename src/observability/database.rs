//! SQLite database layer for observability data
//!
//! This module provides async database operations with:
//! - Connection pooling
//! - Automatic migrations
//! - Batch inserts for performance
//! - WAL mode for concurrent reads/writes

use crate::observability::span::SpanRecord;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::str::FromStr;
use std::time::Duration;

/// Log entry for database storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: u64,          // Unix milliseconds
    pub level: String,           // ERROR/WARN/INFO/DEBUG/TRACE
    pub target: String,          // Rust module path
    pub message: String,
    pub request_id: Option<String>,
    pub span_id: Option<String>,
    pub fields: String,          // JSON
}

/// Observability database handle
///
/// Manages SQLite connection pool and provides CRUD operations.
pub struct ObservabilityDb {
    pool: SqlitePool,
}

impl ObservabilityDb {
    /// Create a new database connection with automatic migration
    ///
    /// # Arguments
    ///
    /// * `database_url` - SQLite database file path (e.g., "sqlite:./data/observability.db")
    ///
    /// # Example
    ///
    /// ```ignore
    /// let db = ObservabilityDb::new("sqlite:./data/observability.db").await?;
    /// ```
    pub async fn new(database_url: &str) -> Result<Self> {
        // Parse connection options
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)  // Write-Ahead Logging for concurrency
            .busy_timeout(Duration::from_secs(30))  // Wait up to 30s for locks
            .pragma("cache_size", "-64000")        // 64MB cache
            .pragma("temp_store", "memory")        // Use memory for temp tables
            .pragma("synchronous", "NORMAL")       // Balance safety/performance
            .pragma("mmap_size", "30000000000");   // 30GB memory-mapped I/O

        // Create connection pool
        let pool = SqlitePoolOptions::new()
            .max_connections(5)  // Limited for SQLite (single writer)
            .acquire_timeout(Duration::from_secs(30))
            .connect_with(options)
            .await
            .context("Failed to connect to observability database")?;

        // Run migrations
        Self::run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    /// Run database migrations
    async fn run_migrations(pool: &SqlitePool) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(pool)
            .await
            .context("Failed to run observability database migrations")?;

        tracing::info!("Observability database migrations completed");
        Ok(())
    }

    /// Insert a single log entry
    ///
    /// Note: For performance, prefer `insert_logs_batch()` when inserting multiple logs.
    pub async fn insert_log(&self, log: &LogEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO logs (timestamp, level, target, message, request_id, span_id, fields)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(log.timestamp as i64)
        .bind(&log.level)
        .bind(&log.target)
        .bind(&log.message)
        .bind(&log.request_id)
        .bind(&log.span_id)
        .bind(&log.fields)
        .execute(&self.pool)
        .await
        .context("Failed to insert log entry")?;

        Ok(())
    }

    /// Insert multiple log entries in a single transaction (batch insert)
    ///
    /// This is ~50x faster than individual inserts due to transaction overhead.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let logs = vec![log1, log2, log3];
    /// db.insert_logs_batch(&logs).await?;
    /// ```
    pub async fn insert_logs_batch(&self, logs: &[LogEntry]) -> Result<()> {
        if logs.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for log in logs {
            sqlx::query(
                "INSERT INTO logs (timestamp, level, target, message, request_id, span_id, fields)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(log.timestamp as i64)
            .bind(&log.level)
            .bind(&log.target)
            .bind(&log.message)
            .bind(&log.request_id)
            .bind(&log.span_id)
            .bind(&log.fields)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(())
    }

    /// Insert a span record
    pub async fn insert_span(&self, span: &SpanRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO spans (span_id, parent_span_id, request_id, name, kind, start_time, end_time, duration_ms, status, attributes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(&span.span_id)
        .bind(&span.parent_span_id)
        .bind(&span.request_id)
        .bind(&span.name)
        .bind(&span.kind)
        .bind(span.start_time as i64)
        .bind(span.end_time.map(|t| t as i64))
        .bind(span.duration_ms.map(|d| d as i64))
        .bind(&span.status)
        .bind(&span.attributes)
        .execute(&self.pool)
        .await
        .context("Failed to insert span")?;

        Ok(())
    }

    /// Insert metrics snapshot
    ///
    /// Stores a snapshot of current metrics for historical analysis.
    pub async fn insert_metrics_snapshot(&self, metrics: &serde_json::Value) -> Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64;

        let metrics_json = serde_json::to_string(metrics)?;

        sqlx::query(
            "INSERT INTO metrics_snapshots (timestamp, metrics)
             VALUES (?, ?)
             ON CONFLICT(timestamp) DO UPDATE SET metrics = excluded.metrics"
        )
        .bind(timestamp)
        .bind(&metrics_json)
        .execute(&self.pool)
        .await
        .context("Failed to insert metrics snapshot")?;

        Ok(())
    }

    /// Query logs by request ID
    ///
    /// Returns all logs associated with a specific request.
    pub async fn query_logs_by_request(&self, request_id: &str) -> Result<Vec<LogEntry>> {
        let rows = sqlx::query(
            "SELECT timestamp, level, target, message, request_id, span_id, fields
             FROM logs
             WHERE request_id = ?
             ORDER BY timestamp ASC"
        )
        .bind(request_id)
        .fetch_all(&self.pool)
        .await?;

        let logs = rows
            .into_iter()
            .map(|row| LogEntry {
                timestamp: row.get::<i64, _>("timestamp") as u64,
                level: row.get("level"),
                target: row.get("target"),
                message: row.get("message"),
                request_id: row.get("request_id"),
                span_id: row.get("span_id"),
                fields: row.get("fields"),
            })
            .collect();

        Ok(logs)
    }

    /// Query spans by request ID
    ///
    /// Returns all spans (including nested hierarchy) for a request.
    pub async fn query_spans_by_request(&self, request_id: &str) -> Result<Vec<SpanRecord>> {
        let rows = sqlx::query(
            "SELECT span_id, parent_span_id, request_id, name, kind, start_time, end_time, duration_ms, status, attributes
             FROM spans
             WHERE request_id = ?
             ORDER BY start_time ASC"
        )
        .bind(request_id)
        .fetch_all(&self.pool)
        .await?;

        let spans = rows
            .into_iter()
            .map(|row| SpanRecord {
                span_id: row.get("span_id"),
                parent_span_id: row.get("parent_span_id"),
                request_id: row.get("request_id"),
                name: row.get("name"),
                kind: row.get("kind"),
                start_time: row.get::<i64, _>("start_time") as u64,
                end_time: row.get::<Option<i64>, _>("end_time").map(|t| t as u64),
                duration_ms: row.get::<Option<i64>, _>("duration_ms").map(|d| d as u64),
                status: row.get("status"),
                attributes: row.get("attributes"),
            })
            .collect();

        Ok(spans)
    }

    /// Get database statistics
    ///
    /// Returns counts for logs, spans, and metrics snapshots.
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        let log_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs")
            .fetch_one(&self.pool)
            .await?;

        let span_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spans")
            .fetch_one(&self.pool)
            .await?;

        let metrics_snapshot_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM metrics_snapshots")
            .fetch_one(&self.pool)
            .await
            .unwrap_or(0);

        Ok(DatabaseStats {
            log_count: log_count as u64,
            span_count: span_count as u64,
            metrics_snapshot_count: metrics_snapshot_count as u64,
        })
    }

    /// Clean up old data based on retention policy
    ///
    /// This should be called periodically (e.g., daily at 3am).
    pub async fn cleanup_old_data(&self) -> Result<CleanupStats> {
        let mut tx = self.pool.begin().await?;

        // Get retention policies
        let policies: Vec<(String, i64)> = sqlx::query_as(
            "SELECT table_name, ttl_days FROM retention_policy"
        )
        .fetch_all(&mut *tx)
        .await?;

        let mut stats = CleanupStats {
            logs_deleted: 0,
            spans_deleted: 0,
            metrics_snapshots_deleted: 0,
        };

        for (table_name, ttl_days) in policies {
            let cutoff_timestamp = crate::observability::span::current_millis() as i64
                - (ttl_days * 24 * 60 * 60 * 1000);

            let deleted = match table_name.as_str() {
                "logs" => {
                    let result = sqlx::query("DELETE FROM logs WHERE timestamp < ?")
                        .bind(cutoff_timestamp)
                        .execute(&mut *tx)
                        .await?;
                    stats.logs_deleted = result.rows_affected();
                    result.rows_affected()
                }
                "spans" => {
                    let result = sqlx::query("DELETE FROM spans WHERE start_time < ?")
                        .bind(cutoff_timestamp)
                        .execute(&mut *tx)
                        .await?;
                    stats.spans_deleted = result.rows_affected();
                    result.rows_affected()
                }
                "metrics_snapshots" => {
                    let result = sqlx::query("DELETE FROM metrics_snapshots WHERE timestamp < ?")
                        .bind(cutoff_timestamp)
                        .execute(&mut *tx)
                        .await?;
                    stats.metrics_snapshots_deleted = result.rows_affected();
                    result.rows_affected()
                }
                _ => 0,
            };

            tracing::info!(
                table = %table_name,
                ttl_days = ttl_days,
                deleted = deleted,
                "Cleaned up old records"
            );

            // Update last cleanup timestamp
            sqlx::query("UPDATE retention_policy SET last_cleanup = ? WHERE table_name = ?")
                .bind(crate::observability::span::current_millis() as i64)
                .bind(&table_name)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        // VACUUM to reclaim disk space (not in transaction)
        sqlx::query("VACUUM")
            .execute(&self.pool)
            .await
            .context("Failed to VACUUM database")?;

        Ok(stats)
    }

    /// Get the underlying connection pool (for advanced usage)
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

/// Database statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseStats {
    pub log_count: u64,
    pub span_count: u64,
    pub metrics_snapshot_count: u64,
}

/// Cleanup statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupStats {
    pub logs_deleted: u64,
    pub spans_deleted: u64,
    pub metrics_snapshots_deleted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::span::{SpanContext, SpanRecord};

    async fn create_test_db() -> Result<ObservabilityDb> {
        let db_path = format!("sqlite::memory:");
        ObservabilityDb::new(&db_path).await
    }

    #[tokio::test]
    async fn test_database_creation() {
        let db = create_test_db().await.unwrap();
        let stats = db.get_stats().await.unwrap();

        assert_eq!(stats.log_count, 0);
        assert_eq!(stats.span_count, 0);
    }

    #[tokio::test]
    async fn test_insert_and_query_log() {
        let db = create_test_db().await.unwrap();

        let log = LogEntry {
            timestamp: 1000,
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Test message".to_string(),
            request_id: Some("req123".to_string()),
            span_id: None,  // No span_id to avoid FK constraint
            fields: "{}".to_string(),
        };

        db.insert_log(&log).await.unwrap();

        let logs = db.query_logs_by_request("req123").await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].message, "Test message");
    }

    #[tokio::test]
    async fn test_batch_insert_logs() {
        let db = create_test_db().await.unwrap();

        let logs: Vec<LogEntry> = (0..10)
            .map(|i| LogEntry {
                timestamp: 1000 + i,
                level: "INFO".to_string(),
                target: "test".to_string(),
                message: format!("Message {}", i),
                request_id: Some("req123".to_string()),
                span_id: None,
                fields: "{}".to_string(),
            })
            .collect();

        db.insert_logs_batch(&logs).await.unwrap();

        let retrieved = db.query_logs_by_request("req123").await.unwrap();
        assert_eq!(retrieved.len(), 10);
    }

    #[tokio::test]
    async fn test_insert_and_query_span() {
        let db = create_test_db().await.unwrap();

        let span_ctx = SpanContext::new_root("test_span");
        let span_record = SpanRecord::from_context(&span_ctx, "ok");

        db.insert_span(&span_record).await.unwrap();

        let spans = db.query_spans_by_request(&span_ctx.request_id).await.unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "test_span");
        assert_eq!(spans[0].status, "ok");
    }

    #[tokio::test]
    async fn test_cleanup_old_data() {
        let db = create_test_db().await.unwrap();

        // Insert old log
        let old_log = LogEntry {
            timestamp: 1000,  // Very old timestamp
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Old message".to_string(),
            request_id: Some("old_req".to_string()),
            span_id: None,
            fields: "{}".to_string(),
        };
        db.insert_log(&old_log).await.unwrap();

        // Insert recent log
        let recent_log = LogEntry {
            timestamp: crate::observability::span::current_millis(),
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Recent message".to_string(),
            request_id: Some("recent_req".to_string()),
            span_id: None,
            fields: "{}".to_string(),
        };
        db.insert_log(&recent_log).await.unwrap();

        // Cleanup (TTL is 7 days by default)
        let stats = db.cleanup_old_data().await.unwrap();

        // Old log should be deleted
        assert_eq!(stats.logs_deleted, 1);

        // Recent log should remain
        let logs = db.query_logs_by_request("recent_req").await.unwrap();
        assert_eq!(logs.len(), 1);
    }
}
