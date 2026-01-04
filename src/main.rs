use axum::{middleware, routing::{get, post}, Router};
use llm_gateway::{auth, config, handlers, metrics, router::ModelRouter};
use std::{net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing/logging
    init_tracing();

    // Initialize metrics
    info!("Initializing Prometheus metrics...");
    let metrics_handle = Arc::new(metrics::init_metrics());

    // Load configuration
    info!("Loading configuration...");
    let cfg = config::load_config()?;
    info!(
        "Configuration loaded successfully. Server will listen on {}:{}",
        cfg.server.host, cfg.server.port
    );

    // Create shared state
    let config = Arc::new(cfg.clone());
    let router = Arc::new(ModelRouter::new(config.clone()));
    let http_client = reqwest::Client::new();

    let app_state = handlers::chat_completions::AppState {
        config: config.clone(),
        router,
        http_client,
    };

    // Build the Axum router
    let app = create_router(config.clone(), app_state, metrics_handle);

    // Create socket address
    let addr = SocketAddr::from((
        cfg.server.host.parse::<std::net::IpAddr>()?,
        cfg.server.port,
    ));

    info!("Starting LLM Gateway on {}", addr);

    // Start the server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn init_tracing() {
    // Configure tracing subscriber
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(true))
        .init();
}

fn create_router(
    config: Arc<config::Config>,
    app_state: handlers::chat_completions::AppState,
    metrics_handle: Arc<metrics_exporter_prometheus::PrometheusHandle>,
) -> Router {
    // Create authenticated routes
    let auth_routes = Router::new()
        .route("/v1/chat/completions", post(handlers::chat_completions::handle_chat_completions))
        .route("/v1/models", get(handlers::models::list_models))
        .layer(middleware::from_fn_with_state(config.clone(), auth::auth_middleware))
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
        .layer(TraceLayer::new_for_http())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use llm_gateway::config::{ApiKeyConfig, MetricsConfig, ModelConfig, ProviderConfig, ProvidersConfig, ServerConfig};
    use std::collections::HashMap;

    fn create_test_config() -> config::Config {
        let mut models = HashMap::new();
        models.insert(
            "gpt-4".to_string(),
            ModelConfig {
                provider: "openai".to_string(),
                api_model: "gpt-4".to_string(),
            },
        );

        config::Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
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
                anthropic: llm_gateway::config::AnthropicConfig {
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

    fn create_test_metrics() -> Arc<metrics_exporter_prometheus::PrometheusHandle> {
        // Use build_recorder instead of install_recorder for tests
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        Arc::new(recorder.handle())
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let cfg = create_test_config();
        let config = Arc::new(cfg.clone());
        let router = Arc::new(ModelRouter::new(config.clone()));
        let http_client = reqwest::Client::new();
        let metrics_handle = create_test_metrics();

        let app_state = handlers::chat_completions::AppState {
            config: config.clone(),
            router,
            http_client,
        };

        let app = create_router(config, app_state, metrics_handle);

        let response = app
            .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_ready_endpoint() {
        let cfg = create_test_config();
        let config = Arc::new(cfg.clone());
        let router = Arc::new(ModelRouter::new(config.clone()));
        let http_client = reqwest::Client::new();
        let metrics_handle = create_test_metrics();

        let app_state = handlers::chat_completions::AppState {
            config: config.clone(),
            router,
            http_client,
        };

        let app = create_router(config, app_state, metrics_handle);

        let response = app
            .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_metrics_endpoint() {
        let cfg = create_test_config();
        let config = Arc::new(cfg.clone());
        let router = Arc::new(ModelRouter::new(config.clone()));
        let http_client = reqwest::Client::new();
        let metrics_handle = create_test_metrics();

        let app_state = handlers::chat_completions::AppState {
            config: config.clone(),
            router,
            http_client,
        };

        let app = create_router(config, app_state, metrics_handle);

        let response = app
            .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}

