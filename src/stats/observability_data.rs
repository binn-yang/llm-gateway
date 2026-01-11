//! Observability data fetcher for stats dashboard
//!
//! Provides high-level API to fetch observability data from SQLite database,
//! including error patterns, slow requests, instance health, and trend data.

use crate::observability::{ErrorPattern, ObservabilityDb, SlowRequest};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;

/// Observability data fetcher
///
/// Wraps ObservabilityDb and provides convenient methods for stats dashboard queries.
pub struct ObservabilityDataFetcher {
    db: Arc<ObservabilityDb>,
}

impl ObservabilityDataFetcher {
    /// Create a new ObservabilityDataFetcher
    ///
    /// # Arguments
    /// * `db_path` - Path to SQLite database (e.g., "./data/observability.db")
    ///
    /// # Example
    /// ```ignore
    /// let fetcher = ObservabilityDataFetcher::new("./data/observability.db").await?;
    /// ```
    pub async fn new(db_path: &str) -> Result<Self> {
        let db_url = format!("sqlite:{}", db_path);
        let db = ObservabilityDb::new(&db_url).await?;
        Ok(Self { db: Arc::new(db) })
    }

    /// Get error patterns from the last hour
    ///
    /// # Arguments
    /// * `limit` - Maximum number of error patterns to return (default: 5)
    ///
    /// # Returns
    /// Vector of error patterns sorted by count (descending)
    pub async fn get_error_patterns(&self, limit: usize) -> Result<Vec<ErrorPattern>> {
        self.db.query_error_patterns(3600, limit).await
    }

    /// Get slow requests above threshold
    ///
    /// # Arguments
    /// * `threshold_ms` - Minimum duration in milliseconds (default: 5000)
    /// * `limit` - Maximum number of results (default: 5)
    ///
    /// # Returns
    /// Vector of slow requests sorted by duration (descending)
    pub async fn get_slow_requests(
        &self,
        threshold_ms: u64,
        limit: usize,
    ) -> Result<Vec<SlowRequest>> {
        self.db.query_slow_requests(threshold_ms, limit).await
    }

    /// Get instance health status from latest metrics snapshot
    ///
    /// Parses the most recent metrics_snapshots row and extracts:
    /// - llm_gateway_instance_health_status (healthy/unhealthy)
    /// - llm_instance_requests_total (success/failure counts)
    ///
    /// # Returns
    /// InstanceHealthData with per-instance health status and overall error rate
    pub async fn get_instance_health(&self) -> Result<InstanceHealthData> {
        // Query the most recent metrics snapshot
        let row = sqlx::query(
            "SELECT metrics_data FROM metrics_snapshots ORDER BY timestamp DESC LIMIT 1",
        )
        .fetch_optional(self.db.pool())
        .await?;

        if let Some(row) = row {
            let metrics_json: String = row.get("metrics_data");
            parse_instance_health_from_metrics(&metrics_json)
        } else {
            // No snapshots available - return empty data
            Ok(InstanceHealthData {
                instances: vec![],
                overall_error_rate: 0.0,
            })
        }
    }

    /// Get request trend from recent metrics snapshots
    ///
    /// # Arguments
    /// * `points` - Number of data points to return (default: 24 = 2 hours at 5-min intervals)
    ///
    /// # Returns
    /// Vector of trend points with timestamp and request count
    pub async fn get_request_trend(&self, points: usize) -> Result<Vec<TrendPoint>> {
        let rows = sqlx::query(
            "SELECT timestamp, metrics_data
             FROM metrics_snapshots
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(points as i64)
        .fetch_all(self.db.pool())
        .await?;

        let mut trend_points = Vec::new();

        for row in rows {
            let timestamp: i64 = row.get("timestamp");
            let metrics_json: String = row.get("metrics_data");

            // Extract llm_requests_total from metrics
            if let Ok(total_requests) = extract_total_requests(&metrics_json) {
                trend_points.push(TrendPoint {
                    timestamp: timestamp as u64,
                    value: total_requests,
                });
            }
        }

        // Reverse to get chronological order (oldest first)
        trend_points.reverse();

        Ok(trend_points)
    }
}

/// Instance health data aggregated from metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceHealthData {
    /// List of provider instances with health status
    pub instances: Vec<InstanceStatus>,

