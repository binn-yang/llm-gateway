//! Metrics snapshot module
//!
//! Periodically captures Prometheus metrics and stores them in SQLite
//! for historical analysis and trending.

use super::database::ObservabilityDb;
use anyhow::Result;
use metrics_exporter_prometheus::PrometheusHandle;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

/// Spawn background task to snapshot metrics periodically
///
/// # Arguments
///
/// * `handle` - Prometheus metrics handle
/// * `db` - Observability database
/// * `interval` - Snapshot interval
///
/// # Example
///
/// ```ignore
/// let interval = Duration::from_secs(300); // 5 minutes
/// spawn_snapshot_task(metrics_handle, db, interval);
/// ```
pub fn spawn_snapshot_task(
    handle: Arc<PrometheusHandle>,
    db: Arc<ObservabilityDb>,
    interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        snapshot_loop(handle, db, interval).await;
    })
}

/// Main snapshot loop
async fn snapshot_loop(
    handle: Arc<PrometheusHandle>,
    db: Arc<ObservabilityDb>,
    interval: Duration,
) {
    let mut ticker = time::interval(interval);

    loop {
        ticker.tick().await;

        match capture_snapshot(&handle, &db).await {
            Ok(()) => {
                tracing::debug!("Metrics snapshot captured successfully");
            }
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "Failed to capture metrics snapshot"
                );
            }
        }
    }
}

/// Capture a metrics snapshot
async fn capture_snapshot(handle: &PrometheusHandle, db: &ObservabilityDb) -> Result<()> {
    // Get current metrics in Prometheus text format
    let metrics_text = handle.render();

    // Parse and aggregate metrics
    let metrics = parse_prometheus_metrics(&metrics_text)?;

    // Store snapshot
    db.insert_metrics_snapshot(&metrics).await?;

    Ok(())
}

/// Parse Prometheus metrics from text format
///
/// This is a simplified parser that extracts metric names and values.
/// For production use, consider using a proper Prometheus parser library.
fn parse_prometheus_metrics(text: &str) -> Result<serde_json::Value> {
    let mut counters = serde_json::Map::new();
    let mut gauges = serde_json::Map::new();
    let mut histograms = serde_json::Map::new();

    for line in text.lines() {
        // Skip comments and empty lines
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        // Parse metric line: metric_name{labels} value
        if let Some((name_with_labels, value_str)) = line.rsplit_once(' ') {
            // Extract metric name (before '{' or entire name if no labels)
            let metric_name = if let Some(pos) = name_with_labels.find('{') {
                &name_with_labels[..pos]
            } else {
                name_with_labels
            };

            // Parse value
            if let Ok(value) = value_str.parse::<f64>() {
                // Classify by metric type (simple heuristic)
                if metric_name.ends_with("_total") {
                    counters.insert(metric_name.to_string(), serde_json::json!(value));
                } else if metric_name.ends_with("_bucket")
                    || metric_name.ends_with("_sum")
                    || metric_name.ends_with("_count")
                {
                    histograms.insert(metric_name.to_string(), serde_json::json!(value));
                } else {
                    gauges.insert(metric_name.to_string(), serde_json::json!(value));
                }
            }
        }
    }

    Ok(serde_json::json!({
        "counters": counters,
        "gauges": gauges,
        "histograms": histograms,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_prometheus_metrics() {
        let text = r#"
# HELP llm_requests_total Total number of LLM requests
# TYPE llm_requests_total counter
llm_requests_total{api_key="test",provider="openai"} 100
# HELP llm_gateway_instance_health_status Instance health status
# TYPE llm_gateway_instance_health_status gauge
llm_gateway_instance_health_status{provider="openai",instance="primary"} 1.0
"#;

        let metrics = parse_prometheus_metrics(text).unwrap();
        let obj = metrics.as_object().unwrap();

        assert!(obj.contains_key("counters"));
        assert!(obj.contains_key("gauges"));
        assert!(obj.contains_key("histograms"));

        let counters = obj["counters"].as_object().unwrap();
        assert!(counters.contains_key("llm_requests_total"));

        let gauges = obj["gauges"].as_object().unwrap();
        assert!(gauges.contains_key("llm_gateway_instance_health_status"));
    }

    #[test]
    fn test_parse_empty_metrics() {
        let text = "";
        let metrics = parse_prometheus_metrics(text).unwrap();
        let obj = metrics.as_object().unwrap();

        assert_eq!(obj["counters"].as_object().unwrap().len(), 0);
        assert_eq!(obj["gauges"].as_object().unwrap().len(), 0);
    }
}
