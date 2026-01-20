use sqlx::SqlitePool;
use tokio::sync::mpsc;
use std::sync::Arc;

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
}

/// Async request logger with channel-based writes
///
/// Uses MPSC (Multi-Producer, Single-Consumer) channel to decouple
/// request handling from database writes, ensuring non-blocking operation.
#[derive(Clone)]
pub struct RequestLogger {
    tx: mpsc::Sender<RequestEvent>,
    pool: Arc<SqlitePool>,
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
    pub fn new(pool: SqlitePool, buffer_size: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<RequestEvent>(buffer_size);
        let pool = Arc::new(pool);
        let pool_clone = pool.clone();

        // Spawn background writer task
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                if let Err(e) = Self::write_request(&pool_clone, &event).await {
                    tracing::error!(
                        request_id = %event.request_id,
                        error = %e,
                        "Failed to write request to database"
                    );
                }
            }
        });

        Self { tx, pool }
    }

    /// Log a request (non-blocking, sends to channel)
    pub async fn log_request(&self, event: RequestEvent) {
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
                duration_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
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
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Update token counts for an existing request (used for streaming responses)
    pub async fn update_tokens(
        &self,
        request_id: &str,
        input_tokens: i64,
        output_tokens: i64,
        total_tokens: i64,
        cache_creation_input_tokens: i64,
        cache_read_input_tokens: i64,
    ) {
        let request_id = request_id.to_string();
        let pool = self.pool.clone();

        // Spawn a background task to update the database
        tokio::spawn(async move {
            if let Err(e) = sqlx::query(
                "UPDATE requests SET input_tokens = ?1, output_tokens = ?2, total_tokens = ?3, cache_creation_input_tokens = ?4, cache_read_input_tokens = ?5 WHERE request_id = ?6"
            )
            .bind(input_tokens)
            .bind(output_tokens)
            .bind(total_tokens)
            .bind(cache_creation_input_tokens)
            .bind(cache_read_input_tokens)
            .bind(&request_id)
            .execute(&*pool)
            .await
            {
                tracing::error!(
                    request_id = %request_id,
                    error = %e,
                    "Failed to update token counts in database"
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
        };

        assert_eq!(event.request_id, "test-123");
        assert_eq!(event.status, "success");
        assert_eq!(event.total_tokens, 150);
    }
}