    /// Overall error rate across all instances (0-100%)
    pub overall_error_rate: f64,
}

/// Health status for a single provider instance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceStatus {
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,

    /// Instance name (e.g., "anthropic-primary", "anthropic-backup")
    pub instance: String,

    /// Whether instance is currently healthy
    pub healthy: bool,

    /// Success rate (0-100%)
    pub success_rate: f64,

    /// Total requests processed by this instance
    pub total_requests: u64,
}

/// Single data point in request trend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendPoint {
    /// Unix timestamp in milliseconds
    pub timestamp: u64,

    /// Request count or rate at this point
    pub value: f64,
}

/// Parse instance health from Prometheus metrics JSON
fn parse_instance_health_from_metrics(metrics_json: &str) -> Result<InstanceHealthData> {
    // Prometheus metrics are stored as newline-separated text format
    // Example lines:
    // llm_gateway_instance_health_status{provider="anthropic",instance="anthropic-primary"} 1
    // llm_instance_requests_total{provider="anthropic",instance="anthropic-primary",status="success"} 100
    // llm_instance_requests_total{provider="anthropic",instance="anthropic-primary",status="failure"} 2

    let mut instances: std::collections::HashMap<(String, String), InstanceStatusBuilder> =
        std::collections::HashMap::new();

    for line in metrics_json.lines() {
        if line.starts_with("llm_gateway_instance_health_status{") {
            // Parse: llm_gateway_instance_health_status{provider="X",instance="Y"} 1
            if let Some((labels, value)) = parse_metric_line(line) {
                let provider = labels.get("provider").cloned().unwrap_or_default();
                let instance = labels.get("instance").cloned().unwrap_or_default();
                let healthy = value > 0.5; // 1.0 = healthy, 0.0 = unhealthy

                instances
                    .entry((provider.clone(), instance.clone()))
                    .or_insert_with(|| InstanceStatusBuilder {
                        provider,
                        instance,
                        healthy,
                        success_count: 0,
                        failure_count: 0,
                    })
                    .healthy = healthy;
            }
        } else if line.starts_with("llm_instance_requests_total{") {
            // Parse: llm_instance_requests_total{provider="X",instance="Y",status="success"} 100
            if let Some((labels, value)) = parse_metric_line(line) {
                let provider = labels.get("provider").cloned().unwrap_or_default();
                let instance = labels.get("instance").cloned().unwrap_or_default();
                let status = labels.get("status").cloned().unwrap_or_default();

                let builder = instances
                    .entry((provider.clone(), instance.clone()))
                    .or_insert_with(|| InstanceStatusBuilder {
                        provider,
                        instance,
                        healthy: true, // Default to healthy if no health metric
                        success_count: 0,
                        failure_count: 0,
                    });

                if status == "success" {
                    builder.success_count = value as u64;
                } else if status == "failure" {
                    builder.failure_count = value as u64;
                }
            }
        }
    }

    // Build final instances list
    let mut instance_list: Vec<InstanceStatus> = instances
        .into_values()
        .map(|b| b.build())
        .collect();

    // Sort by provider then instance name
    instance_list.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.instance.cmp(&b.instance))
    });

    // Calculate overall error rate
    let total_requests: u64 = instance_list.iter().map(|i| i.total_requests).sum();
    let total_failures: u64 = instance_list
        .iter()
        .map(|i| {
            let failure_rate = (100.0 - i.success_rate) / 100.0;
            (i.total_requests as f64 * failure_rate) as u64
        })
        .sum();

    let overall_error_rate = if total_requests > 0 {
        (total_failures as f64 / total_requests as f64) * 100.0
    } else {
        0.0
    };

    Ok(InstanceHealthData {
        instances: instance_list,
        overall_error_rate,
    })
}

/// Helper struct for building InstanceStatus
struct InstanceStatusBuilder {
    provider: String,
    instance: String,
    healthy: bool,
    success_count: u64,
    failure_count: u64,
}

