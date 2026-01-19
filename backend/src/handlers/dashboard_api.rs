//! Dashboard HTTP API handlers
//!
//! Provides RESTful API for the Vue dashboard:
//! - Metrics scraping from Prometheus endpoint
//! - Real-time stats aggregation
//! - Configuration management

use crate::config::Config;
use crate::error::AppError;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Json};
use axum::Router;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use metrics_exporter_prometheus::PrometheusHandle;

/// Dashboard state shared across all dashboard API handlers
#[derive(Clone)]
pub struct DashboardState {
    pub config: Arc<arc_swap::ArcSwap<crate::config::Config>>,
    pub metrics_handle: Arc<PrometheusHandle>,
}

/// GET /api/dashboard/metrics - Scrape Prometheus metrics
///
/// Returns the raw Prometheus metrics text format.
/// The frontend will parse this to extract specific metrics.
pub async fn get_metrics(
    State(state): State<DashboardState>,
) -> Result<Json<MetricsResponse>, AppError> {
    let metrics_text = state.metrics_handle.render();

    Ok(Json(MetricsResponse {
        timestamp: chrono::Utc::now().to_rfc3339(),
        metrics: metrics_text,
    }))
}

/// GET /api/dashboard/stats - Aggregated statistics
///
/// Returns pre-aggregated statistics for the dashboard.
/// Query parameters:
/// - group_by: How to group metrics (provider, model, api_key, all)
pub async fn get_stats(
    State(state): State<DashboardState>,
    Query(params): Query<StatsQueryParams>,
) -> Result<Json<StatsResponse>, AppError> {
    let metrics_text = state.metrics_handle.render();

    // Parse and aggregate metrics
    let aggregated = aggregate_metrics(&metrics_text, &params.group_by)?;

    Ok(Json(StatsResponse {
        timestamp: chrono::Utc::now().to_rfc3339(),
        group_by: params.group_by,
        data: aggregated,
    }))
}

/// GET /api/dashboard/config - Current configuration
///
/// Returns the current configuration with secrets masked.
/// This is read-only and does not expose sensitive API keys.
pub async fn get_config(
    State(state): State<DashboardState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let config = state.config.load();
    // Dereference ArcSwap Guard and then the Arc to get Config
    let config_ref: &Config = &*config;

    // Serialize to JSON and mask secrets
    let config_value = serde_json::to_value(config_ref)
        .map_err(|e| AppError::ConversionError(e.to_string()))?;

    let masked = mask_secrets(config_value);

    Ok(Json(masked))
}

/// GET /api/dashboard/ - Serve the dashboard HTML
///
/// Returns the dashboard HTML page.
/// In production, this will be served by the static file handler.
pub async fn get_dashboard() -> impl IntoResponse {
    // TODO: Return the dashboard HTML once frontend is built
    // For now, return a placeholder
    Html("<html><body>Dashboard will be served here</body></html>")
}

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    pub timestamp: String,
    pub metrics: String,
}

#[derive(Debug, Deserialize)]
pub struct StatsQueryParams {
    #[serde(default = "default_group_by")]
    pub group_by: String,
}

fn default_group_by() -> String {
    "provider".to_string()
}

#[derive(Debug, Serialize)]
pub struct StatsResponse {
    pub timestamp: String,
    pub group_by: String,
    pub data: serde_json::Value,
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Aggregate Prometheus metrics into structured data
fn aggregate_metrics(metrics_text: &str, group_by: &str) -> Result<serde_json::Value, AppError> {
    let mut result = serde_json::Map::new();

    // Common metrics we want to extract
    let metric_names = vec![
        "llm_requests_total",
        "llm_tokens_total",
        "llm_request_duration_seconds_sum",
        "llm_request_duration_seconds_count",
        "llm_instance_health_status",
        "llm_instance_requests_total",
    ];

    for metric_name in metric_names {
        let value = extract_metric_value(metrics_text, metric_name, group_by)?;
        result.insert(metric_name.to_string(), value);
    }

    Ok(serde_json::to_value(result).unwrap())
}

/// Extract a specific metric value from Prometheus text format
fn extract_metric_value(
    metrics_text: &str,
    metric_name: &str,
    group_by: &str,
) -> Result<serde_json::Value, AppError> {
    let lines: Vec<&str> = metrics_text.lines().collect();
    let mut values = serde_json::Map::new();

    for line in lines {
        // Skip comments and empty lines
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }

        // Parse metric line
        // Format: metric_name{labels} value
        if let Some(pos) = line.find('{') {
            let name = &line[..pos];
            if name != metric_name {
                continue;
            }

            // Extract labels and value
            if let Some(end_pos) = line.rfind('}') {
                let labels_str = &line[pos + 1..end_pos];
                let value_str = line[end_pos + 1..].trim();

                // Parse labels
                let labels = parse_labels(labels_str);

                // Group by specified key
                if let Some(group_key) = get_group_key(group_by, &labels) {
                    if let Ok(value) = value_str.parse::<f64>() {
                        let entry = values.entry(group_key).or_insert(serde_json::json!(0.0f64));
                        if let Some(num) = entry.as_f64() {
                            *entry = serde_json::json!(num + value);
                        }
                    }
                }
            }
        } else if line.starts_with(metric_name) {
            // Metric without labels
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(value) = parts[1].parse::<f64>() {
                    let entry = values.entry("total".to_string()).or_insert(serde_json::json!(0.0f64));
                    if let Some(num) = entry.as_f64() {
                        *entry = serde_json::json!(num + value);
                    }
                }
            }
        }
    }

    Ok(serde_json::to_value(values).unwrap())
}

