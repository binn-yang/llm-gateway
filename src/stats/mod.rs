//! Stats module for real-time metrics monitoring
//!
//! This module provides functionality for fetching, parsing, and displaying
//! Prometheus metrics in a terminal-based dashboard.

pub mod fetcher;
pub mod histogram;
pub mod parser;
pub mod ui;

// Re-export commonly used types
pub use parser::GroupBy;
pub use ui::StatsApp;
