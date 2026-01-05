use anyhow::Result;
use arc_swap::ArcSwap;
use axum::{extract::DefaultBodyLimit, middleware, routing::{get, post}, Router};
use std::{net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::{
    auth, config::Config, handlers, metrics, router::ModelRouter, signals::setup_signal_handlers,
};

/// Start the LLM Gateway server
///
/// This function:
/// 1. Initializes metrics
/// 2. Sets up signal handlers for graceful shutdown and config reload
/// 3. Creates the Axum application
/// 4. Binds to the configured address
/// 5. Serves requests with graceful shutdown support
pub async fn start_server(config: Config) -> Result<()> {
    // Initialize metrics
    info!("Initializing Prometheus metrics...");
    let metrics_handle = Arc::new(metrics::init_metrics());

    // Wrap config in ArcSwap for atomic reload support
    let config_swap = Arc::new(ArcSwap::from_pointee(config.clone()));

    // Setup signal handlers (SIGTERM, SIGINT for shutdown; SIGHUP for reload)
    let (shutdown_tx, signal_handle) = setup_signal_handlers(config_swap.clone());
    let mut shutdown_rx = shutdown_tx.subscribe();

    // Create shared state
    let router = Arc::new(ModelRouter::new(config_swap.clone()));
    let http_client = reqwest::Client::new();

    let app_state = handlers::chat_completions::AppState {
        config: config_swap.clone(),
        router,
        http_client,
    };

    // Build the Axum router
    let app = create_router(config_swap.clone(), app_state, metrics_handle);

    // Create socket address
    let addr = SocketAddr::from((
        config.server.host.parse::<std::net::IpAddr>()?,
        config.server.port,
    ));

    info!("Starting LLM Gateway on {}", addr);
    info!(
        "Configuration: {} models, {} API keys, {} enabled providers",
        config.models.len(),
        config.api_keys.len(),
        count_enabled_providers(&config)
    );

    // Bind to address
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            // Wait for shutdown signal
            let _ = shutdown_rx.recv().await;
            info!("Shutdown signal received, draining connections...");
        })
        .await?;

    // Wait for signal handler task to complete
    signal_handle.await?;
    info!("Server stopped gracefully");

    Ok(())
}

/// Create the Axum router with all routes and middleware
fn create_router(
    config: Arc<arc_swap::ArcSwap<Config>>,
    app_state: handlers::chat_completions::AppState,
    metrics_handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
) -> Router {
    // Create authenticated routes
    let auth_routes = Router::new()
        .route(
            "/v1/chat/completions",
            post(handlers::chat_completions::handle_chat_completions),
        )
        .route(
            "/v1/messages",
            post(handlers::messages::handle_messages),
        )
        .route("/v1/models", get(handlers::models::list_models))
        .layer(middleware::from_fn_with_state(
            config.clone(),
            auth::auth_middleware,
        ))
        .with_state(app_state);

    // Combine with public routes
    Router::new()
        // Public endpoints (no auth required)
        .route("/health", get(handlers::health::health_check))
        .route("/ready", get(handlers::health::readiness_check))
        .route("/metrics", get(handlers::metrics_handler::metrics))
        .with_state(metrics_handle)
        // Merge authenticated routes
        .merge(auth_routes)
        // Security: Limit request body size to 10MB to prevent memory exhaustion attacks
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
}

/// Count the number of enabled providers
fn count_enabled_providers(config: &Config) -> usize {
    let mut count = 0;
    if config.providers.openai.enabled {
        count += 1;
    }
    if config.providers.anthropic.enabled {
        count += 1;
    }
    if config.providers.gemini.enabled {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, ApiKeyConfig, MetricsConfig, ModelConfig, ProviderConfig,
        ProvidersConfig, ServerConfig,
    };
    use std::collections::HashMap;

    fn create_test_config() -> Config {
        let mut models = HashMap::new();
        models.insert(
            "gpt-4".to_string(),
            ModelConfig {
                provider: "openai".to_string(),
                api_model: "gpt-4".to_string(),
            },
        );

        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                log_format: "json".to_string(),
            },
            api_keys: vec![ApiKeyConfig {
                key: "sk-test".to_string(),
                name: "test".to_string(),
                enabled: true,
            }],
            models,
            providers: ProvidersConfig {
                openai: ProviderConfig {
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                },
                anthropic: AnthropicConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                },
                gemini: ProviderConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                },
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "/metrics".to_string(),
                include_api_key_hash: true,
            },
        }
    }

    #[test]
    fn test_count_enabled_providers() {
        let config = create_test_config();
        assert_eq!(count_enabled_providers(&config), 1); // Only OpenAI enabled

        let mut config2 = config.clone();
        config2.providers.anthropic.enabled = true;
        assert_eq!(count_enabled_providers(&config2), 2);

        let mut config3 = config2.clone();
        config3.providers.gemini.enabled = true;
        assert_eq!(count_enabled_providers(&config3), 3);
    }

    #[tokio::test]
    async fn test_create_router() {
        let config = create_test_config();
        let config_arc = Arc::new(config.clone());
        let router = Arc::new(ModelRouter::new(config_arc.clone()));
        let http_client = reqwest::Client::new();

        let app_state = handlers::chat_completions::AppState {
            config: config_arc.clone(),
            router,
            http_client,
        };

        let recorder =
            metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let metrics_handle = Arc::new(recorder.handle());

        let _app = create_router(config_arc, app_state, metrics_handle);
        // Router created successfully - no panic
    }
}
