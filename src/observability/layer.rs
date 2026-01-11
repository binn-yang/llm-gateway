//! Tracing layer for observability
//!
//! This module provides a custom tracing layer that writes logs to SQLite.

use super::{database::LogEntry, writer::AsyncWriter};
use std::sync::Arc;
use tracing::{Event, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

/// Custom tracing layer that writes logs to observability database
pub struct ObservabilityLayer {
    writer: Arc<AsyncWriter>,
}

impl ObservabilityLayer {
    pub fn new(writer: Arc<AsyncWriter>) -> Self {
        Self { writer }
    }
}

impl<S> Layer<S> for ObservabilityLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Extract event metadata
        let metadata = event.metadata();
        let level = metadata.level().to_string();
        let target = metadata.target().to_string();

        // Visitor to extract fields from the event
        struct FieldVisitor {
            message: Option<String>,
            request_id: Option<String>,
            span_id: Option<String>,
            fields: serde_json::Map<String, serde_json::Value>,
        }

        impl tracing::field::Visit for FieldVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let name = field.name();
                let value_str = format!("{:?}", value);

                match name {
                    "message" => {
                        self.message = Some(value_str.trim_matches('"').to_string());
                    }
                    "request_id" => {
                        self.request_id = Some(value_str.trim_matches('"').to_string());
                    }
                    "span_id" => {
                        self.span_id = Some(value_str.trim_matches('"').to_string());
                    }
                    _ => {
                        self.fields.insert(
                            name.to_string(),
                            serde_json::Value::String(value_str),
                        );
                    }
                }
            }

            fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                let name = field.name();
                match name {
                    "message" => {
                        self.message = Some(value.to_string());
                    }
                    "request_id" => {
                        self.request_id = Some(value.to_string());
                    }
                    "span_id" => {
                        self.span_id = Some(value.to_string());
                    }
                    _ => {
                        self.fields.insert(
                            name.to_string(),
                            serde_json::Value::String(value.to_string()),
                        );
                    }
                }
            }
        }

        let mut visitor = FieldVisitor {
            message: None,
            request_id: None,
            span_id: None,
            fields: serde_json::Map::new(),
        };

        event.record(&mut visitor);

        // Create log entry
        let log_entry = LogEntry {
            timestamp: super::span::current_millis(),
            level,
            target,
            message: visitor.message.unwrap_or_default(),
            request_id: visitor.request_id,
            span_id: visitor.span_id,
            fields: serde_json::to_string(&visitor.fields).unwrap_or_else(|_| "{}".to_string()),
        };

        // Write to database asynchronously
        self.writer.write_log(log_entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::{database::ObservabilityDb, writer::AsyncWriter};
    use std::sync::Arc;
    use std::time::Duration;

    #[tokio::test]
    async fn test_observability_layer() {
        // Create in-memory database
        let db = ObservabilityDb::new("sqlite::memory:").await.unwrap();
        let writer = AsyncWriter::spawn(Arc::new(db), 10, Duration::from_millis(50));
        let writer = Arc::new(writer);

        // Create layer
        let _layer = ObservabilityLayer::new(writer.clone());

        // The layer will be tested through integration tests
        // where actual tracing events are emitted
    }
}
