use anyhow::Result;
use arc_swap::ArcSwap;
use axum::{extract::DefaultBodyLimit, middleware, routing::{get, post}, Router};
use sqlx::SqlitePool;
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::{
    auth,
    config::Config,
    handlers,
    load_balancer::{LoadBalancer, ProviderInstance, ProviderInstanceConfigEnum},
    observability::RequestLogger,
    router::{ModelRouter, Provider},
    signals::setup_signal_handlers,
};

/// Start the LLM Gateway server
///
/// This function:
/// 1. Sets up signal handlers for graceful shutdown and config reload
/// 2. Creates the Axum application
/// 3. Binds to the configured address
/// 4. Serves requests with graceful shutdown support
pub async fn start_server(config: Config) -> Result<()> {
    // Initialize tracing/logging
    crate::init_tracing();
    tracing::info!("LLM Gateway starting...");

    // Clean up old log files (7 days retention)
    cleanup_old_logs(7);

    // Initialize observability (SQLite-based request logger)
    let (_db_pool, request_logger) = if config.observability.enabled {
        tracing::info!(
            database = %config.observability.database_path,
            "Initializing observability database"
        );

        // Create SQLite connection pool
        // Ensure parent directory exists
        if let Some(parent) = std::path::Path::new(&config.observability.database_path).parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Use SqliteConnectOptions for better control
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&config.observability.database_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);
        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

        // Run migrations
        tracing::info!("Running database migrations...");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to run migrations: {}", e))?;

        // Create request logger with async channel (10k buffer)
        let logger = Arc::new(RequestLogger::new(pool.clone(), 10000));
        tracing::info!("Request logger initialized with 10000 event buffer");

        (Some(pool), Some(logger))
    } else {
        tracing::info!("Observability disabled, skipping SQLite request logging");
        (None, None)
    };

    // Wrap config in ArcSwap for atomic reload support
    let config_swap = Arc::new(ArcSwap::from_pointee(config.clone()));

    // Create HTTP client (before load balancers so they can use it for health checks)
    let http_client = Arc::new(reqwest::Client::new());

    // Build load balancers for each provider type with HTTP client for active health checks
    let load_balancers = Arc::new(arc_swap::ArcSwap::from_pointee(
        (*build_load_balancers(&config, Some(&http_client))).clone()
    ));

    // Setup signal handlers (SIGTERM, SIGINT for shutdown; SIGHUP for reload)
    let (shutdown_tx, signal_handle) = setup_signal_handlers(config_swap.clone(), load_balancers.clone());
    let mut shutdown_rx = shutdown_tx.subscribe();

    // Initialize OAuth components if OAuth providers are configured
    let (token_store, oauth_manager) = if !config.oauth_providers.is_empty() {
        tracing::info!(
            providers = config.oauth_providers.len(),
            "Initializing OAuth support"
        );

        // Create token store directory if it doesn't exist
        let token_store_path = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?
            .join(".llm-gateway")
            .join("oauth_tokens.json");

        if let Some(parent) = token_store_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Initialize token store
        let store = Arc::new(
            crate::oauth::TokenStore::new(token_store_path)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to initialize token store: {}", e))?
        );

        // Initialize OAuth manager
        let manager = Arc::new(crate::oauth::OAuthManager::new(
            config.oauth_providers.clone(),
            store.clone(),
        ));

        tracing::info!("OAuth support initialized successfully");

        // Start automatic token refresh task
        crate::oauth::start_auto_refresh_task(store.clone(), manager.clone());
        tracing::info!("OAuth token auto-refresh task started (checks every 5 minutes)");

        (Some(store), Some(manager))
    } else {
        tracing::debug!("No OAuth providers configured, skipping OAuth initialization");
        (None, None)
    };

    // Create shared state
    let router = Arc::new(ModelRouter::new(config_swap.clone()));

    let app_state = handlers::chat_completions::AppState {
        config: config_swap.clone(),
        router,
        http_client: (*http_client).clone(),
        load_balancers: load_balancers.clone(),
        request_logger: request_logger.clone(),
        token_store: token_store.clone(),
        oauth_manager: oauth_manager.clone(),
    };

    // Build the Axum router
    let app = create_router(config_swap.clone(), app_state);

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

    // Build app with all routes
    let app = Router::new()
        // Public endpoints (no auth required)
        .route("/health", get(handlers::health::health_check))
        .route("/ready", get(handlers::health::readiness_check))
        // Merge authenticated routes
        .merge(auth_routes);

    app
        // Security: Limit request body size to 10MB to prevent memory exhaustion attacks
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
}

/// Build load balancers for each provider type
pub fn build_load_balancers(config: &Config, http_client: Option<&reqwest::Client>) -> Arc<HashMap<Provider, Arc<LoadBalancer>>> {
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
            let lb = Arc::new(LoadBalancer::with_client(
                "openai".to_string(),
                instances,
                http_client.cloned(),
            ));

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
            let lb = Arc::new(LoadBalancer::with_client(
                "anthropic".to_string(),
                instances,
                http_client.cloned(),
            ));

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
            let lb = Arc::new(LoadBalancer::with_client(
                "gemini".to_string(),
                instances,
                http_client.cloned(),
            ));

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
        AnthropicInstanceConfig, ApiKeyConfig, DiscoveryConfig,
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
                    api_key: Some("sk-test".to_string()),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-primary".to_string(),
                    enabled: false,
                    api_key: Some("test".to_string()),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: crate::config::CacheConfig::default(),
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                gemini: vec![ProviderInstanceConfig {
                    name: "gemini-primary".to_string(),
                    enabled: false,
                    api_key: Some("test".to_string()),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
            },
            observability: crate::config::ObservabilityConfig::default(),
            oauth_providers: vec![],
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
        let load_balancers = Arc::new(arc_swap::ArcSwap::from_pointee(
            (*build_load_balancers(&config, Some(&http_client))).clone()
        ));

        let app_state = handlers::chat_completions::AppState {
            config: config_swap.clone(),
            router,
            http_client,
            load_balancers: load_balancers.clone(),
            request_logger: None,
            token_store: None,
            oauth_manager: None,
        };

        let _app = create_router(config_swap, app_state);
        // Router created successfully - no panic
    }
}

// ============================================================================
// Log File Management
// ============================================================================

/// Clean up old log files
///
/// Deletes log files older than the specified retention period.
/// This function is called on server startup to ensure old logs don't accumulate.
///
/// # Arguments
/// * `retention_days` - Number of days to keep logs (files older than this are deleted)
fn cleanup_old_logs(retention_days: i64) {
    use chrono::Duration;
    use std::path::PathBuf;

    let logs_dir = PathBuf::from("logs");
    if !logs_dir.exists() {
        return;
    }

    let cutoff_date = chrono::Utc::now() - Duration::days(retention_days);

    if let Ok(entries) = std::fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // 解析文件名中的日期：requests.2024-01-20
                if filename.starts_with("requests.") {
                    if let Some(date_str) = filename.strip_prefix("requests.") {
                        if let Ok(file_date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                            if file_date.and_hms_opt(0, 0, 0).unwrap().and_utc() < cutoff_date {
                                if let Err(e) = std::fs::remove_file(&path) {
                                    tracing::warn!(
                                        file = ?path,
                                        error = %e,
                                        "Failed to delete old log file"
                                    );
                                } else {
                                    tracing::info!(file = ?path, "Deleted old log file");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
