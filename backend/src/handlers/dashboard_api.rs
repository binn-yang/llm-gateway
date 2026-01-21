//! Dashboard HTTP API handlers
//!
//! Provides RESTful API for the Vue dashboard:
//! - Real-time stats aggregation
//! - Configuration management
//! - Time-series queries from SQLite

use crate::config::Config;
use crate::error::AppError;
use crate::load_balancer::LoadBalancer;
use crate::router::Provider;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Json};
use axum::Router;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

/// Dashboard state shared across all dashboard API handlers
#[derive(Clone)]
pub struct DashboardState {
    pub config: Arc<arc_swap::ArcSwap<crate::config::Config>>,
    pub db_pool: Option<SqlitePool>,
    pub load_balancers: Arc<arc_swap::ArcSwap<HashMap<Provider, Arc<LoadBalancer>>>>,
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
// Helper Functions
// ============================================================================

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

    // Try to get data from SQLite requests table
    let (total_requests, total_tokens, today_requests, today_tokens) = if let Some(pool) = &state.db_pool {
        // Query total requests and tokens from SQLite (all time)
        let total_results = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM requests"
        )
        .fetch_one(pool)
        .await
        .ok();

        // Query today's requests and tokens from SQLite (filtered by date)
        let today_results = sqlx::query_as::<_, (i64, i64)>(
            "SELECT COUNT(*), COALESCE(SUM(total_tokens), 0) FROM requests WHERE date = ?"
        )
        .bind(&today)
        .fetch_one(pool)
        .await
        .ok();

        match (total_results, today_results) {
            (Some(total), Some(today)) => (total.0, total.1, today.0, today.1),
            _ => (0, 0, 0, 0), // Fallback to zero if query fails
        }
    } else {
        // No SQLite pool, return zeros
        (0, 0, 0, 0)
    };

    // Get system health from LoadBalancer memory (real-time health status)
    let load_balancers = state.load_balancers.load();
    let health_status = if instance_count > 0 {
        let mut healthy_count = 0;
        for (_provider, load_balancer) in load_balancers.iter() {
            let health_infos = load_balancer.get_all_instances_health().await;
            healthy_count += health_infos.iter().filter(|h| h.is_healthy).count();
        }
        healthy_count as f64 / instance_count as f64 >= 0.5
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
        "model" => "provider || ':' || model",
        "api_key" => "api_key_name",
        "instance" => "provider || ':' || instance",
        _ => return Err(AppError::ConfigError(format!("Invalid group_by: {}", group_by))),
    };

    let query_str = format!(
        r#"
        SELECT
            {} as label,
            date || 'T' || printf('%02d', hour) || ':00:00' as timestamp,
            CAST(SUM(total_tokens) AS INTEGER) as tokens,
            CAST(COUNT(*) AS INTEGER) as requests
        FROM requests
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

/// Response for model statistics endpoint
#[derive(Debug, Serialize)]
pub struct ModelsStatsResponse {
    pub timestamp: String,         // ISO 8601 timestamp
    pub date: String,              // YYYY-MM-DD
    pub total_requests: i64,       // Total requests across all models
    pub total_tokens: i64,         // Total tokens across all models
    pub models: Vec<ModelStat>,
}

/// Statistics for a single model
#[derive(Debug, Serialize)]
pub struct ModelStat {
    pub model: String,                          // Model name
    pub requests: i64,                          // Number of requests
    pub tokens: i64,                            // Total tokens
    pub percentage: f64,                        // Percentage of total tokens (0.0-100.0)
    pub input_tokens: i64,                      // Input tokens
    pub output_tokens: i64,                     // Output tokens
    pub cache_creation_input_tokens: i64,       // Cache creation input tokens
    pub cache_read_input_tokens: i64,           // Cache read input tokens
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

/// GET /api/dashboard/models-stats
///
/// Returns today's model statistics including request counts, token usage,
/// and percentage of total tokens per model.
pub async fn get_models_stats(
    State(state): State<DashboardState>,
) -> Result<Json<ModelsStatsResponse>, AppError> {
    let pool = state.db_pool.as_ref()
        .ok_or_else(|| AppError::ConfigError("Observability not enabled".to_string()))?;

    // Get today's date
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Query per-model stats with detailed token breakdown
    let model_rows = sqlx::query_as::<_, (String, i64, i64, i64, i64, i64, i64)>(
        "SELECT
            model,
            COUNT(*) as requests,
            COALESCE(SUM(total_tokens), 0) as tokens,
            COALESCE(SUM(input_tokens), 0) as input_tokens,
            COALESCE(SUM(output_tokens), 0) as output_tokens,
            COALESCE(SUM(cache_creation_input_tokens), 0) as cache_creation_input_tokens,
            COALESCE(SUM(cache_read_input_tokens), 0) as cache_read_input_tokens
         FROM requests
         WHERE date = ?1
         GROUP BY model
         ORDER BY tokens DESC"
    )
    .bind(&today)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::ConversionError(format!("Query failed: {}", e)))?;

    // Query total tokens for percentage calculation
    let total_tokens_row = sqlx::query_as::<_, (i64,)>(
        "SELECT COALESCE(SUM(total_tokens), 0) FROM requests WHERE date = ?1"
    )
    .bind(&today)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::ConversionError(format!("Query failed: {}", e)))?;

    let total_tokens = total_tokens_row.0;
    let total_requests: i64 = model_rows.iter().map(|(_, r, _, _, _, _, _)| r).sum();

    // Build model stats with percentage
    let models: Vec<ModelStat> = model_rows
        .into_iter()
        .map(|(model, requests, tokens, input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens)| {
            let percentage = if total_tokens > 0 {
                (tokens as f64 / total_tokens as f64) * 100.0
            } else {
                0.0
            };

            ModelStat {
                model,
                requests,
                tokens,
                percentage,
                input_tokens,
                output_tokens,
                cache_creation_input_tokens,
                cache_read_input_tokens,
            }
        })
        .collect();

    Ok(Json(ModelsStatsResponse {
        timestamp: chrono::Utc::now().to_rfc3339(),
        date: today,
        total_requests,
        total_tokens,
        models,
    }))
}

/// Create the dashboard API router
pub fn create_dashboard_router(state: DashboardState) -> Router {
    Router::new()
        // Configuration endpoint
        .route("/config", axum::routing::get(get_config))
        // Summary endpoint
        .route("/summary", axum::routing::get(get_summary))
        // Time-series endpoints
        .route("/timeseries/tokens", axum::routing::get(get_timeseries_tokens))
        .route("/timeseries/health", axum::routing::get(get_timeseries_health))
        // Instance health monitoring
        .route("/instances-health", axum::routing::get(get_instances_health))
        // Model statistics
        .route("/models-stats", axum::routing::get(get_models_stats))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

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
