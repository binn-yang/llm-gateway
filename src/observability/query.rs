//! Query API for observability data
//!
//! Provides high-level query functions for logs, spans, and metrics.

use super::database::{LogEntry, ObservabilityDb};
use super::span::SpanRecord;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Filter for log queries
#[derive(Debug, Clone, Default)]
pub struct LogFilter {
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

    /// Start time (Unix milliseconds)
    pub since: Option<u64>,

    /// End time (Unix milliseconds)
    pub until: Option<u64>,

    /// Maximum number of results
    pub limit: Option<usize>,

    /// Show newest first (default: true)
    pub reverse: bool,
}

/// Trace tree representing request execution flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTree {
    pub request_id: String,
    pub root_span: Option<SpanNode>,
    pub logs: Vec<LogEntry>,
    pub total_duration_ms: Option<u64>,
}

/// Span node in trace tree (recursive structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanNode {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status: String,
    pub attributes: serde_json::Value,
    pub children: Vec<SpanNode>,
}

/// Slow request analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowRequest {
    pub request_id: String,
    pub total_duration_ms: u64,
    pub span_count: usize,
    pub slowest_span: Option<SlowSpanInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowSpanInfo {
    pub name: String,
    pub duration_ms: u64,
    pub percentage: f64,
}

impl ObservabilityDb {
    /// Query logs with flexible filtering
    ///
    /// # Example
    ///
    /// ```ignore
    /// let filter = LogFilter {
    ///     level: Some("ERROR".to_string()),
    ///     since: Some(current_millis() - 3600_000), // Last hour
    ///     limit: Some(100),
    ///     reverse: true,
    ///     ..Default::default()
    /// };
    /// let logs = db.query_logs(filter).await?;
    /// ```
    pub async fn query_logs(&self, filter: LogFilter) -> Result<Vec<LogEntry>> {
        let mut query = String::from("SELECT timestamp, level, target, message, request_id, span_id, fields FROM logs WHERE 1=1");

        let mut bindings: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send>> = Vec::new();

        // Build WHERE clause
        if let Some(level) = &filter.level {
            query.push_str(" AND level = ?");
            bindings.push(Box::new(level.clone()));
        }

        if let Some(request_id) = &filter.request_id {
            query.push_str(" AND request_id = ?");
            bindings.push(Box::new(request_id.clone()));
        }

        if let Some(span_id) = &filter.span_id {
            query.push_str(" AND span_id = ?");
            bindings.push(Box::new(span_id.clone()));
        }

        if let Some(target) = &filter.target {
            query.push_str(" AND target LIKE ?");
            bindings.push(Box::new(format!("%{}%", target)));
        }

        if let Some(grep) = &filter.grep {
            query.push_str(" AND message LIKE ?");
            bindings.push(Box::new(format!("%{}%", grep)));
        }

        if let Some(since) = filter.since {
            query.push_str(" AND timestamp >= ?");
            bindings.push(Box::new(since as i64));
        }

        if let Some(until) = filter.until {
            query.push_str(" AND timestamp <= ?");
            bindings.push(Box::new(until as i64));
        }

        // Order by
        if filter.reverse {
            query.push_str(" ORDER BY timestamp DESC");
        } else {
            query.push_str(" ORDER BY timestamp ASC");
        }

        // Limit
        if let Some(limit) = filter.limit {
            query.push_str(&format!(" LIMIT {}", limit));
        }

        // Execute query
        // Note: For simplicity, we'll use the existing query_logs_by_request for now
        // A more advanced implementation would use dynamic query building

        // Simplified implementation using existing methods
        if let Some(request_id) = &filter.request_id {
            return self.query_logs_by_request(request_id).await;
        }

        // Fallback: query all and filter in memory (not efficient for large datasets)
        let all_logs = self.query_all_logs(filter.limit.unwrap_or(1000)).await?;

        let filtered: Vec<LogEntry> = all_logs
            .into_iter()
            .filter(|log| {
                if let Some(ref level) = filter.level {
                    if &log.level != level {
                        return false;
                    }
                }
                if let Some(ref grep) = filter.grep {
                    if !log.message.contains(grep) {
                        return false;
                    }
                }
                if let Some(ref target) = filter.target {
                    if !log.target.contains(target) {
                        return false;
                    }
                }
                if let Some(since) = filter.since {
                    if log.timestamp < since {
                        return false;
                    }
                }
                if let Some(until) = filter.until {
                    if log.timestamp > until {
                        return false;
                    }
                }
                true
            })
            .collect();

        Ok(filtered)
    }

