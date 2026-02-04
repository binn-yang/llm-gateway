use crate::error::AppError;
use crate::pricing::models::CostBreakdown;
use crate::pricing::service::PricingService;
use std::sync::Arc;
use tracing::warn;

/// Calculator for computing request costs based on token usage
pub struct CostCalculator {
    pricing_service: Arc<PricingService>,
}

impl CostCalculator {
    /// Create a new cost calculator
    pub fn new(pricing_service: Arc<PricingService>) -> Self {
        Self { pricing_service }
    }

    /// Calculate cost for a request
    /// Returns zero cost if pricing data is not available
    pub async fn calculate_cost(
        &self,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
        cache_creation_tokens: i64,
        cache_read_tokens: i64,
    ) -> Result<CostBreakdown, AppError> {
        // Get pricing data for the model
        let price = match self.pricing_service.get_model_price(model).await? {
            Some(p) => p,
            None => {
                warn!("No pricing data for model: {}", model);
                return Ok(CostBreakdown::zero());
            }
        };

        // Calculate costs (price is per 1M tokens)
        let mut breakdown = CostBreakdown {
            input_cost: (input_tokens as f64 / 1_000_000.0) * price.input_price,
            output_cost: (output_tokens as f64 / 1_000_000.0) * price.output_price,
            cache_write_cost: 0.0,
            cache_read_cost: 0.0,
            total_cost: 0.0,
        };

        // Calculate cache costs if pricing is available
        if let Some(cache_write_price) = price.cache_write_price {
            breakdown.cache_write_cost =
                (cache_creation_tokens as f64 / 1_000_000.0) * cache_write_price;
        }

        if let Some(cache_read_price) = price.cache_read_price {
            breakdown.cache_read_cost = (cache_read_tokens as f64 / 1_000_000.0) * cache_read_price;
        }

        // Calculate total
        breakdown.calculate_total();

        Ok(breakdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::models::ModelPrice;

    #[tokio::test]
    async fn test_cost_calculation_with_cache() {
        // Create mock pricing service with test data
        let db_pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();

        // Run migrations
        sqlx::query(
            r#"
            CREATE TABLE model_prices (
                model_name TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                input_price REAL NOT NULL,
                output_price REAL NOT NULL,
                cache_write_price REAL,
                cache_read_price REAL,
                currency TEXT NOT NULL DEFAULT 'USD',
                effective_date TEXT NOT NULL,
                notes TEXT,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&db_pool)
        .await
        .unwrap();

        let pricing_service = Arc::new(PricingService::new(db_pool));

        // Insert test pricing
        let test_price = ModelPrice {
            model_name: "claude-3-5-sonnet-20241022".to_string(),
            provider: "anthropic".to_string(),
            input_price: 3.0,
            output_price: 15.0,
            cache_write_price: Some(3.75),
            cache_read_price: Some(0.3),
            currency: "USD".to_string(),
            effective_date: "2024-01-01".to_string(),
            notes: None,
        };

        pricing_service.upsert_model_price(&test_price).await.unwrap();
        pricing_service.load_cache().await.unwrap();

        let calculator = CostCalculator::new(pricing_service);

        // Test: 1M tokens each
        let breakdown = calculator
            .calculate_cost("claude-3-5-sonnet-20241022", 1_000_000, 1_000_000, 1_000_000, 1_000_000)
            .await
            .unwrap();

        assert_eq!(breakdown.input_cost, 3.0);
        assert_eq!(breakdown.output_cost, 15.0);
        assert_eq!(breakdown.cache_write_cost, 3.75);
        assert_eq!(breakdown.cache_read_cost, 0.3);
        assert_eq!(breakdown.total_cost, 22.05);
    }

    #[tokio::test]
    async fn test_cost_calculation_no_cache() {
        let db_pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();

        sqlx::query(
            r#"
            CREATE TABLE model_prices (
                model_name TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                input_price REAL NOT NULL,
                output_price REAL NOT NULL,
                cache_write_price REAL,
                cache_read_price REAL,
                currency TEXT NOT NULL DEFAULT 'USD',
                effective_date TEXT NOT NULL,
                notes TEXT,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&db_pool)
        .await
        .unwrap();

        let pricing_service = Arc::new(PricingService::new(db_pool));

        // Insert test pricing without cache prices
        let test_price = ModelPrice {
            model_name: "gpt-4o".to_string(),
            provider: "openai".to_string(),
            input_price: 2.5,
            output_price: 10.0,
            cache_write_price: None,
            cache_read_price: None,
            currency: "USD".to_string(),
            effective_date: "2024-01-01".to_string(),
            notes: None,
        };

        pricing_service.upsert_model_price(&test_price).await.unwrap();
        pricing_service.load_cache().await.unwrap();

        let calculator = CostCalculator::new(pricing_service);

        let breakdown = calculator
            .calculate_cost("gpt-4o", 1_000_000, 1_000_000, 0, 0)
            .await
            .unwrap();

        assert_eq!(breakdown.input_cost, 2.5);
        assert_eq!(breakdown.output_cost, 10.0);
        assert_eq!(breakdown.cache_write_cost, 0.0);
        assert_eq!(breakdown.cache_read_cost, 0.0);
        assert_eq!(breakdown.total_cost, 12.5);
    }

    #[tokio::test]
    async fn test_cost_calculation_unknown_model() {
        let db_pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();

        sqlx::query(
            r#"
            CREATE TABLE model_prices (
                model_name TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                input_price REAL NOT NULL,
                output_price REAL NOT NULL,
                cache_write_price REAL,
                cache_read_price REAL,
                currency TEXT NOT NULL DEFAULT 'USD',
                effective_date TEXT NOT NULL,
                notes TEXT,
                created_at INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&db_pool)
        .await
        .unwrap();

        let pricing_service = Arc::new(PricingService::new(db_pool));
        let calculator = CostCalculator::new(pricing_service);

        // Unknown model should return zero cost
        let breakdown = calculator
            .calculate_cost("unknown-model", 1_000_000, 1_000_000, 0, 0)
            .await
            .unwrap();

        assert_eq!(breakdown.total_cost, 0.0);
    }
}

