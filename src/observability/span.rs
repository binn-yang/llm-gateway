//! Lightweight span context for request tracing
//!
//! This module provides a simple, non-OpenTelemetry span implementation
//! optimized for single-machine deployments.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Span kind (similar to OpenTelemetry, but simplified)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpanKind {
    /// Server-side request handler
    Server,
    /// Client-side outgoing request
    Client,
    /// Internal operation
    Internal,
}

/// Span context for distributed tracing
///
/// Each HTTP request creates a root span with a unique `request_id`.
/// Internal operations create child spans linked via `parent_span_id`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpanContext {
    /// Unique span identifier
    pub span_id: String,

    /// Parent span ID (None for root spans)
    pub parent_span_id: Option<String>,

    /// Request-level unique identifier (same for all spans in a request)
    pub request_id: String,

    /// Span name (e.g., "chat_completions", "load_balancer::select")
    pub name: String,

    /// Span kind
    pub kind: SpanKind,

    /// Start time (Unix milliseconds)
    pub start_time: u64,

    /// Custom attributes (JSON-serializable)
    #[serde(default)]
    pub attributes: serde_json::Value,
}

impl SpanContext {
    /// Create a new root span (typically at HTTP handler entry)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let span = SpanContext::new_root("chat_completions");
    /// tracing::info!(
    ///     request_id = %span.request_id,
    ///     span_id = %span.span_id,
    ///     "Request started"
    /// );
    /// ```
    pub fn new_root(name: impl Into<String>) -> Self {
        let request_id = Uuid::new_v4().to_string();
        let span_id = Uuid::new_v4().to_string();

        Self {
            span_id,
            parent_span_id: None,
            request_id,
            name: name.into(),
            kind: SpanKind::Server,
            start_time: current_millis(),
            attributes: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Create a child span inheriting `request_id` from parent
    ///
    /// # Example
    ///
    /// ```ignore
    /// let root_span = SpanContext::new_root("chat_completions");
    /// let child_span = root_span.child("load_balancer::select");
    /// ```
    pub fn child(&self, name: impl Into<String>) -> Self {
        Self {
            span_id: Uuid::new_v4().to_string(),
            parent_span_id: Some(self.span_id.clone()),
            request_id: self.request_id.clone(),
            name: name.into(),
            kind: SpanKind::Internal,
            start_time: current_millis(),
            attributes: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Create a child span for an outgoing client request
    pub fn child_client(&self, name: impl Into<String>) -> Self {
        let mut child = self.child(name);
        child.kind = SpanKind::Client;
        child
    }

    /// Add an attribute to the span
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut span = SpanContext::new_root("chat_completions");
    /// span.set_attribute("model", "gpt-4");
    /// span.set_attribute("provider", "openai");
    /// ```
    pub fn set_attribute(&mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) {
        if let serde_json::Value::Object(ref mut map) = self.attributes {
            map.insert(key.into(), value.into());
        }
    }

    /// Calculate duration since span start (in milliseconds)
    pub fn duration_ms(&self) -> u64 {
        current_millis().saturating_sub(self.start_time)
    }

    /// Get span end time (current time)
    pub fn end_time(&self) -> u64 {
        current_millis()
    }
}

/// Record for span storage in database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanRecord {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub request_id: String,
    pub name: String,
    pub kind: String,
    pub start_time: u64,
    pub end_time: Option<u64>,
    pub duration_ms: Option<u64>,
    pub status: String,
    pub attributes: String, // JSON
}

impl SpanRecord {
    /// Create a span record from context with "ok" status
    pub fn from_context(ctx: &SpanContext, status: &str) -> Self {
        let end_time = current_millis();
        let duration_ms = end_time.saturating_sub(ctx.start_time);

        Self {
            span_id: ctx.span_id.clone(),
            parent_span_id: ctx.parent_span_id.clone(),
            request_id: ctx.request_id.clone(),
            name: ctx.name.clone(),
            kind: format!("{:?}", ctx.kind).to_lowercase(),
            start_time: ctx.start_time,
            end_time: Some(end_time),
            duration_ms: Some(duration_ms),
            status: status.to_string(),
            attributes: ctx.attributes.to_string(),
        }
    }
}

/// Get current time as Unix milliseconds
///
/// This is used throughout the observability system for consistent timestamps.
pub fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_span_creation() {
        let span = SpanContext::new_root("test_handler");

        assert_eq!(span.name, "test_handler");
        assert_eq!(span.kind, SpanKind::Server);
        assert!(span.parent_span_id.is_none());
        assert!(!span.request_id.is_empty());
        assert!(!span.span_id.is_empty());
    }

    #[test]
    fn test_child_span_inherits_request_id() {
        let root = SpanContext::new_root("root");
        let child = root.child("child");

        assert_eq!(child.request_id, root.request_id);
        assert_eq!(child.parent_span_id, Some(root.span_id.clone()));
        assert_eq!(child.kind, SpanKind::Internal);
        assert_ne!(child.span_id, root.span_id);
    }

    #[test]
    fn test_span_attributes() {
        let mut span = SpanContext::new_root("test");
        span.set_attribute("key1", "value1");
        span.set_attribute("key2", 42);

        if let serde_json::Value::Object(map) = &span.attributes {
            assert_eq!(map.get("key1").and_then(|v| v.as_str()), Some("value1"));
            assert_eq!(map.get("key2").and_then(|v| v.as_i64()), Some(42));
        } else {
            panic!("Attributes should be an object");
        }
    }

    #[test]
    fn test_duration_calculation() {
        let span = SpanContext::new_root("test");
        std::thread::sleep(std::time::Duration::from_millis(10));

        let duration = span.duration_ms();
        assert!(duration >= 10, "Duration should be at least 10ms, got {}", duration);
    }

    #[test]
    fn test_current_millis() {
        let now1 = current_millis();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let now2 = current_millis();

        assert!(now2 > now1, "Time should advance");
        assert!(now2 - now1 >= 5, "At least 5ms should have passed");
    }

    #[test]
    fn test_span_record_creation() {
        let mut span = SpanContext::new_root("test");
        span.set_attribute("provider", "openai");

        let record = SpanRecord::from_context(&span, "ok");

        assert_eq!(record.span_id, span.span_id);
        assert_eq!(record.request_id, span.request_id);
        assert_eq!(record.status, "ok");
        assert!(record.end_time.is_some());
        assert!(record.duration_ms.is_some());
    }
}
