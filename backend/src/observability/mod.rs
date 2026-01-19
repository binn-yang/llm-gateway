//! Observability module for LLM Gateway
//!
//! Provides time-series storage and querying capabilities for monitoring data.

pub mod metrics_snapshot;

pub use metrics_snapshot::MetricsSnapshotWriter;
