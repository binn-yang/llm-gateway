//! Analysis API handlers
//!
//! Provides intelligent analysis endpoints for performance and error patterns.

use crate::error::AppError;
use crate::handlers::observability_api::ObservabilityState;
use crate::observability::{ErrorPattern, SlowRequest};
use axum::extract::{Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};

/// Query parameters for slow requests analysis
#[derive(Debug, Deserialize)]
pub struct SlowRequestParams {
    /// Minimum duration in milliseconds
    #[serde(default = "default_threshold")]
    pub threshold: u64,

    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_threshold() -> u64 {
    5000 // 5 seconds
}

fn default_limit() -> usize {
    10
}

/// Response for slow requests analysis
#[derive(Debug, Serialize)]
pub struct SlowRequestsResponse {
    pub threshold_ms: u64,
    pub total_found: usize,
    pub requests: Vec<SlowRequest>,
}

/// GET /api/v1/analyze/slow-requests - Analyze slow requests
///
/// Example: GET /api/v1/analyze/slow-requests?threshold=5000&limit=10
pub async fn analyze_slow_requests(
    State(state): State<ObservabilityState>,
    Query(params): Query<SlowRequestParams>,
) -> Result<Json<SlowRequestsResponse>, AppError> {
    let requests = state
        .db
        .query_slow_requests(params.threshold, params.limit)
        .await?;

    let total_found = requests.len();

    Ok(Json(SlowRequestsResponse {
        threshold_ms: params.threshold,
        total_found,
        requests,
    }))
}

/// Query parameters for error pattern analysis
#[derive(Debug, Deserialize)]
pub struct ErrorPatternParams {
    /// Time window in seconds
    #[serde(default = "default_window")]
    pub window: u64,

    /// Maximum number of patterns to return
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_window() -> u64 {
    3600 // 1 hour
}

/// Response for error pattern analysis
#[derive(Debug, Serialize)]
pub struct ErrorPatternsResponse {
    pub window_seconds: u64,
    pub total_patterns: usize,
    pub patterns: Vec<ErrorPattern>,
}

/// GET /api/v1/analyze/error-patterns - Analyze error patterns
///
/// Example: GET /api/v1/analyze/error-patterns?window=3600&limit=10
pub async fn analyze_error_patterns(
    State(state): State<ObservabilityState>,
    Query(params): Query<ErrorPatternParams>,
) -> Result<Json<ErrorPatternsResponse>, AppError> {
    let patterns = state
        .db
        .query_error_patterns(params.window, params.limit)
        .await?;

    let total_patterns = patterns.len();

    Ok(Json(ErrorPatternsResponse {
        window_seconds: params.window,
        total_patterns,
        patterns,
    }))
}

/// Instance health status
#[derive(Debug, Serialize)]
pub struct InstanceHealth {
    pub provider: String,
    pub instance: String,
    pub healthy: bool,
    pub error_rate: f64,
    pub total_requests: u64,
    pub failed_requests: u64,
}

/// Response for instance health analysis
#[derive(Debug, Serialize)]
pub struct InstanceHealthResponse {
    pub timestamp: u64,
    pub instances: Vec<InstanceHealth>,
    pub summary: HealthSummary,
}

#[derive(Debug, Serialize)]
pub struct HealthSummary {
    pub total_instances: usize,
    pub healthy_instances: usize,
    pub unhealthy_instances: usize,
    pub overall_error_rate: f64,
}

/// GET /api/v1/analyze/instance-health - Analyze instance health
///
/// Example: GET /api/v1/analyze/instance-health
pub async fn analyze_instance_health(
    State(state): State<ObservabilityState>,
) -> Result<Json<InstanceHealthResponse>, AppError> {
    // Query metrics from the most recent snapshot
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // For now, return mock data based on current metrics
    // In a real implementation, this would query the metrics_snapshots table
    // and parse instance health from the stored Prometheus metrics

    let instances = vec![
        InstanceHealth {
            provider: "openai".to_string(),
            instance: "ollama-local".to_string(),
            healthy: true,
            error_rate: 0.0,
            total_requests: 0,
            failed_requests: 0,
        },
        InstanceHealth {
            provider: "anthropic".to_string(),
            instance: "anthropic-primary".to_string(),
            healthy: true,
            error_rate: 0.0,
            total_requests: 0,
            failed_requests: 0,
        },
        InstanceHealth {
            provider: "anthropic".to_string(),
            instance: "anthropic-backup".to_string(),
            healthy: true,
            error_rate: 0.0,
            total_requests: 0,
            failed_requests: 0,
        },
    ];

    let healthy_count = instances.iter().filter(|i| i.healthy).count();
    let unhealthy_count = instances.len() - healthy_count;

    let total_reqs: u64 = instances.iter().map(|i| i.total_requests).sum();
    let total_failed: u64 = instances.iter().map(|i| i.failed_requests).sum();
    let overall_error_rate = if total_reqs > 0 {
        (total_failed as f64 / total_reqs as f64) * 100.0
    } else {
        0.0
    };

    Ok(Json(InstanceHealthResponse {
        timestamp,
        instances,
        summary: HealthSummary {
            total_instances: 3,
            healthy_instances: healthy_count,
            unhealthy_instances: unhealthy_count,
            overall_error_rate,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slow_request_params_defaults() {
        let json = r#"{}"#;
        let params: SlowRequestParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.threshold, 5000);
        assert_eq!(params.limit, 10);
    }

    #[test]
    fn test_error_pattern_params_defaults() {
        let json = r#"{}"#;
        let params: ErrorPatternParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.window, 3600);
        assert_eq!(params.limit, 10);
    }
}
