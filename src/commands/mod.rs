//! Command implementations for the CLI
//!
//! This module contains the implementation of all CLI commands:
//! - start: Start the gateway server
//! - stop: Stop a running instance
//! - reload: Reload configuration
//! - test: Test configuration validity
//! - config: Configuration display and validation
//! - stats: Display real-time stats dashboard

pub mod config;
pub mod reload;
pub mod start;
pub mod stats;
pub mod stop;
pub mod test;
