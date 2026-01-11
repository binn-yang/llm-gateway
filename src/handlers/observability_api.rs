//! Observability HTTP API handlers
//!
//! Provides RESTful API for querying logs, traces, and metrics.
//! AI-friendly with JSON responses and structured query support.

use crate::error::AppError;
use crate::observability::query_dsl::{QueryDSL, QueryResult, QueryType};
use crate::observability::{LogFilter, ObservabilityDb, TraceTree};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Shared state for observability API
#[derive(Clone)]
pub struct ObservabilityState {
    pub db: Arc<ObservabilityDb>,
}

/// Query parameters for logs API
#[derive(Debug, Deserialize)]
pub struct LogQueryParams {
    /// Filter by log level (ERROR, WARN, INFO, DEBUG, TRACE)
    pub level: Option<String>,

    /// Filter by request ID
    pub request_id: Option<String>,

    /// Filter by span ID
    pub span_id: Option<String>,

    /// Filter by target (module path)
    pub target: Option<String>,

    /// Grep pattern for message content
    pub grep: Option<String>,

    /// Show logs since N seconds ago
    pub since: Option<u64>,

    /// Show logs until N seconds ago
    pub until: Option<u64>,

    /// Maximum number of results
    #[serde(default = "default_limit")]
    pub limit: usize,

    /// Show oldest first (default: newest first)
    #[serde(default)]
    pub oldest_first: bool,
}

fn default_limit() -> usize {
    100
}

/// Response for logs query
#[derive(Debug, Serialize)]
pub struct LogsResponse {
    pub total: usize,
    pub logs: Vec<crate::observability::LogEntry>,
}

/// Response for trace query
#[derive(Debug, Serialize)]
pub struct TraceResponse {
    pub request_id: String,
    pub found: bool,
    pub trace: Option<TraceTree>,
}

/// GET /api/v1/logs - Query logs with filtering
///
/// Example: GET /api/v1/logs?level=error&limit=10&since=3600
pub async fn get_logs(
    State(state): State<ObservabilityState>,
    Query(params): Query<LogQueryParams>,
) -> Result<Json<LogsResponse>, AppError> {
    // Build filter
    let since_timestamp = params.since.map(|s| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now - (s * 1000)
    });

    let until_timestamp = params.until.map(|u| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now - (u * 1000)
    });

    let filter = LogFilter {
        level: params.level,
        request_id: params.request_id,
        span_id: params.span_id,
        target: params.target,
        grep: params.grep,
        since: since_timestamp,
        until: until_timestamp,
        limit: Some(params.limit),
        reverse: !params.oldest_first,
    };

    // Query logs
    let logs = state.db.query_logs(filter).await?;
    let total = logs.len();

    Ok(Json(LogsResponse { total, logs }))
}

/// GET /api/v1/trace/{request_id} - Get trace for a specific request
///
/// Example: GET /api/v1/trace/550e8400-e29b-41d4-a716-446655440000
pub async fn get_trace(
    State(state): State<ObservabilityState>,
    Path(request_id): Path<String>,
) -> Result<Json<TraceResponse>, AppError> {
    let trace = state.db.query_trace(&request_id).await?;

    let found = trace.root_span.is_some() || !trace.logs.is_empty();

    Ok(Json(TraceResponse {
        request_id: request_id.clone(),
        found,
        trace: if found { Some(trace) } else { None },
    }))
}

/// POST /api/v1/query - Structured query with DSL
///
/// Example:
/// ```json
/// {
///   "query_type": "logs",
///   "filters": [
///     {"field": "level", "op": "eq", "value": "ERROR"},
///     {"field": "timestamp", "op": "gte", "value": 1767863000000}
///   ],
///   "limit": 100
/// }
/// ```
pub async fn structured_query(
    State(state): State<ObservabilityState>,
    Json(query): Json<QueryDSL>,
) -> Result<Json<QueryResult>, AppError> {
    let start_time = std::time::Instant::now();

    // Validate query
    query.validate().map_err(|e| {
        AppError::ConversionError(format!("Invalid query: {}", e))
    })?;

    // Execute query based on type
    let results = match query.query_type {
        QueryType::Logs => {
            // Convert DSL to LogFilter (simplified)
            let mut filter = LogFilter::default();

            for f in &query.filters {
                match f.field.as_str() {
                    "level" => filter.level = Some(f.value.as_str().unwrap_or("").to_string()),
                    "request_id" => filter.request_id = Some(f.value.as_str().unwrap_or("").to_string()),
                    "span_id" => filter.span_id = Some(f.value.as_str().unwrap_or("").to_string()),
                    "target" => filter.target = Some(f.value.as_str().unwrap_or("").to_string()),
                    _ => {}
                }
            }

            filter.limit = Some(query.limit);
            filter.reverse = query.sort_order == "desc";

            let logs = state.db.query_logs(filter).await?;
            serde_json::to_value(&logs).unwrap_or_default()
        }
        QueryType::Spans => {
            // Placeholder for spans query
            serde_json::json!([])
        }
        QueryType::Metrics => {
            // Placeholder for metrics query
            serde_json::json!([])
        }
    };

    let execution_time_ms = start_time.elapsed().as_millis() as u64;

    let total = if let Some(arr) = results.as_array() {
        arr.len()
    } else {
        0
    };

    Ok(Json(QueryResult {
        query_type: query.query_type,
        results,
        total,
        execution_time_ms,
    }))
}

/// Health check endpoint for observability API
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({
        "status": "healthy",
        "component": "observability-api"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_limit() {
        assert_eq!(default_limit(), 100);
    }

    #[test]
    fn test_log_query_params_defaults() {
        let json = r#"{}"#;
        let params: LogQueryParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.limit, 100);
        assert!(!params.oldest_first);
    }
}
