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
pub mod stats;
pub mod streaming;

use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize tracing/logging
///
/// Note: This function can only be called once. For the server command,
/// observability layer will be added via the global dispatcher if enabled.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}

/// Add observability layer to existing tracing subscriber
///
/// This should be called from server initialization after creating the observability writer.
/// It works by setting a global dispatcher that includes the observability layer.
///
/// # Arguments
///
/// * `writer` - The async writer for SQLite persistence
///
/// # Notes
///
/// This function reinitializes the tracing subscriber with the observability layer added.
/// It should only be called from the server start command.
pub fn add_observability_layer(writer: Arc<observability::AsyncWriter>) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let fmt_layer = fmt::layer().with_target(true);
    let observability_layer = observability::ObservabilityLayer::new(writer);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(observability_layer);

    // Replace the global default subscriber
    // SAFETY: This is safe because we only call this once during server initialization
    match tracing::subscriber::set_global_default(subscriber) {
        Ok(_) => {
            tracing::info!("Observability layer added to tracing subscriber");
        }
        Err(e) => {
            eprintln!("Warning: Failed to add observability layer: {}", e);
            eprintln!("Logs will only be written to console, not to SQLite");
        }
    }
}
