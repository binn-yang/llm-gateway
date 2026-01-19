//! Dashboard HTTP API handlers
//!
//! Provides RESTful API for the Vue dashboard:
//! - Metrics scraping from Prometheus endpoint
//! - Real-time stats aggregation
//! - Configuration management
//! - Time-series queries from SQLite

use crate::config::Config;
use crate::error::AppError;
use crate::load_balancer::{InstanceHealthInfo, LoadBalancer};
use crate::router::Provider;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Json};
use axum::Router;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use metrics_exporter_prometheus::PrometheusHandle;

/// Dashboard state shared across all dashboard API handlers
#[derive(Clone)]
pub struct DashboardState {
    pub config: Arc<arc_swap::ArcSwap<crate::config::Config>>,
    pub metrics_handle: Arc<PrometheusHandle>,
    pub db_pool: Option<SqlitePool>,
    pub load_balancers: Arc<arc_swap::ArcSwap<HashMap<Provider, Arc<LoadBalancer>>>>,
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

/// GET /api/dashboard/summary - Dashboard summary
///
/// Returns aggregated summary data for the dashboard top cards
pub async fn get_summary(
    State(state): State<DashboardState>,
) -> Result<Json<DashboardSummary>, AppError> {
    let config = state.config.load();
    let config_ref: &Config = &*config;

    // Count API keys
    let api_key_count = config_ref.api_keys.len();

    // Count providers and instances
    let provider_count = config_ref.providers.openai.len()
        + config_ref.providers.anthropic.len()
        + config_ref.providers.gemini.len();

    let instance_count = config_ref.providers.openai.len()
        + config_ref.providers.anthropic.len()
        + config_ref.providers.gemini.len();

    // Get today's date
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Try to get data from SQLite first (more accurate for time-series data)
    let (total_requests, total_tokens, today_requests, today_tokens) = if let Some(pool) = &state.db_pool {
        // Query total requests and tokens from SQLite (all time)
        let total_results = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(SUM(request_count), 0), COALESCE(SUM(total_tokens), 0) FROM token_usage"
        )
        .fetch_one(pool)
        .await
        .ok();

        // Query today's requests and tokens from SQLite (filtered by date)
        let today_results = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COALESCE(SUM(request_count), 0), COALESCE(SUM(total_tokens), 0) FROM token_usage WHERE date = ?"
        )
        .bind(&today)
        .fetch_one(pool)
        .await
        .ok();