/// Parse Prometheus labels string into a map
fn parse_labels(labels_str: &str) -> std::collections::HashMap<String, String> {
    let mut labels = std::collections::HashMap::new();

    // Split by comma, but handle quoted strings
    let parts: Vec<&str> = labels_str.split(',').collect();

    for part in parts {
        let part = part.trim();
        if let Some(eq_pos) = part.find('=') {
            let key = part[..eq_pos].trim().to_string();
            let value = part[eq_pos + 1..].trim().to_string();
            labels.insert(key, value);
        }
    }

    labels
}

/// Get the group key based on group_by parameter
fn get_group_key(group_by: &str, labels: &std::collections::HashMap<String, String>) -> Option<String> {
    match group_by {
        "provider" => labels.get("provider").cloned(),
        "model" => labels.get("model").cloned(),
        "api_key" => labels.get("api_key").cloned(),
        "instance" => labels.get("instance").cloned(),
        "all" => Some("all".to_string()),
        _ => Some("unknown".to_string()),
    }
}

/// Mask secret values in configuration JSON
fn mask_secrets(mut config: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = config.as_object_mut() {
        // Mask API keys
        if let Some(providers) = obj.get_mut("providers").and_then(|v| v.as_object_mut()) {
            for (_provider_name, provider_config) in providers {
                if let Some(provider_obj) = provider_config.as_object_mut() {
                    if let Some(instances) = provider_obj.get_mut("instances").and_then(|v| v.as_array_mut()) {
                        for instance in instances {
                            if let Some(instance_obj) = instance.as_object_mut() {
                                if let Some(api_key) = instance_obj.get_mut("api_key") {
                                    *api_key = serde_json::json!("***MASKED***");
                                }
                            }
                        }
                    }
                }
            }
        }

        // Mask authentication tokens
        if let Some(auth) = obj.get_mut("auth").and_then(|v| v.as_object_mut()) {
            for (_key, value) in auth.iter_mut() {
                if let Some(token) = value.as_str() {
                    if token.len() > 10 {
                        *value = serde_json::json!(format!("{}***", &token[..8]));
                    }
                }
            }
        }
    }

    config
}

// ============================================================================
// Router Setup
// ============================================================================

/// Create the dashboard API router
pub fn create_dashboard_router(state: DashboardState) -> Router {
    Router::new()
        .route("/metrics", axum::routing::get(get_metrics))
        .route("/stats", axum::routing::get(get_stats))
        .route("/config", axum::routing::get(get_config))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_labels() {
        let labels_str = r#"provider="openai",model="gpt-4",api_key="sk-test""#;
        let labels = parse_labels(labels_str);

        assert_eq!(labels.get("provider"), Some(&"openai".to_string()));
        assert_eq!(labels.get("model"), Some(&"gpt-4".to_string()));
        assert_eq!(labels.get("api_key"), Some(&"sk-test".to_string()));
    }

    #[test]
    fn test_mask_secrets() {
        let config_json = r#"{
            "providers": {
                "openai": {
                    "instances": [{
                        "api_key": "sk-1234567890"
                    }]
                }
            }
        }"#;

        let config: serde_json::Value = serde_json::from_str(config_json).unwrap();
        let masked = mask_secrets(config);

        assert_eq!(
            masked["providers"]["openai"]["instances"][0]["api_key"],
            "***MASKED***"
        );
    }
}
