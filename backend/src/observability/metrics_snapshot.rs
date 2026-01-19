//! Prometheus metrics to SQLite snapshot writer
//!
//! Periodically scrapes Prometheus metrics, calculates deltas,
//! and persists to SQLite for time-series queries.

use crate::error::AppError;
use chrono::Timelike;
use chrono::Utc;
use dashmap::DashMap;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

/// Metrics snapshot writer that persists Prometheus metrics to SQLite
#[derive(Clone)]
pub struct MetricsSnapshotWriter {
    metrics_handle: Arc<PrometheusHandle>,
    db_pool: SqlitePool,
    last_values: Arc<DashMap<String, f64>>,
}

impl MetricsSnapshotWriter {
    /// Create a new metrics snapshot writer
    pub fn new(metrics_handle: Arc<PrometheusHandle>, db_pool: SqlitePool) -> Self {
        Self {
            metrics_handle,
            db_pool,
            last_values: Arc::new(DashMap::new()),
        }
    }

    /// Start the background snapshot task
    pub async fn spawn_snapshot_task(&self, interval_secs: u64) {
        let mut timer = interval(Duration::from_secs(interval_secs));

        loop {
            timer.tick().await;
            if let Err(e) = self.snapshot_metrics().await {
                tracing::error!("Failed to snapshot metrics: {}", e);
            }
        }
    }

    /// Scrape metrics, calculate deltas, and persist to SQLite
    pub async fn snapshot_metrics(&self) -> Result<(), AppError> {
        let metrics_text = self.metrics_handle.render();

        // Parse and calculate deltas
        let (token_deltas, request_counts) = self.parse_metrics(&metrics_text)?;
        let health_statuses = self.parse_health_metrics(&metrics_text)?;

        // Batch insert with UPSERT
        self.insert_token_usage(token_deltas, request_counts).await?;
        self.insert_instance_health(health_statuses).await?;

        Ok(())
    }

    /// Parse both llm_tokens_total and llm_requests_total metrics
    fn parse_metrics(&self, text: &str) -> Result<(Vec<TokenDelta>, HashMap<String, u64>), AppError> {
        let mut token_deltas = Vec::new();
        let mut request_counts: HashMap<String, u64> = HashMap::new();

        for line in text.lines() {
            // Parse token metrics
            if line.starts_with("llm_tokens_total{") {
                match parse_prometheus_line(line) {
                    Ok((labels, value)) => {
                        // Build unique key for delta calculation
                        let key = format!(
                            "{}:{}:{}:{}",
                            labels.get("api_key").unwrap_or(&"unknown".to_string()),
                            labels.get("provider").unwrap_or(&"unknown".to_string()),
                            labels.get("model").unwrap_or(&"unknown".to_string()),
                            labels.get("instance").unwrap_or(&"".to_string())
                        );

                        // Calculate delta
                        let last_value = self.last_values.get(&key).map(|v| *v).unwrap_or(0.0);
                        let delta = value - last_value;

                        // Only record positive deltas
                        if delta > 0.0 {
                            if let Some(token_type) = labels.get("type") {
                                token_deltas.push(TokenDelta {
                                    api_key: labels.get("api_key").cloned().unwrap_or_default(),
                                    provider: labels.get("provider").cloned().unwrap_or_default(),
                                    model: labels.get("model").cloned().unwrap_or_default(),
                                    instance: labels.get("instance").cloned(),
                                    token_type: (*token_type).clone(),
                                    delta: delta as u64,
                                });
                            }
                        }

                        // Update last value (even if delta is zero)
                        self.last_values.insert(key, value);
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse Prometheus line: {}, error: {}", line, e);
                    }
                }
            }
            // Parse request metrics
            else if line.starts_with("llm_requests_total{") {
                match parse_prometheus_line(line) {
                    Ok((labels, value)) => {
                        let key = format!(
                            "{}:{}:{}:{}",
                            labels.get("api_key").unwrap_or(&"unknown".to_string()),
                            labels.get("provider").unwrap_or(&"unknown".to_string()),
                            labels.get("model").unwrap_or(&"unknown".to_string()),
                            labels.get("instance").unwrap_or(&"".to_string())
                        );
                        request_counts.insert(key, value as u64);
                    }
                    Err(e) => {
                        tracing::debug!("Failed to parse request Prometheus line: {}, error: {}", line, e);
                    }
                }
            }
        }

        Ok((token_deltas, request_counts))
    }