        match (total_results, today_results) {
            (Some(total), Some(today)) => (total.0, total.1, today.0, today.1),
            _ => {
                // Fallback to Prometheus metrics if SQLite query fails
                let metrics_text = state.metrics_handle.render();
                let mut req_total = 0i64;
                let mut tok_total = 0i64;
                for line in metrics_text.lines() {
                    if line.starts_with('#') || line.trim().is_empty() {
                        continue;
                    }
                    if line.contains("llm_requests_total{") {
                        if let Some(value) = extract_metric_value_simple(&line) {
                            req_total += value;
                        }
                    } else if line.contains("llm_tokens_total{") {
                        if let Some(value) = extract_metric_value_simple(&line) {
                            tok_total += value;
                        }
                    }
                }
                (req_total, tok_total, req_total, tok_total)
            }
        }
    } else {
        // No SQLite pool, use Prometheus metrics
        let metrics_text = state.metrics_handle.render();
        let mut req_total = 0i64;
        let mut tok_total = 0i64;
        for line in metrics_text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            if line.contains("llm_requests_total{") {
                if let Some(value) = extract_metric_value_simple(&line) {
                    req_total += value;
                }
            } else if line.contains("llm_tokens_total{") {
                if let Some(value) = extract_metric_value_simple(&line) {
                    tok_total += value;
                }
            }
        }
        (req_total, tok_total, req_total, tok_total)
    };

    // Get system health from Prometheus metrics (real-time health status)
    let metrics_text = state.metrics_handle.render();
    let health_status = if instance_count > 0 {
        let healthy_instances = count_healthy_instances(&metrics_text);
        healthy_instances as f64 / instance_count as f64 >= 0.5
    } else {
        false
    };

    Ok(Json(DashboardSummary {
        api_key_count,
        provider_count,
        instance_count,
        today_requests,
        today_tokens,
        total_requests,
        total_tokens,
        health_status,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Extract simple metric value from Prometheus line
fn extract_metric_value_simple(line: &str) -> Option<i64> {
    if let Some(end_pos) = line.rfind('}') {
        let value_str = line[end_pos + 1..].trim();
        if let Ok(v) = value_str.parse::<f64>() {
            return Some(v as i64);
        }
    }
    None
}

/// Count healthy instances
fn count_healthy_instances(metrics_text: &str) -> i32 {
    let mut healthy_count = 0;
    for line in metrics_text.lines() {
        if line.contains("llm_gateway_instance_health_status{") && line.contains("} 1") {
            healthy_count += 1;
        }
    }
    healthy_count
}

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DashboardSummary {
    pub api_key_count: usize,
    pub provider_count: usize,
    pub instance_count: usize,
    pub today_requests: i64,
    pub today_tokens: i64,
    pub total_requests: i64,
    pub total_tokens: i64,
    pub health_status: bool,
    pub timestamp: String,
}

// ============================================================================
// Time-Series Query APIs
// ============================================================================

/// GET /api/dashboard/timeseries/tokens
///
/// Query token usage time-series data from SQLite
///
/// Query parameters:
/// - start_date: YYYY-MM-DD (required)
/// - end_date: YYYY-MM-DD (optional, defaults to today)
/// - group_by: provider | model | api_key | instance
/// - interval: hour | day (optional, defaults to day)
pub async fn get_timeseries_tokens(
    State(state): State<DashboardState>,
    Query(params): Query<TimeseriesQuery>,
) -> Result<Json<TimeseriesResponse>, AppError> {
    let pool = state.db_pool.as_ref()
        .ok_or_else(|| AppError::ConfigError("Observability not enabled".to_string()))?;

    // Parse dates
    let start_date = parse_date(&params.start_date)?;
    let end_date = params.end_date
        .as_ref()
        .map(|d| parse_date(d))
        .transpose()?
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    // Query token usage data
    let data = query_token_usage_timeseries(
        pool,
        start_date,
        end_date,
        &params.group_by,
        params.interval.as_deref().unwrap_or("day")
    ).await?;

    Ok(Json(TimeseriesResponse {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        group_by: params.group_by,
        interval: params.interval.unwrap_or_else(|| "day".to_string()),
        data,
    }))
}

/// GET /api/dashboard/timeseries/health
///
/// Query instance health status time-series data
///
/// Query parameters:
/// - start_date: YYYY-MM-DD (required)
/// - end_date: YYYY-MM-DD (optional, defaults to today)
/// - instance: string (optional, filter by specific instance)
pub async fn get_timeseries_health(
    State(state): State<DashboardState>,
    Query(params): Query<HealthTimeseriesQuery>,
) -> Result<Json<HealthTimeseriesResponse>, AppError> {
    let pool = state.db_pool.as_ref()
        .ok_or_else(|| AppError::ConfigError("Observability not enabled".to_string()))?;

    let start_date = parse_date(&params.start_date)?;
    let end_date = params.end_date
        .as_ref()
        .map(|d| parse_date(d))
        .transpose()?
        .unwrap_or_else(|| chrono::Utc::now().date_naive());

    let data = query_instance_health_timeseries(
        pool,
        start_date,
        end_date,
        params.instance.as_deref()
    ).await?;

    Ok(Json(HealthTimeseriesResponse {
        start_date: start_date.to_string(),
        end_date: end_date.to_string(),
        data,
    }))
}

// ============================================================================
// Query Implementations
// ============================================================================

/// Query token usage time-series from SQLite
async fn query_token_usage_timeseries(
    pool: &SqlitePool,
    start_date: NaiveDate,
    end_date: NaiveDate,
    group_by: &str,
    _interval: &str,
) -> Result<Vec<TimeseriesDataPoint>, AppError> {
    let group_clause = match group_by {
        "provider" => "provider",
        "model" => "provider, model",
        "api_key" => "api_key",
        "instance" => "provider, instance",
        _ => return Err(AppError::ConfigError(format!("Invalid group_by: {}", group_by))),
    };

    let query_str = format!(
        r#"
        SELECT
            {} as label,
            date || 'T' || printf('%02d', hour) || ':00:00' as timestamp,
            SUM(total_tokens) as tokens,
            SUM(request_count) as requests
        FROM token_usage
        WHERE date >= ?1 AND date <= ?2
        GROUP BY {}, date, hour
        ORDER BY date, hour
        "#,
        group_clause, group_clause
    );

    let rows = sqlx::query_as::<_, (String, String, i64, i64)>(&query_str)
        .bind(start_date.to_string())
        .bind(end_date.to_string())
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::ConversionError(format!("Query failed: {}", e)))?;

    Ok(rows.into_iter().map(|(label, timestamp, tokens, requests)| {
        TimeseriesDataPoint {
            label,
            timestamp,
            value: serde_json::json!({
                "tokens": tokens,
                "requests": requests
            }),
        }
    }).collect())
}

/// Query instance health time-series from SQLite
async fn query_instance_health_timeseries(
    pool: &SqlitePool,
    start_date: NaiveDate,
    end_date: NaiveDate,
    instance_filter: Option<&str>,
) -> Result<Vec<HealthDataPoint>, AppError> {
    let mut query_str = r#"
        SELECT
            provider,
            instance,
            date || 'T' || printf('%02d', hour) || ':00:00' as timestamp,
            health_status,
            failover_count
        FROM instance_health
        WHERE date >= ?1 AND date <= ?2
    "#.to_string();

    if let Some(instance) = instance_filter {
        query_str.push_str(&format!(" AND instance = '{}' ", instance));
    }

    query_str.push_str("ORDER BY date, hour");

    let rows = sqlx::query_as::<_, (String, String, String, String, i32)>(&query_str)
        .bind(start_date.to_string())
        .bind(end_date.to_string())
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::ConversionError(format!("Query failed: {}", e)))?;

    Ok(rows.into_iter().map(|(provider, instance, timestamp, health_status, failover_count)| {
        HealthDataPoint {
            provider,
            instance,
            timestamp,
            health_status,
            failover_count,
        }
    }).collect())
}

/// Parse date string in YYYY-MM-DD format
fn parse_date(date_str: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|_| AppError::ConversionError(format!("Invalid date format: {}, expected YYYY-MM-DD", date_str)))
}

