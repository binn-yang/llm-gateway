//! Observability module for LLM Gateway
//!
//! Provides time-series storage and querying capabilities for monitoring data.

pub mod request_logger;

pub use request_logger::{RequestEvent, RequestLogger};
