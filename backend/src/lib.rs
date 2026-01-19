pub mod auth;
pub mod config;
pub mod conversion_warnings;
pub mod converters;
pub mod error;
pub mod handlers;
pub mod image_utils;
pub mod load_balancer;
pub mod logging;
pub mod metrics;
pub mod models;
pub mod observability;
pub mod providers;
pub mod retry;
pub mod router;
pub mod server;
pub mod signals;
pub mod static_files;
pub mod streaming;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing/logging
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}
