use anyhow::Result;
use arc_swap::ArcSwap;
use axum::{extract::DefaultBodyLimit, middleware, routing::{get, post}, Router};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::{
    auth,
    config::Config,
    handlers,
    load_balancer::{LoadBalancer, ProviderInstance, ProviderInstanceConfigEnum},
    metrics,
    router::{ModelRouter, Provider},
    signals::setup_signal_handlers,
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

    // Build load balancers for each provider type
    let load_balancers = build_load_balancers(&config);

    let app_state = handlers::chat_completions::AppState {
        config: config_swap.clone(),
        router,
        http_client,
        load_balancers,
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
        "Configuration: {} routing rules, {} API keys, {} enabled providers",
        config.routing.rules.len(),
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

/// Build load balancers for each provider type
fn build_load_balancers(config: &Config) -> Arc<HashMap<Provider, Arc<LoadBalancer>>> {
    let mut load_balancers = HashMap::new();

    // OpenAI load balancer
    if !config.providers.openai.is_empty() {
        let instances: Vec<ProviderInstance> = config
            .providers
            .openai
            .iter()
            .filter(|i| i.enabled)
            .map(|cfg| ProviderInstance {
                name: Arc::from(cfg.name.as_str()),
                config: ProviderInstanceConfigEnum::Generic(Arc::new(cfg.clone())),
            })
            .collect();

        if !instances.is_empty() {
            let lb = Arc::new(LoadBalancer::new("openai".to_string(), instances));

            // Spawn background tasks for this load balancer
            tokio::spawn({
                let lb = lb.clone();
                async move {
                    lb.health_recovery_loop().await;
                }
            });

            tokio::spawn({
                let lb = lb.clone();
                async move {
                    lb.session_cleanup_loop().await;
                }
            });

            load_balancers.insert(Provider::OpenAI, lb);
        }
    }

    // Anthropic load balancer
    if !config.providers.anthropic.is_empty() {
        let instances: Vec<ProviderInstance> = config
            .providers
            .anthropic
            .iter()
            .filter(|i| i.enabled)
            .map(|cfg| ProviderInstance {
                name: Arc::from(cfg.name.as_str()),
                config: ProviderInstanceConfigEnum::Anthropic(Arc::new(cfg.clone())),
            })
            .collect();

        if !instances.is_empty() {
            let lb = Arc::new(LoadBalancer::new("anthropic".to_string(), instances));

            // Spawn background tasks
            tokio::spawn({
                let lb = lb.clone();
                async move {
                    lb.health_recovery_loop().await;
                }
            });

            tokio::spawn({
                let lb = lb.clone();
                async move {
                    lb.session_cleanup_loop().await;
                }
            });

            load_balancers.insert(Provider::Anthropic, lb);
        }
    }

    // Gemini load balancer
    if !config.providers.gemini.is_empty() {
        let instances: Vec<ProviderInstance> = config
            .providers
            .gemini
            .iter()
            .filter(|i| i.enabled)
            .map(|cfg| ProviderInstance {
                name: Arc::from(cfg.name.as_str()),
                config: ProviderInstanceConfigEnum::Generic(Arc::new(cfg.clone())),
            })
            .collect();

        if !instances.is_empty() {
            let lb = Arc::new(LoadBalancer::new("gemini".to_string(), instances));

            // Spawn background tasks
            tokio::spawn({
                let lb = lb.clone();
                async move {
                    lb.health_recovery_loop().await;
                }
            });

            tokio::spawn({
                let lb = lb.clone();
                async move {
                    lb.session_cleanup_loop().await;
                }
            });

            load_balancers.insert(Provider::Gemini, lb);
        }
    }

    Arc::new(load_balancers)
}

/// Count the number of enabled providers
fn count_enabled_providers(config: &Config) -> usize {
    let mut count = 0;
    if config.providers.openai.iter().any(|p| p.enabled) {
        count += 1;
    }
    if config.providers.anthropic.iter().any(|p| p.enabled) {
        count += 1;
    }
    if config.providers.gemini.iter().any(|p| p.enabled) {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicInstanceConfig, ApiKeyConfig, DiscoveryConfig, MetricsConfig,
        ProviderInstanceConfig, ProvidersConfig, RoutingConfig, ServerConfig,
    };
    use std::collections::HashMap;

    fn create_test_config() -> Config {
        let mut routing_rules = HashMap::new();
        routing_rules.insert("gpt-".to_string(), "openai".to_string());
        routing_rules.insert("claude-".to_string(), "anthropic".to_string());
        routing_rules.insert("gemini-".to_string(), "gemini".to_string());

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
            routing: RoutingConfig {
                rules: routing_rules,
                default_provider: Some("openai".to_string()),
                discovery: DiscoveryConfig {
                    enabled: true,
                    cache_ttl_seconds: 3600,
                    refresh_on_startup: true,
                    providers_with_listing: vec!["openai".to_string()],
                },
            },
            providers: ProvidersConfig {
                openai: vec![ProviderInstanceConfig {
                    name: "openai-primary".to_string(),
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-primary".to_string(),
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                }],
                gemini: vec![ProviderInstanceConfig {
                    name: "gemini-primary".to_string(),
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                }],
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
        config2.providers.anthropic[0].enabled = true;
        assert_eq!(count_enabled_providers(&config2), 2);

        let mut config3 = config2.clone();
        config3.providers.gemini[0].enabled = true;
        assert_eq!(count_enabled_providers(&config3), 3);
    }

    #[tokio::test]
    async fn test_create_router() {
        let config = create_test_config();
        let config_swap = Arc::new(ArcSwap::from_pointee(config.clone()));
        let router = Arc::new(ModelRouter::new(config_swap.clone()));
        let http_client = reqwest::Client::new();
        let load_balancers = build_load_balancers(&config);

        let app_state = handlers::chat_completions::AppState {
            config: config_swap.clone(),
            router,
            http_client,
            load_balancers,
        };

        let recorder =
            metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let metrics_handle = Arc::new(recorder.handle());

        let _app = create_router(config_swap, app_state, metrics_handle);
        // Router created successfully - no panic
    }
}