    /// Query all logs (with limit)
    async fn query_all_logs(&self, limit: usize) -> Result<Vec<LogEntry>> {
        let rows = sqlx::query(
            "SELECT timestamp, level, target, message, request_id, span_id, fields
             FROM logs
             ORDER BY timestamp DESC
             LIMIT ?"
        )
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await?;

        let logs = rows
            .into_iter()
            .map(|row| LogEntry {
                timestamp: row.get::<i64, _>("timestamp") as u64,
                level: row.get("level"),
                target: row.get("target"),
                message: row.get("message"),
                request_id: row.get("request_id"),
                span_id: row.get("span_id"),
                fields: row.get("fields"),
            })
            .collect();

        Ok(logs)
    }

    /// Query complete trace tree for a request
    ///
    /// Returns all spans and logs associated with a request ID,
    /// organized in a hierarchical tree structure.
    pub async fn query_trace(&self, request_id: &str) -> Result<TraceTree> {
        // Get all spans for this request
        let spans = self.query_spans_by_request(request_id).await?;

        // Get all logs for this request
        let logs = self.query_logs_by_request(request_id).await?;

        // Build span tree
        let root_span = if !spans.is_empty() {
            Some(Self::build_span_tree(&spans))
        } else {
            None
        };

        // Calculate total duration
        let total_duration_ms = spans
            .iter()
            .filter(|s| s.parent_span_id.is_none())
            .filter_map(|s| s.duration_ms)
            .next();

        Ok(TraceTree {
            request_id: request_id.to_string(),
            root_span,
            logs,
            total_duration_ms,
        })
    }

    /// Build hierarchical span tree from flat list
    fn build_span_tree(spans: &[SpanRecord]) -> SpanNode {
        // Find root span (no parent)
        let root = spans
            .iter()
            .find(|s| s.parent_span_id.is_none())
            .expect("No root span found");

        Self::build_span_node(root, spans)
    }

    /// Recursively build span node with children
    fn build_span_node(span: &SpanRecord, all_spans: &[SpanRecord]) -> SpanNode {
        let children: Vec<SpanNode> = all_spans
            .iter()
            .filter(|s| s.parent_span_id.as_ref() == Some(&span.span_id))
            .map(|child| Self::build_span_node(child, all_spans))
            .collect();

        SpanNode {
            span_id: span.span_id.clone(),
            parent_span_id: span.parent_span_id.clone(),
            name: span.name.clone(),
            kind: span.kind.clone(),
            start_time: span.start_time,
            end_time: span.end_time,
            duration_ms: span.duration_ms,
            status: span.status.clone(),
            attributes: serde_json::from_str(&span.attributes).unwrap_or_default(),
            children,
        }
    }

    /// Query slow requests above threshold
    ///
    /// # Arguments
    ///
    /// * `threshold_ms` - Minimum duration in milliseconds
    /// * `limit` - Maximum number of results
    ///
    /// # Example
    ///
    /// ```ignore
    /// let slow_requests = db.query_slow_requests(5000, 10).await?;
    /// ```
    pub async fn query_slow_requests(
        &self,
        threshold_ms: u64,
        limit: usize,
    ) -> Result<Vec<SlowRequest>> {
        let rows = sqlx::query(
            "SELECT
                request_id,
                MAX(duration_ms) as max_duration,
                COUNT(*) as span_count
             FROM spans
             WHERE duration_ms IS NOT NULL
             GROUP BY request_id
             HAVING max_duration >= ?
             ORDER BY max_duration DESC
             LIMIT ?"
        )
        .bind(threshold_ms as i64)
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await?;

        let mut results = Vec::new();

        for row in rows {
            let request_id: String = row.get("request_id");
            let max_duration: i64 = row.get("max_duration");
            let span_count: i64 = row.get("span_count");

            // Get slowest span details
            let spans = self.query_spans_by_request(&request_id).await?;
            let slowest_span = spans
                .iter()
                .filter_map(|s| {
                    s.duration_ms.map(|d| (s, d))
                })
                .max_by_key(|(_, d)| *d)
                .map(|(s, d)| SlowSpanInfo {
                    name: s.name.clone(),
                    duration_ms: d,
                    percentage: (d as f64 / max_duration as f64) * 100.0,
                });

            results.push(SlowRequest {
                request_id,
                total_duration_ms: max_duration as u64,
                span_count: span_count as usize,
                slowest_span,
            });
        }

        Ok(results)
    }