// ============================================================================
// Data Structures for Time-Series APIs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct TimeseriesQuery {
    pub start_date: String,
    pub end_date: Option<String>,
    pub group_by: String,
    pub interval: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TimeseriesResponse {
    pub start_date: String,
    pub end_date: String,
    pub group_by: String,
    pub interval: String,
    pub data: Vec<TimeseriesDataPoint>,
}

#[derive(Debug, Serialize)]
pub struct TimeseriesDataPoint {
    pub label: String,
    pub timestamp: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct HealthTimeseriesQuery {
    pub start_date: String,
    pub end_date: Option<String>,
    pub instance: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthTimeseriesResponse {
    pub start_date: String,
    pub end_date: String,
    pub data: Vec<HealthDataPoint>,
}

#[derive(Debug, Serialize)]
pub struct HealthDataPoint {
    pub provider: String,
    pub instance: String,
    pub timestamp: String,
    pub health_status: String,
    pub failover_count: i32,
}

/// GET /api/dashboard/instances-health - Get all provider instances health status
///
/// Returns real-time health information for all provider instances including:
/// - Current health status
/// - Duration in current state
/// - Cumulative downtime in the last 24 hours
pub async fn get_instances_health(
    State(state): State<DashboardState>,
) -> Result<Json<InstancesHealthResponse>, AppError> {
    let mut instances_health = Vec::new();

    // Get health info from all load balancers
    let load_balancers = state.load_balancers.load();
    for (_provider, load_balancer) in load_balancers.iter() {
        let health_infos = load_balancer.get_all_instances_health().await;
        instances_health.extend(health_infos);
    }

    // For each instance, query cumulative downtime from SQLite (last 24 hours)
    let instances_with_downtime: Vec<InstanceHealthDetail> = if let Some(pool) = &state.db_pool {
        let yesterday = chrono::Utc::now() - chrono::Duration::days(1);
        let yesterday_str = yesterday.format("%Y-%m-%d").to_string();

        futures::future::join_all(instances_health.iter().map(|info| async {
            // Calculate cumulative downtime: count minutes where health_status = 'unhealthy'
            let downtime_secs = sqlx::query_as::<_, (i64,)>(
                r#"
                SELECT COUNT(*) * 60
                FROM instance_health
                WHERE provider = ?1
                  AND instance = ?2
                  AND timestamp >= (
                    SELECT datetime((julianday(?3) - 2440587.5) * 86400.0, 'unixepoch')
                  )
                  AND health_status = 'unhealthy'
                "#
            )
            .bind(&info.provider)
            .bind(&info.instance)
            .bind(&yesterday_str)
            .fetch_one(pool)
            .await
            .ok()
            .map(|(count,)| count as u64)
            .unwrap_or(0);

            InstanceHealthDetail {
                provider: info.provider.clone(),
                instance: info.instance.clone(),
                is_healthy: info.is_healthy,
                duration_secs: info.duration_secs,
                downtime_last_24h_secs: downtime_secs,
            }
        })).await
    } else {
        // No SQLite, return health info without downtime data
        instances_health.into_iter().map(|info| InstanceHealthDetail {
            provider: info.provider,
            instance: info.instance,
            is_healthy: info.is_healthy,
            duration_secs: info.duration_secs,
            downtime_last_24h_secs: 0,
        }).collect()
    };

    Ok(Json(InstancesHealthResponse {
        timestamp: chrono::Utc::now().to_rfc3339(),
        instances: instances_with_downtime,
    }))
}

#[derive(Debug, Serialize)]
pub struct InstancesHealthResponse {
    pub timestamp: String,
    pub instances: Vec<InstanceHealthDetail>,
}

#[derive(Debug, Serialize)]
pub struct InstanceHealthDetail {
    pub provider: String,
    pub instance: String,
    pub is_healthy: bool,
    pub duration_secs: u64,
    pub downtime_last_24h_secs: u64,
}

/// Create the dashboard API router
pub fn create_dashboard_router(state: DashboardState) -> Router {
    Router::new()
        // Existing endpoints
        .route("/metrics", axum::routing::get(get_metrics))
        .route("/stats", axum::routing::get(get_stats))
        .route("/config", axum::routing::get(get_config))
        .route("/summary", axum::routing::get(get_summary))
        // New time-series endpoints
        .route("/timeseries/tokens", axum::routing::get(get_timeseries_tokens))
        .route("/timeseries/health", axum::routing::get(get_timeseries_health))
        // Instance health monitoring
        .route("/instances-health", axum::routing::get(get_instances_health))
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
