

//! Observability subsystem for llm-gateway
//!
//! This module provides a lightweight, single-machine observability solution with:
//! - **Logs**: Persistent structured logging to SQLite
//! - **Metrics**: Periodic Prometheus metrics snapshots
//! - **Traces**: Request-level span correlation
//!
//! ## Architecture
//!
//! ```text
//! Layer 1: Collection (In-Memory)
//!     ↓
//! Layer 2: Persistence (SQLite + Async Writer)
//!     ↓
//! Layer 3: Query (CLI + HTTP API)
//! ```
//!
//! ## Design Principles
//!
//! - **Zero external dependencies**: Only SQLite file database
//! - **Performance first**: <1ms overhead per request
//! - **Async-friendly**: Non-blocking writes with batching
//! - **Configurable retention**: TTL-based automatic cleanup

pub mod cleanup;
pub mod database;
pub mod layer;
pub mod metrics_snapshot;
pub mod query;
pub mod query_dsl;
pub mod span;
pub mod writer;

// Re-export public types
pub use cleanup::{run_cleanup_now, spawn_cleanup_task, CleanupConfig};
pub use database::{LogEntry, ObservabilityDb};
pub use layer::ObservabilityLayer;
pub use query::{ErrorPattern, LogFilter, SlowRequest, TraceTree};
pub use span::{SpanContext, SpanKind, SpanRecord};
pub use writer::AsyncWriter;

/// Current version of the observability schema
pub const SCHEMA_VERSION: &str = "1.0.0";
