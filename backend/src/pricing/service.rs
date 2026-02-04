use crate::error::AppError;
use crate::pricing::models::ModelPrice;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Service for managing model pricing data
pub struct PricingService {
    db_pool: SqlitePool,
    /// In-memory cache of pricing data for fast lookups
    cache: Arc<RwLock<HashMap<String, ModelPrice>>>,
}

impl PricingService {
    /// Create a new pricing service
    pub fn new(db_pool: SqlitePool) -> Self {
        Self {
            db_pool,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load pricing data from database into cache
    pub async fn load_cache(&self) -> Result<(), AppError> {
        let prices = self.list_all_prices().await?;
        let mut cache = self.cache.write().await;
        cache.clear();

        for price in prices {
            cache.insert(price.model_name.clone(), price);
        }

        info!("Loaded {} model prices into cache", cache.len());
        Ok(())
    }

    /// Get pricing for a specific model (from cache)
    pub async fn get_model_price(&self, model: &str) -> Result<Option<ModelPrice>, AppError> {
        let cache = self.cache.read().await;
        Ok(cache.get(model).cloned())
    }

    /// List all pricing data from database
    pub async fn list_all_prices(&self) -> Result<Vec<ModelPrice>, AppError> {
        let rows = sqlx::query_as::<_, (String, String, f64, f64, Option<f64>, Option<f64>, String, String, Option<String>)>(
            r#"
            SELECT model_name, provider, input_price, output_price,
                   cache_write_price, cache_read_price, currency,
                   effective_date, notes
            FROM model_prices
            ORDER BY provider, model_name
            "#
        )
        .fetch_all(&self.db_pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|(model_name, provider, input_price, output_price, cache_write_price, cache_read_price, currency, effective_date, notes)| ModelPrice {
                model_name,
                provider,
                input_price,
                output_price,
                cache_write_price,
                cache_read_price,
                currency,
                effective_date,
                notes,
            })
            .collect())
    }

    /// Insert or update a model price
    pub async fn upsert_model_price(&self, price: &ModelPrice) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO model_prices (
                model_name, provider, input_price, output_price,
                cache_write_price, cache_read_price, currency,
                effective_date, notes, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(model_name) DO UPDATE SET
                provider = excluded.provider,
                input_price = excluded.input_price,
                output_price = excluded.output_price,
                cache_write_price = excluded.cache_write_price,
                cache_read_price = excluded.cache_read_price,
                currency = excluded.currency,
                effective_date = excluded.effective_date,
                notes = excluded.notes,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&price.model_name)
        .bind(&price.provider)
        .bind(price.input_price)
        .bind(price.output_price)
        .bind(price.cache_write_price)
        .bind(price.cache_read_price)
        .bind(&price.currency)
        .bind(&price.effective_date)
        .bind(&price.notes)
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&self.db_pool)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(())
    }
}
