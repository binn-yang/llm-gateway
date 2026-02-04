use sqlx::{SqlitePool, Row};
use chrono::Utc;
use crate::quota::types::QuotaSnapshot;

pub struct QuotaDatabase {
    pool: SqlitePool,
}

impl QuotaDatabase {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 保存配额快照
    pub async fn save_snapshot(
        &self,
        snapshot: &QuotaSnapshot,
    ) -> Result<(), sqlx::Error> {
        let timestamp = Utc::now().timestamp_millis();
        let created_at = Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO quota_snapshots
             (provider, instance, auth_mode, timestamp, status, error_message, quota_data, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(provider, instance, timestamp) DO UPDATE SET
             status = excluded.status,
             error_message = excluded.error_message,
             quota_data = excluded.quota_data"
        )
        .bind(&snapshot.provider)
        .bind(&snapshot.instance)
        .bind(&snapshot.auth_mode)
        .bind(timestamp)
        .bind(snapshot.status.as_str())
        .bind(&snapshot.error_message)
        .bind(snapshot.quota_data.to_string())
        .bind(created_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 获取所有实例的最新配额快照
    pub async fn get_latest_snapshots(
        &self,
    ) -> Result<Vec<QuotaSnapshotRow>, sqlx::Error> {
        let rows = sqlx::query_as::<_, QuotaSnapshotRow>(
            "SELECT provider, instance, auth_mode, timestamp, status, error_message, quota_data
             FROM quota_snapshots
             WHERE (provider, instance, timestamp) IN (
                 SELECT provider, instance, MAX(timestamp)
                 FROM quota_snapshots
                 GROUP BY provider, instance
             )
             ORDER BY provider, instance"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    /// 清理过期的配额快照
    pub async fn cleanup_old_snapshots(
        &self,
        retention_days: i64,
    ) -> Result<u64, sqlx::Error> {
        let cutoff = Utc::now()
            .checked_sub_signed(chrono::Duration::days(retention_days))
            .unwrap()
            .timestamp_millis();

        let result = sqlx::query("DELETE FROM quota_snapshots WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[derive(Debug)]
pub struct QuotaSnapshotRow {
    pub provider: String,
    pub instance: String,
    pub auth_mode: String,
    pub timestamp: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub quota_data: String,
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for QuotaSnapshotRow {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            provider: row.try_get("provider")?,
            instance: row.try_get("instance")?,
            auth_mode: row.try_get("auth_mode")?,
            timestamp: row.try_get("timestamp")?,
            status: row.try_get("status")?,
            error_message: row.try_get("error_message")?,
            quota_data: row.try_get("quota_data")?,
        })
    }
}
