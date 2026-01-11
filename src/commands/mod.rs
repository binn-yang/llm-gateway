//! Command implementations for the CLI
//!
//! This module contains the implementation of all CLI commands:
//! - start: Start the gateway server
//! - stop: Stop a running instance
//! - reload: Reload configuration
//! - test: Test configuration validity
//! - config: Configuration display and validation
//! - stats: Display real-time stats dashboard
//! - logs: Query observability logs
//! - trace: Display request traces
//! - observability: Manage observability database

pub mod config;
pub mod logs;
pub mod observability;
pub mod reload;
pub mod start;
pub mod stats;
pub mod stop;
pub mod test;
pub mod trace;
