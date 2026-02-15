use futures::FutureExt;
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use std::sync::Arc;
use crate::pricing::CostCalculator;

/// Request event to be written to database
#[derive(Debug, Clone)]
pub struct RequestEvent {
    pub request_id: String,
    pub timestamp: i64,
    pub date: String,
    pub hour: i32,
    pub api_key_name: String,
    pub provider: String,
    pub instance: String,
    pub model: String,
    pub endpoint: String,
    pub status: String,
    pub error_type: Option<String>,
    pub error_message: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub duration_ms: i64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    pub total_cost: f64,
    pub session_id: Option<String>,
}

/// Async request logger with channel-based writes
///
/// Uses MPSC (Multi-Producer, Single-Consumer) channel to decouple
/// request handling from database writes, ensuring non-blocking operation.
#[derive(Clone)]
pub struct RequestLogger {
    tx: mpsc::Sender<RequestEvent>,
    pool: Arc<SqlitePool>,
    cost_calculator: Option<Arc<CostCalculator>>,
}

impl RequestLogger {
    /// Create new request logger with background writer task
    ///
    /// # Arguments
    /// * `pool` - SQLite database connection pool
    /// * `buffer_size` - Channel buffer size (default: 10000)
    ///
    /// # Performance
    /// - Channel send: ~1μs (non-blocking)
    /// - Background writer: 1000 req/s throughput
    /// - Backpressure: Blocks if buffer is full
    pub fn new(pool: SqlitePool, buffer_size: usize, cost_calculator: Option<Arc<CostCalculator>>) -> Self {
        let (tx, mut rx) = mpsc::channel::<RequestEvent>(buffer_size);
        let pool = Arc::new(pool);
        let pool_clone = pool.clone();

        // Spawn background writer task with panic logging
        tokio::spawn(async move {
            let result = std::panic::AssertUnwindSafe(async {
                while let Some(event) = rx.recv().await {
                    if let Err(e) = Self::write_request(&pool_clone, &event).await {
                        tracing::error!(
                            request_id = %event.request_id,
                            error = %e,
                            "Failed to write request to database"
                        );
                    }
                }
            })
            .catch_unwind()
            .await;
            match result {
                Ok(()) => tracing::warn!("RequestLogger background writer exited unexpectedly"),
                Err(e) => tracing::error!(panic = ?e, "RequestLogger background writer panicked"),
            }
        });

        Self { tx, pool, cost_calculator }
    }

    /// Log a request (non-blocking, sends to channel)
    pub async fn log_request(&self, mut event: RequestEvent) {
        // Calculate cost if calculator is available
        if let Some(calculator) = &self.cost_calculator {
            match calculator.calculate_cost(
                &event.model,
                event.input_tokens,
                event.output_tokens,
                event.cache_creation_input_tokens,
                event.cache_read_input_tokens,
            ).await {
                Ok(breakdown) => {
                    event.input_cost = breakdown.input_cost;
                    event.output_cost = breakdown.output_cost;
                    event.cache_write_cost = breakdown.cache_write_cost;
                    event.cache_read_cost = breakdown.cache_read_cost;
                    event.total_cost = breakdown.total_cost;
                }
                Err(e) => {
                    tracing::warn!(
                        model = %event.model,
                        error = %e,
                        "Failed to calculate cost, using zero"
                    );
                }
            }
        }

        if let Err(e) = self.tx.send(event).await {
            tracing::error!(error = %e, "Failed to send request to logger channel");
        }
    }