    /// Query error patterns within a time window
    ///
    /// # Arguments
    ///
    /// * `window_seconds` - Time window in seconds (e.g., 3600 for last hour)
    /// * `limit` - Maximum number of error patterns to return
    ///
    /// # Example
    ///
    /// ```ignore
    /// let patterns = db.query_error_patterns(3600, 10).await?;
    /// ```
    pub async fn query_error_patterns(
        &self,
        window_seconds: u64,
        limit: usize,
    ) -> Result<Vec<ErrorPattern>> {
        let since_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - (window_seconds * 1000);

        // Query error logs grouped by level and message pattern
        let rows = sqlx::query(
            "SELECT
                level,
                COUNT(*) as count,
                GROUP_CONCAT(DISTINCT json_extract(fields, '$.provider')) as providers,
                MIN(timestamp) as first_seen,
                MAX(timestamp) as last_seen
             FROM logs
             WHERE timestamp >= ? AND (level = 'ERROR' OR level = 'WARN')
             GROUP BY level, substr(message, 1, 100)
             ORDER BY count DESC
             LIMIT ?"
        )
        .bind(since_timestamp as i64)
        .bind(limit as i64)
        .fetch_all(self.pool())
        .await?;

        let mut patterns = Vec::new();
        for row in rows {
            let level: String = row.get("level");
            let count: i64 = row.get("count");
            let providers: Option<String> = row.get("providers");
            let first_seen: i64 = row.get("first_seen");
            let last_seen: i64 = row.get("last_seen");

            patterns.push(ErrorPattern {
                level,
                count: count as u64,
                providers: providers
                    .map(|p| p.split(',').map(|s| s.to_string()).collect())
                    .unwrap_or_default(),
                first_seen: first_seen as u64,
                last_seen: last_seen as u64,
                window_seconds,
            });
        }

        Ok(patterns)
    }
}

/// Error pattern analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorPattern {
    pub level: String,
    pub count: u64,
    pub providers: Vec<String>,
    pub first_seen: u64,
    pub last_seen: u64,
    pub window_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::span::{current_millis, SpanContext};
    use std::sync::Arc;

    async fn create_test_db() -> Arc<ObservabilityDb> {
        let db = ObservabilityDb::new("sqlite::memory:").await.unwrap();
        Arc::new(db)
    }

    #[tokio::test]
    async fn test_query_logs_with_filter() {
        let db = create_test_db().await;

        // Insert test logs
        let log1 = LogEntry {
            timestamp: current_millis(),
            level: "ERROR".to_string(),
            target: "test".to_string(),
            message: "Test error message".to_string(),
            request_id: Some("req1".to_string()),
            span_id: None,
            fields: "{}".to_string(),
        };

        db.insert_log(&log1).await.unwrap();

        // Query by level
        let filter = LogFilter {
            level: Some("ERROR".to_string()),
            ..Default::default()
        };

        let logs = db.query_logs(filter).await.unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, "ERROR");
    }

    #[tokio::test]
    async fn test_query_trace() {
        let db = create_test_db().await;

        let span_ctx = SpanContext::new_root("test_span");
        let span_record = SpanRecord::from_context(&span_ctx, "ok");

        db.insert_span(&span_record).await.unwrap();

        let trace = db.query_trace(&span_ctx.request_id).await.unwrap();

        assert_eq!(trace.request_id, span_ctx.request_id);
        assert!(trace.root_span.is_some());

        let root = trace.root_span.unwrap();
        assert_eq!(root.name, "test_span");
        assert_eq!(root.status, "ok");
    }
}