    /// Parse llm_gateway_instance_health_status metrics
    fn parse_health_metrics(&self, text: &str) -> Result<Vec<HealthStatus>, AppError> {
        let mut statuses = Vec::new();

        for line in text.lines() {
            if !line.starts_with("llm_gateway_instance_health_status{") {
                continue;
            }

            match parse_prometheus_line(line) {
                Ok((labels, value)) => {
                    if let (Some(provider), Some(instance)) = (
                        labels.get("provider"),
                        labels.get("instance"),
                    ) {
                        statuses.push(HealthStatus {
                            provider: (*provider).clone(),
                            instance: (*instance).clone(),
                            health_status: if value == 1.0 { "healthy" } else { "unhealthy" }.to_string(),
                        });
                    }
                }
                Err(e) => {
                    tracing::debug!("Failed to parse health line: {}, error: {}", line, e);
                }
            }
        }

        Ok(statuses)
    }

    /// Batch insert token usage deltas and request counts
    async fn insert_token_usage(&self, deltas: Vec<TokenDelta>, request_counts: HashMap<String, u64>) -> Result<(), sqlx::Error> {
        if deltas.is_empty() && request_counts.is_empty() {
            return Ok(());
        }

        let now = Utc::now().timestamp_millis();
        let aligned_ts = (now / 60_000) * 60_000; // Minute-aligned
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let hour = Utc::now().hour();

        // Aggregate by dimensions
        let mut aggregated: HashMap<String, TokenUsageRow> = HashMap::new();

        // Process token deltas
        for delta in deltas {
            let key = format!(
                "{}:{}:{}:{}",
                delta.api_key,
                delta.provider,
                delta.model,
                delta.instance.as_ref().unwrap_or(&"".to_string())
            );

            let entry = aggregated.entry(key.clone()).or_insert_with(|| TokenUsageRow {
                timestamp: aligned_ts,
                date: date.clone(),
                hour: hour as i32,
                api_key: delta.api_key.clone(),
                provider: delta.provider.clone(),
                model: delta.model.clone(),
                instance: delta.instance.clone(),
                ..Default::default()
            });

            match delta.token_type.as_str() {
                "input" => entry.input_tokens += delta.delta,
                "output" => entry.output_tokens += delta.delta,
                _ => {}
            }
            entry.total_tokens += delta.delta;
        }

        // Add request counts
        for (key, count) in request_counts {
            let entry = aggregated.entry(key.clone()).or_insert_with(|| {
                // Parse the key to get dimensions
                let parts: Vec<&str> = key.split(':').collect();
                let instance_str = parts.get(3).unwrap_or(&"");
                TokenUsageRow {
                    timestamp: aligned_ts,
                    date: date.clone(),
                    hour: hour as i32,
                    api_key: parts.get(0).unwrap_or(&"").to_string(),
                    provider: parts.get(1).unwrap_or(&"").to_string(),
                    model: parts.get(2).unwrap_or(&"").to_string(),
                    instance: if instance_str.is_empty() { None } else { Some(instance_str.to_string()) },
                    ..Default::default()
                }
            });
            entry.request_count += count;
        }

        // Batch insert with UPSERT
        for row in aggregated.values() {
            sqlx::query(
                r#"
                INSERT INTO token_usage (
                    timestamp, date, hour, api_key, provider, model, instance,
                    input_tokens, output_tokens, total_tokens, request_count
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ON CONFLICT(timestamp, api_key, provider, model, instance)
                DO UPDATE SET
                    input_tokens = input_tokens + excluded.input_tokens,
                    output_tokens = output_tokens + excluded.output_tokens,
                    total_tokens = total_tokens + excluded.total_tokens,
                    request_count = request_count + excluded.request_count
                "#,
            )
            .bind(row.timestamp)
            .bind(&row.date)
            .bind(row.hour)
            .bind(&row.api_key)
            .bind(&row.provider)
            .bind(&row.model)
            .bind(&row.instance)
            .bind(row.input_tokens as i64)
            .bind(row.output_tokens as i64)
            .bind(row.total_tokens as i64)
            .bind(row.request_count as i64)
            .execute(&self.db_pool)
            .await?;
        }

        Ok(())
    }

    /// Batch insert instance health statuses
    async fn insert_instance_health(&self, statuses: Vec<HealthStatus>) -> Result<(), sqlx::Error> {
        if statuses.is_empty() {
            return Ok(());
        }

        let now = Utc::now().timestamp_millis();
        let aligned_ts = (now / 60_000) * 60_000;
        let date = Utc::now().format("%Y-%m-%d").to_string();
        let hour = Utc::now().hour();

        for status in statuses {
            sqlx::query(
                r#"
                INSERT INTO instance_health (
                    timestamp, date, hour, provider, instance, health_status,
                    request_count, success_count, failure_count, failover_count
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 0, 0, 0)
                ON CONFLICT(timestamp, provider, instance)
                DO UPDATE SET
                    health_status = excluded.health_status
                "#,
            )
            .bind(aligned_ts)
            .bind(&date)
            .bind(hour as i32)
            .bind(&status.provider)
            .bind(&status.instance)
            .bind(&status.health_status)
            .execute(&self.db_pool)
            .await?;
        }

        Ok(())
    }
}

/// Token usage delta for a specific metric
#[derive(Debug, Clone)]
struct TokenDelta {
    api_key: String,
    provider: String,
    model: String,
    instance: Option<String>,
    token_type: String,
    delta: u64,
}

/// Aggregated token usage row for database insertion
#[derive(Debug, Default)]
struct TokenUsageRow {
    timestamp: i64,
    date: String,
    hour: i32,
    api_key: String,
    provider: String,
    model: String,
    instance: Option<String>,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    request_count: u64,
    success_count: u64,
    error_count: u64,
    avg_duration_ms: Option<i64>,
}

/// Instance health status
#[derive(Debug, Clone)]
struct HealthStatus {
    provider: String,
    instance: String,
    health_status: String,
}

/// Parse a single Prometheus metric line
/// Format: metric_name{label1="value1",label2="value2"} value
fn parse_prometheus_line(line: &str) -> Result<(HashMap<String, String>, f64), AppError> {
    // Find the opening brace
    let brace_start = line.find('{').ok_or_else(|| {
        AppError::ConversionError(format!("Missing opening brace in line: {}", line))
    })?;

    // Extract metric name (before the brace)
    let _metric_name = &line[..brace_start];

    // Find the closing brace
    let brace_end = line.find('}').ok_or_else(|| {
        AppError::ConversionError(format!("Missing closing brace in line: {}", line))
    })?;

    // Extract labels string
    let labels_str = &line[brace_start + 1..brace_end];

    // Extract value (after the closing brace)
    let value_str = line[brace_end + 1..].trim();
    let value: f64 = value_str.parse().map_err(|_| {
        AppError::ConversionError(format!("Invalid value '{}' in line: {}", value_str, line))
    })?;

    // Parse labels
    let labels = parse_labels(labels_str)?;

    Ok((labels, value))
}

/// Parse Prometheus labels string into a HashMap
/// Format: label1="value1",label2="value2"
fn parse_labels(labels_str: &str) -> Result<HashMap<String, String>, AppError> {
    let mut labels = HashMap::new();

    if labels_str.is_empty() {
        return Ok(labels);
    }

    for part in labels_str.split(',') {
        let part = part.trim();
        if let Some(eq_pos) = part.find('=') {
            let key = part[..eq_pos].trim().to_string();
            let value = part[eq_pos + 1..].trim().to_string();

            // Remove quotes if present
            let value = if value.starts_with('"') && value.ends_with('"') {
                value[1..value.len() - 1].to_string()
            } else {
                value
            };

            labels.insert(key, value);
        }
    }

    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prometheus_line() {
        let line = r#"llm_tokens_total{api_key="test",provider="anthropic",model="claude-3",type="input"} 12345.0"#;

        let (labels, value) = parse_prometheus_line(line).unwrap();

        assert_eq!(labels.get("api_key"), Some(&"test".to_string()));
        assert_eq!(labels.get("provider"), Some(&"anthropic".to_string()));
        assert_eq!(labels.get("model"), Some(&"claude-3".to_string()));
        assert_eq!(labels.get("type"), Some(&"input".to_string()));
        assert_eq!(value, 12345.0);
    }

    #[test]
    fn test_parse_labels() {
        let labels_str = r#"api_key="test",provider="anthropic",model="claude-3""#;
        let labels = parse_labels(labels_str).unwrap();

        assert_eq!(labels.get("api_key"), Some(&"test".to_string()));
        assert_eq!(labels.get("provider"), Some(&"anthropic".to_string()));
        assert_eq!(labels.get("model"), Some(&"claude-3".to_string()));
    }

    #[test]
    fn test_parse_empty_labels() {
        let labels_str = "";
        let labels = parse_labels(labels_str).unwrap();

        assert!(labels.is_empty());
    }
}
