//! Background cleanup task for observability data
//!
//! Automatically deletes old data based on TTL policies.

use super::database::{CleanupStats, ObservabilityDb};
use anyhow::Result;
use chrono::{Datelike, Timelike};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

/// Cleanup configuration
#[derive(Debug, Clone, Copy)]
pub struct CleanupConfig {
    /// Hour of day to run cleanup (0-23)
    pub cleanup_hour: u32,

    /// Check interval (how often to check if it's cleanup time)
    pub check_interval: Duration,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            cleanup_hour: 3, // 3 AM by default
            check_interval: Duration::from_secs(3600), // Check every hour
        }
    }
}

/// Spawn background cleanup task
///
/// This task periodically checks if it's time to run cleanup (based on cleanup_hour),
/// and executes the cleanup process to delete old data.
///
/// # Arguments
///
/// * `db` - Database handle
/// * `config` - Cleanup configuration
///
/// # Example
///
/// ```ignore
/// let config = CleanupConfig {
///     cleanup_hour: 3,  // Run at 3 AM
///     check_interval: Duration::from_secs(3600),  // Check every hour
/// };
/// spawn_cleanup_task(db.clone(), config);
/// ```
pub fn spawn_cleanup_task(db: Arc<ObservabilityDb>, config: CleanupConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        cleanup_loop(db, config).await;
    })
}

/// Main cleanup loop
async fn cleanup_loop(db: Arc<ObservabilityDb>, config: CleanupConfig) {
    let mut interval = time::interval(config.check_interval);
    let mut last_cleanup_day: Option<u32> = None;

    loop {
        interval.tick().await;

        // Get current hour and day
        let now = chrono::Local::now();
        let current_hour = now.hour();
        let current_day = now.ordinal();

        // Check if it's time to run cleanup
        if current_hour == config.cleanup_hour && Some(current_day) != last_cleanup_day {
            tracing::info!(
                cleanup_hour = config.cleanup_hour,
                "Starting scheduled cleanup"
            );

            match db.cleanup_old_data().await {
                Ok(stats) => {
                    tracing::info!(
                        logs_deleted = stats.logs_deleted,
                        spans_deleted = stats.spans_deleted,
                        metrics_snapshots_deleted = stats.metrics_snapshots_deleted,
                        "Cleanup completed successfully"
                    );

                    last_cleanup_day = Some(current_day);
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Cleanup failed"
                    );
                }
            }
        }
    }
}

/// Run cleanup immediately (for manual triggering)
///
/// This is useful for testing or manual cleanup operations.
pub async fn run_cleanup_now(db: &ObservabilityDb) -> Result<CleanupStats> {
    tracing::info!("Running manual cleanup");

    let stats = db.cleanup_old_data().await?;

    tracing::info!(
        logs_deleted = stats.logs_deleted,
        spans_deleted = stats.spans_deleted,
        metrics_snapshots_deleted = stats.metrics_snapshots_deleted,
        "Manual cleanup completed"
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::database::{LogEntry, ObservabilityDb};
    use crate::observability::span::current_millis;

    async fn create_test_db() -> Arc<ObservabilityDb> {
        let db = ObservabilityDb::new("sqlite::memory:").await.unwrap();
        Arc::new(db)
    }

    #[tokio::test]
    async fn test_run_cleanup_now() {
        let db = create_test_db().await;

        // Insert an old log entry
        let old_log = LogEntry {
            timestamp: 1000, // Very old
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Old message".to_string(),
            request_id: None,
            span_id: None,
            fields: "{}".to_string(),
        };

        db.insert_log(&old_log).await.unwrap();

        // Insert a recent log entry
        let recent_log = LogEntry {
            timestamp: current_millis(),
            level: "INFO".to_string(),
            target: "test".to_string(),
            message: "Recent message".to_string(),
            request_id: None,
            span_id: None,
            fields: "{}".to_string(),
        };

        db.insert_log(&recent_log).await.unwrap();

        // Run cleanup
        let stats = run_cleanup_now(&db).await.unwrap();

        // Old log should be deleted
        assert_eq!(stats.logs_deleted, 1);
    }

    #[tokio::test]
    async fn test_cleanup_config_default() {
        let config = CleanupConfig::default();
        assert_eq!(config.cleanup_hour, 3);
        assert_eq!(config.check_interval, Duration::from_secs(3600));
    }
}
