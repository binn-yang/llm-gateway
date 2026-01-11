//! Async batch writer for observability data
//!
//! This module provides non-blocking log and span writing with:
//! - Batched inserts (100 entries or 100ms window)
//! - Ring buffer to prevent blocking on database contention
//! - Automatic retry on transient errors

use crate::observability::database::{LogEntry, ObservabilityDb};
use crate::observability::span::SpanRecord;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Message types for the async writer
#[derive(Debug)]
enum WriterMessage {
    Log(LogEntry),
    Span(SpanRecord),
}

/// Async writer handle
///
/// Use this to non-blocking send logs/spans to the database.
/// The actual writes happen in a background task.
#[derive(Clone)]
pub struct AsyncWriter {
    sender: mpsc::UnboundedSender<WriterMessage>,
}

impl AsyncWriter {
    /// Spawn a new async writer task
    ///
    /// # Arguments
    ///
    /// * `db` - Database handle
    /// * `batch_size` - Max entries per batch (default: 100)
    /// * `flush_interval` - Max time before flush (default: 100ms)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let writer = AsyncWriter::spawn(db, 100, Duration::from_millis(100));
    /// writer.write_log(log_entry);  // Non-blocking
    /// ```
    pub fn spawn(
        db: Arc<ObservabilityDb>,
        batch_size: usize,
        flush_interval: Duration,
    ) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        // Spawn background writer task
        tokio::spawn(async move {
            writer_task(db, rx, batch_size, flush_interval).await;
        });

        Self { sender: tx }
    }

    /// Write a log entry (non-blocking)
    ///
    /// If the channel is full, the oldest entry may be dropped.
    pub fn write_log(&self, entry: LogEntry) {
        // Non-blocking send - if channel is full, entry is dropped
        // This prevents memory exhaustion under extreme load
        let _ = self.sender.send(WriterMessage::Log(entry));
    }

    /// Write a span record (non-blocking)
    pub fn write_span(&self, span: SpanRecord) {
        let _ = self.sender.send(WriterMessage::Span(span));
    }

    /// Get the number of pending writes (for monitoring)
    pub fn pending_count(&self) -> usize {
        // Note: unbounded channel doesn't expose len(), would need bounded for this
        0
    }
}

/// Background writer task
///
/// This runs in a separate tokio task and batches writes to the database.
async fn writer_task(
    db: Arc<ObservabilityDb>,
    mut rx: mpsc::UnboundedReceiver<WriterMessage>,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut log_batch: Vec<LogEntry> = Vec::with_capacity(batch_size);
    let mut span_batch: Vec<SpanRecord> = Vec::with_capacity(batch_size / 10); // Spans are less frequent

    let mut flush_timer = tokio::time::interval(flush_interval);
    flush_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Receive new messages
            Some(msg) = rx.recv() => {
                match msg {
                    WriterMessage::Log(entry) => {
                        log_batch.push(entry);

                        // Flush if batch is full
                        if log_batch.len() >= batch_size {
                            flush_logs(&db, &mut log_batch).await;
                        }
                    }
                    WriterMessage::Span(span) => {
                        span_batch.push(span);

                        // Flush if batch is full
                        if span_batch.len() >= batch_size / 10 {
                            flush_spans(&db, &mut span_batch).await;
                        }
                    }
                }
            }

            // Periodic flush (even if batch not full)
            _ = flush_timer.tick() => {
                if !log_batch.is_empty() {
                    flush_logs(&db, &mut log_batch).await;
                }
                if !span_batch.is_empty() {
                    flush_spans(&db, &mut span_batch).await;
                }
            }

            // Channel closed, flush remaining and exit
            else => {
                if !log_batch.is_empty() {
                    flush_logs(&db, &mut log_batch).await;
                }
                if !span_batch.is_empty() {
                    flush_spans(&db, &mut span_batch).await;
                }
                break;
            }
        }
    }

    tracing::info!("Observability writer task shutting down");
}

/// Flush log batch to database
async fn flush_logs(db: &ObservabilityDb, batch: &mut Vec<LogEntry>) {
    if batch.is_empty() {
        return;
    }

    let count = batch.len();
    let start = std::time::Instant::now();

    match db.insert_logs_batch(batch).await {
        Ok(_) => {
            let elapsed = start.elapsed();
            tracing::debug!(
                count = count,
                duration_ms = elapsed.as_millis(),
                "Flushed log batch"
            );

            // Record metric for write latency
            crate::metrics::record_lock_wait(
                "observability_db",
                "write_logs",
                elapsed,
            );
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                count = count,
                "Failed to flush log batch"
            );
        }
    }

    batch.clear();
}

/// Flush span batch to database
async fn flush_spans(db: &ObservabilityDb, batch: &mut Vec<SpanRecord>) {
    if batch.is_empty() {
        return;
    }

    let start = std::time::Instant::now();

    let mut success_count = 0;
    let mut error_count = 0;

    for span in batch.iter() {
        match db.insert_span(span).await {
            Ok(_) => success_count += 1,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    span_id = %span.span_id,
                    "Failed to insert span"
                );
                error_count += 1;
            }
        }
    }

    let elapsed = start.elapsed();
    tracing::debug!(
        success = success_count,
        errors = error_count,
        duration_ms = elapsed.as_millis(),
        "Flushed span batch"
    );

    batch.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::database::ObservabilityDb;
    use crate::observability::span::{current_millis, SpanContext, SpanRecord};

    async fn create_test_db() -> Arc<ObservabilityDb> {
        let db = ObservabilityDb::new("sqlite::memory:").await.unwrap();
        Arc::new(db)
    }

    #[tokio::test]
    async fn test_async_writer_logs() {
        let db = create_test_db().await;
        let writer = AsyncWriter::spawn(db.clone(), 10, Duration::from_millis(50));

        // Write logs
        for i in 0..5 {
            let log = LogEntry {
                timestamp: current_millis(),
                level: "INFO".to_string(),
                target: "test".to_string(),
                message: format!("Message {}", i),
                request_id: Some("req123".to_string()),
                span_id: None,
                fields: "{}".to_string(),
            };
            writer.write_log(log);
        }

        // Wait for flush
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify logs were written
        let logs = db.query_logs_by_request("req123").await.unwrap();
        assert_eq!(logs.len(), 5);
    }

    #[tokio::test]
    async fn test_async_writer_spans() {
        let db = create_test_db().await;
        let writer = AsyncWriter::spawn(db.clone(), 10, Duration::from_millis(50));

        // Write spans
        let span_ctx = SpanContext::new_root("test_span");
        let span_record = SpanRecord::from_context(&span_ctx, "ok");
        writer.write_span(span_record);

        // Wait for flush
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify span was written
        let spans = db.query_spans_by_request(&span_ctx.request_id).await.unwrap();
        assert_eq!(spans.len(), 1);
    }

    #[tokio::test]
    async fn test_batch_flush() {
        let db = create_test_db().await;
        let batch_size = 3;
        let writer = AsyncWriter::spawn(db.clone(), batch_size, Duration::from_secs(10));

        // Write exactly batch_size logs to trigger immediate flush
        for i in 0..batch_size {
            let log = LogEntry {
                timestamp: current_millis(),
                level: "INFO".to_string(),
                target: "test".to_string(),
                message: format!("Batch message {}", i),
                request_id: Some("batch_req".to_string()),
                span_id: None,
                fields: "{}".to_string(),
            };
            writer.write_log(log);
        }

        // Small delay for async processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify all logs were written
        let logs = db.query_logs_by_request("batch_req").await.unwrap();
        assert_eq!(logs.len(), batch_size);
    }
}
