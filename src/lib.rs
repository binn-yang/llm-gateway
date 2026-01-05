pub mod auth;
pub mod config;
pub mod converters;
pub mod error;
pub mod handlers;
pub mod metrics;
pub mod models;
pub mod providers;
pub mod router;
pub mod server;
pub mod signals;
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
