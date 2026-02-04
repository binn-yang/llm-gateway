use sqlx::SqlitePool;
use std::time::Duration;
use tokio::time::interval;
use chrono::Timelike;
use crate::config::ObservabilityConfig;
use crate::quota::db::QuotaDatabase;

/// Start the cleanup task
///
/// This task runs hourly and checks if it's time to perform cleanup
/// based on the configured cleanup hour.
pub fn start_cleanup_task(pool: SqlitePool, config: ObservabilityConfig) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(3600)); // 每小时检查一次

        loop {
            interval.tick().await;

            // 检查是否到了配置的清理时间
            if should_run_cleanup(&config) {
                tracing::info!("开始执行数据清理任务");

                // 清理请求日志
                if let Err(e) = cleanup_requests(&pool, &config).await {
                    tracing::error!("清理请求日志失败: {}", e);
                }

                // 清理配额快照
                let quota_db = QuotaDatabase::new(pool.clone());
                let retention_days = config.quota_refresh.retention_days;

                match quota_db.cleanup_old_snapshots(retention_days).await {
                    Ok(deleted) => {
                        if deleted > 0 {
                            tracing::info!("清理了 {} 条过期配额快照", deleted);
                        }
                    }
                    Err(e) => {
                        tracing::error!("配额快照清理失败: {}", e);
                    }
                }

                tracing::info!("数据清理任务完成");
            }
        }
    })
}

/// Check if cleanup should run based on current time and configured hour
fn should_run_cleanup(config: &ObservabilityConfig) -> bool {
    use chrono::Utc;

    let now = Utc::now();
    let current_hour = now.hour() as u8;

    // 只在配置的小时运行
    current_hour == config.retention.cleanup_hour
}

/// Clean up old request logs
async fn cleanup_requests(pool: &SqlitePool, config: &ObservabilityConfig) -> Result<(), sqlx::Error> {
    use chrono::{Utc, Duration};

    let cutoff = Utc::now()
        .checked_sub_signed(Duration::days(config.retention.logs_days as i64))
        .unwrap()
        .timestamp_millis();

    let result = sqlx::query("DELETE FROM requests WHERE timestamp < ?")
        .bind(cutoff)
        .execute(pool)
        .await?;

    if result.rows_affected() > 0 {
        tracing::info!(
            "清理了 {} 条过期请求日志 (保留 {} 天)",
            result.rows_affected(),
            config.retention.logs_days
        );
    }

    Ok(())
}