impl InstanceStatusBuilder {
    fn build(self) -> InstanceStatus {
        let total_requests = self.success_count + self.failure_count;
        let success_rate = if total_requests > 0 {
            (self.success_count as f64 / total_requests as f64) * 100.0
        } else {
            100.0 // Default to 100% if no data
        };

        InstanceStatus {
            provider: self.provider,
            instance: self.instance,
            healthy: self.healthy,
            success_rate,
            total_requests,
        }
    }
}

/// Parse a single Prometheus metric line
///
/// Returns (labels_map, value) or None if parsing fails
fn parse_metric_line(line: &str) -> Option<(std::collections::HashMap<String, String>, f64)> {
    // Example: llm_gateway_instance_health_status{provider="anthropic",instance="primary"} 1.0

    // Find the label section (between { and })
    let start = line.find('{')?;
    let end = line.find('}')?;
    let labels_str = &line[start + 1..end];

    // Parse labels
    let mut labels = std::collections::HashMap::new();
    for pair in labels_str.split(',') {
        let parts: Vec<&str> = pair.split('=').collect();
        if parts.len() == 2 {
            let key = parts[0].trim();
            let value = parts[1].trim().trim_matches('"');
            labels.insert(key.to_string(), value.to_string());
        }
    }

    // Parse value (after the })
    let value_str = line[end + 1..].trim();
    let value = value_str.parse::<f64>().ok()?;

    Some((labels, value))
}

/// Extract total request count from Prometheus metrics
fn extract_total_requests(metrics_json: &str) -> Result<f64> {
    // Look for llm_requests_total (sum across all dimensions)
    let mut total = 0.0;

    for line in metrics_json.lines() {
        if line.starts_with("llm_requests_total{") {
            if let Some((_, value)) = parse_metric_line(line) {
                total += value;
            }
        }
    }

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_metric_line() {
        let line = r#"llm_gateway_instance_health_status{provider="anthropic",instance="primary"} 1.0"#;
        let result = parse_metric_line(line);

        assert!(result.is_some());
        let (labels, value) = result.unwrap();

        assert_eq!(labels.get("provider"), Some(&"anthropic".to_string()));
        assert_eq!(labels.get("instance"), Some(&"primary".to_string()));
        assert_eq!(value, 1.0);
    }

    #[test]
    fn test_parse_instance_health_from_metrics() {
        let metrics = r#"
llm_gateway_instance_health_status{provider="anthropic",instance="primary"} 1
llm_instance_requests_total{provider="anthropic",instance="primary",status="success"} 100
llm_instance_requests_total{provider="anthropic",instance="primary",status="failure"} 5
llm_gateway_instance_health_status{provider="openai",instance="main"} 1
llm_instance_requests_total{provider="openai",instance="main",status="success"} 200
llm_instance_requests_total{provider="openai",instance="main",status="failure"} 0
"#;

        let result = parse_instance_health_from_metrics(metrics).unwrap();

        assert_eq!(result.instances.len(), 2);

        // Find anthropic instance
        let anthropic = result
            .instances
            .iter()
            .find(|i| i.provider == "anthropic")
            .unwrap();
        assert!(anthropic.healthy);
        assert_eq!(anthropic.total_requests, 105);
        assert!((anthropic.success_rate - 95.24).abs() < 0.1);

        // Find openai instance
        let openai = result.instances.iter().find(|i| i.provider == "openai").unwrap();
        assert!(openai.healthy);
        assert_eq!(openai.total_requests, 200);
        assert_eq!(openai.success_rate, 100.0);

        // Overall error rate should be around 2.5%
        assert!((result.overall_error_rate - 1.64).abs() < 0.5);
    }

    #[test]
    fn test_extract_total_requests() {
        let metrics = r#"
llm_requests_total{api_key="key1",provider="openai"} 50
llm_requests_total{api_key="key2",provider="anthropic"} 75
"#;

        let total = extract_total_requests(metrics).unwrap();
        assert_eq!(total, 125.0);
    }
}
