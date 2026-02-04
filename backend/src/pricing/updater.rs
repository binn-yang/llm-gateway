use crate::error::AppError;
use crate::pricing::loader::{calculate_hash, download_pricing_from_url, parse_pricing_json, save_backup};
use crate::pricing::service::PricingService;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

/// Pricing updater that periodically checks for pricing updates
pub struct PricingUpdater {
    pricing_service: Arc<PricingService>,
    db_pool: SqlitePool,
    remote_url: String,
    backup_dir: String,
    update_interval: Duration,
}

impl PricingUpdater {
    /// Create a new pricing updater
    pub fn new(
        pricing_service: Arc<PricingService>,
        db_pool: SqlitePool,
        remote_url: String,
        backup_dir: String,
        update_interval: Duration,
    ) -> Self {
        Self {
            pricing_service,
            db_pool,
            remote_url,
            backup_dir,
            update_interval,
        }
    }

    /// Start background task for periodic updates
    pub async fn start_background_task(self: Arc<Self>) {
        info!(
            "Starting pricing updater (interval: {:?})",
            self.update_interval
        );

        // Perform initial update immediately
        if let Err(e) = self.check_and_update().await {
            error!("Initial pricing update failed: {}", e);
        }

        // Start periodic update loop
        let mut interval = tokio::time::interval(self.update_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match self.check_and_update().await {
                Ok(updated) => {
                    if updated {
                        info!("Pricing data updated successfully");
                    } else {
                        info!("Pricing data unchanged (no update needed)");
                    }
                }
                Err(e) => {
                    error!("Pricing update failed: {}", e);
                }
            }
        }
    }

    /// Check for updates and apply if changed
    /// Returns true if data was updated, false if unchanged
    pub async fn check_and_update(&self) -> Result<bool, AppError> {
        // Download pricing data
        let content = download_pricing_from_url(&self.remote_url).await?;

        // Calculate hash
        let new_hash = calculate_hash(&content);

        // Get last hash from database
        let last_hash = self.get_last_hash().await?;

        // Check if changed
        if let Some(last) = last_hash {
            if last == new_hash {
                return Ok(false); // No change
            }
        }

        info!("Pricing data changed, updating database");

        // Save backup
        save_backup(&content, &self.backup_dir).await?;

        // Parse pricing data
        let prices = parse_pricing_json(&content).await?;

        // Update database
        for price in prices {
            self.pricing_service.upsert_model_price(&price).await?;
        }

        // Reload cache
        self.pricing_service.load_cache().await?;

        // Save new hash
        self.save_hash(&new_hash).await?;

        Ok(true)
    }

    /// Get last pricing hash from database
    async fn get_last_hash(&self) -> Result<Option<String>, AppError> {
        let row = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT value FROM pricing_metadata WHERE key = 'last_pricing_hash'
            "#
        )
        .fetch_optional(&self.db_pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(row.and_then(|(value,): (String,)| if value.is_empty() { None } else { Some(value) }))
    }

    /// Save pricing hash to database
    async fn save_hash(&self, hash: &str) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO pricing_metadata (key, value, updated_at)
            VALUES ('last_pricing_hash', ?, ?)
            "#,
        )
        .bind(hash)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.db_pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(())
    }
}