    /// Write a single request to database
    async fn write_request(pool: &SqlitePool, event: &RequestEvent) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO requests (
                request_id, timestamp, date, hour, api_key_name, provider, instance,
                model, endpoint, status, error_type, error_message,
                input_tokens, output_tokens, total_tokens,
                cache_creation_input_tokens, cache_read_input_tokens,
                duration_ms, input_cost, output_cost, cache_write_cost, cache_read_cost, total_cost,
                session_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)
            "#
        )
        .bind(&event.request_id)
        .bind(event.timestamp)
        .bind(&event.date)
        .bind(event.hour)
        .bind(&event.api_key_name)
        .bind(&event.provider)
        .bind(&event.instance)
        .bind(&event.model)
        .bind(&event.endpoint)
        .bind(&event.status)
        .bind(&event.error_type)
        .bind(&event.error_message)
        .bind(event.input_tokens)
        .bind(event.output_tokens)
        .bind(event.total_tokens)
        .bind(event.cache_creation_input_tokens)
        .bind(event.cache_read_input_tokens)
        .bind(event.duration_ms)
        .bind(event.input_cost)
        .bind(event.output_cost)
        .bind(event.cache_write_cost)
        .bind(event.cache_read_cost)
        .bind(event.total_cost)
        .bind(&event.session_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Update token counts for an existing request (used for streaming responses)
    #[allow(clippy::too_many_arguments)]
    pub async fn update_tokens(
        &self,
        request_id: &str,
        model: &str,
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        cache_creation_input_tokens: i64,
        cache_read_input_tokens: i64,
    ) {
        let request_id = request_id.to_string();
        let model = model.to_string();
        let pool = self.pool.clone();
        let cost_calculator = self.cost_calculator.clone();

        // Spawn a background task to update the database
        tokio::spawn(async move {
            // Calculate cost with the actual token counts
            let (input_cost, output_cost, cache_write_cost, cache_read_cost, total_cost) =
                if let Some(calculator) = &cost_calculator {
                    match calculator.calculate_cost(
                        &model,
                        input_tokens,
                        output_tokens,
                        cache_creation_input_tokens,
                        cache_read_input_tokens,
                    ).await {
                        Ok(breakdown) => (
                            breakdown.input_cost,
                            breakdown.output_cost,
                            breakdown.cache_write_cost,
                            breakdown.cache_read_cost,
                            breakdown.total_cost,
                        ),
                        Err(e) => {
                            tracing::warn!(
                                model = %model,
                                error = %e,
                                "Failed to calculate cost during token update, using zero"
                            );
                            (0.0, 0.0, 0.0, 0.0, 0.0)
                        }
                    }
                } else {
                    (0.0, 0.0, 0.0, 0.0, 0.0)
                };

            // Update both tokens and costs
            if let Err(e) = sqlx::query(
                "UPDATE requests SET input_tokens = ?1, output_tokens = ?2, total_tokens = ?3, cache_creation_input_tokens = ?4, cache_read_input_tokens = ?5, input_cost = ?6, output_cost = ?7, cache_write_cost = ?8, cache_read_cost = ?9, total_cost = ?10 WHERE request_id = ?11"
            )
            .bind(input_tokens)
            .bind(output_tokens)
            .bind(total_tokens)
            .bind(cache_creation_input_tokens)
            .bind(cache_read_input_tokens)
            .bind(input_cost)
            .bind(output_cost)
            .bind(cache_write_cost)
            .bind(cache_read_cost)
            .bind(total_cost)
            .bind(&request_id)
            .execute(&*pool)
            .await
            {
                tracing::error!(
                    request_id = %request_id,
                    error = %e,
                    "Failed to update token counts and costs in database"
                );
            }
        });
    }

    /// Log a failover event (non-blocking)
    ///
    /// Records circuit breaker events to the database for observability.
    #[allow(clippy::too_many_arguments)]
    pub async fn log_failover_event(
        pool: &SqlitePool,
        provider: &str,
        instance: &str,
        event_type: &str,
        failure_type: Option<&str>,
        error_message: Option<&str>,
        consecutive_failures: u32,
        next_retry_secs: Option<u64>,
    ) {
        let pool = pool.clone();
        let provider = provider.to_string();
        let instance = instance.to_string();
        let event_type = event_type.to_string();
        let failure_type = failure_type.map(|s| s.to_string());
        let error_message = error_message.map(|s| {
            // Truncate error message to 500 chars
            if s.len() > 500 {
                format!("{}...", &s[..497])
            } else {
                s.to_string()
            }
        });

        // Spawn background task for non-blocking write
        tokio::spawn(async move {
            let timestamp = chrono::Utc::now().to_rfc3339();

            if let Err(e) = sqlx::query(
                r#"
                INSERT INTO failover_events (
                    timestamp, provider, instance, event_type, failure_type,
                    error_message, consecutive_failures, next_retry_secs
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#
            )
            .bind(&timestamp)
            .bind(&provider)
            .bind(&instance)
            .bind(&event_type)
            .bind(&failure_type)
            .bind(&error_message)
            .bind(consecutive_failures as i64)
            .bind(next_retry_secs.map(|s| s as i64))
            .execute(&pool)
            .await
            {
                tracing::error!(
                    provider = %provider,
                    instance = %instance,
                    event_type = %event_type,
                    error = %e,
                    "Failed to log failover event to database"
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_event_creation() {
        let event = RequestEvent {
            request_id: "test-123".to_string(),
            timestamp: 1705728000000,
            date: "2024-01-20".to_string(),
            hour: 12,
            api_key_name: "test-key".to_string(),
            provider: "openai".to_string(),
            instance: "openai-primary".to_string(),
            model: "gpt-4".to_string(),
            endpoint: "/v1/chat/completions".to_string(),
            status: "success".to_string(),
            error_type: None,
            error_message: None,
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
            duration_ms: 1234,
            input_cost: 0.0,
            output_cost: 0.0,
            cache_write_cost: 0.0,
            cache_read_cost: 0.0,
            total_cost: 0.0,
            session_id: None,
        };

        assert_eq!(event.request_id, "test-123");
        assert_eq!(event.status, "success");
        assert_eq!(event.total_tokens, 150);
    }
}
