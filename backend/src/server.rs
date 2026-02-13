use anyhow::Result;
use arc_swap::ArcSwap;
use axum::{extract::DefaultBodyLimit, middleware, routing::{get, post}, Router};
use futures::FutureExt;
use sqlx::SqlitePool;
use std::{net::SocketAddr, sync::Arc};
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::{
    auth,
    config::Config,
    handlers,
    load_balancer::{LoadBalancer, ProviderInstance},
    observability::RequestLogger,
    pricing::{CostCalculator, PricingService, PricingUpdater},
    quota::QuotaRefresher,
    router::ModelRouter,
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
    let (db_pool, request_logger) = if config.observability.enabled {
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

        // Initialize pricing service
        tracing::info!("Initializing pricing service...");
        let pricing_service = Arc::new(PricingService::new(pool.clone()));

        // Create pricing updater
        let pricing_updater = Arc::new(PricingUpdater::new(
            pricing_service.clone(),
            pool.clone(),
            "https://raw.githubusercontent.com/Wei-Shaw/claude-relay-service/price-mirror/model_prices_and_context_window.json".to_string(),
            "./data/pricing/backups".to_string(),
            std::time::Duration::from_secs(3600), // Update every hour
        ));

        // Perform initial pricing update synchronously to ensure data is available
        tracing::info!("Performing initial pricing data update...");
        match pricing_updater.check_and_update().await {
            Ok(true) => tracing::info!("Pricing data updated from remote source"),
            Ok(false) => {
                tracing::info!("Pricing data unchanged from remote, loading from database");
                // Data unchanged but still need to load cache from database
                if let Err(e) = pricing_service.load_cache().await {
                    tracing::error!("Failed to load pricing cache from database: {}", e);
                } else {
                    tracing::info!("Pricing cache loaded successfully from database");
                }
            },
            Err(e) => {
                tracing::warn!("Initial pricing update from remote failed: {}, loading from database", e);
                // Try to load from database as fallback
                if let Err(e) = pricing_service.load_cache().await {
                    tracing::error!("Failed to load pricing cache from database: {}", e);
                } else {
                    tracing::info!("Pricing cache loaded successfully from database (fallback)");
                }
            }
        }

        // Create cost calculator
        let cost_calculator = Arc::new(CostCalculator::new(pricing_service.clone()));

        // Start pricing updater background task for periodic updates
        tokio::spawn(async move {
            let result = std::panic::AssertUnwindSafe(pricing_updater.start_background_task())
                .catch_unwind()
                .await;
            match result {
                Ok(()) => tracing::warn!("pricing_updater background task exited unexpectedly"),
                Err(e) => tracing::error!(panic = ?e, "pricing_updater background task panicked"),
            }
        });
        tracing::info!("Pricing updater started (checks every hour)");

        // Create request logger with async channel (10k buffer) and cost calculator
        let logger = Arc::new(RequestLogger::new(pool.clone(), 10000, Some(cost_calculator)));
        tracing::info!("Request logger initialized with 10000 event buffer and cost calculation");

        (Some(pool), Some(logger))
    } else {
        tracing::info!("Observability disabled, skipping SQLite request logging");
        (None, None)
    };

    // Wrap config in ArcSwap for atomic reload support
    let config_swap = Arc::new(ArcSwap::from_pointee(config.clone()));

    // Create HTTP client (before load balancers so they can use it for health checks)
    let http_client = Arc::new(reqwest::Client::new());

    // Build provider registry with load balancers for each provider type
    let registry = Arc::new(arc_swap::ArcSwap::from_pointee(
        create_provider_registry(&config, Some(&http_client))
    ));

    // Setup signal handlers (SIGTERM, SIGINT for shutdown; SIGHUP for reload)
    let (shutdown_tx, signal_handle) = setup_signal_handlers(config_swap.clone(), registry.clone());
    let mut shutdown_rx = shutdown_tx.subscribe();

    // Initialize OAuth components if OAuth providers are configured
    let (token_store, oauth_manager) = if !config.oauth_providers.is_empty() {
        tracing::info!(
            providers = config.oauth_providers.len(),
            "Initializing OAuth support"
        );

        // Create token store (in data directory)
        let token_store_path = std::path::PathBuf::from("./data/oauth_tokens.json");

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

    // Start quota refresh background task if observability is enabled
    if config.observability.enabled && config.observability.quota_refresh.enabled {
        if let (Some(pool), Some(token_store)) = (db_pool.clone(), token_store.clone()) {
            let quota_db = crate::quota::db::QuotaDatabase::new(pool);
            let refresher = QuotaRefresher::new(quota_db, &config, token_store);
            let _quota_task = refresher.spawn(Arc::new(config.clone()));
            tracing::info!(
                "配额刷新任务已启动 (间隔: {} 秒)",
                config.observability.quota_refresh.interval_seconds
            );
        } else {
            tracing::warn!("配额刷新任务未启动: 需要数据库和 OAuth 支持");
        }
    }

    // Start data cleanup task if observability is enabled
    if config.observability.enabled {
        if let Some(pool) = db_pool.clone() {
            let _cleanup_task = crate::observability::cleanup::start_cleanup_task(
                pool,
                config.observability.clone(),
            );
            tracing::info!("数据清理任务已启动 (每天 {} 点执行)",
                config.observability.retention.cleanup_hour);
        }
    }

    // Create shared state
    let router = Arc::new(ModelRouter::new(config_swap.clone()));

    let app_state = handlers::chat_completions::AppState {
        config: config_swap.clone(),
        router,
        http_client: (*http_client).clone(),
        registry: registry.clone(),
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
        // Gemini 原生 API 路由 (v1beta)
        // GET /v1beta/models → listModels
        // GET /v1beta/models/{model} → getModel
        // POST /v1beta/models/{model}:generateContent → generateContent
        // POST /v1beta/models/{model}:streamGenerateContent → streamGenerateContent
        // POST /v1beta/models/{model}:countTokens → countTokens
        .route(
            "/v1beta/models",
            get(handlers::gemini_native::handle_list_models),
        )
        .route(
            "/v1beta/models/*path",
            get(handlers::gemini_native::handle_get_model),
        )
        .route(
            "/v1beta/models/*path",
            post(handlers::gemini_native::handle_generate_content_any),
        )
        // Path-routed endpoints (bypass ModelRouter, provider determined by URL)
        .route(
            "/azure/v1/chat/completions",
            post(handlers::azure::handle),
        )
        .route(
            "/bedrock/v1/messages",
            post(handlers::bedrock::handle),
        )
        .route(
            "/v1/responses",
            post(handlers::openai_responses::handle),
        )
        .route(
            "/custom/:provider_id/v1/chat/completions",
            post(handlers::custom::handle),
        )
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

/// Helper to register a provider type into the registry.
///
/// Creates a LoadBalancer from instance configs, spawns background tasks, and registers.
fn register_provider<C: crate::provider_config::ProviderConfig + Clone>(
    registry: &mut crate::registry::ProviderRegistry,
    provider_name: &str,
    provider_impl: Arc<dyn crate::provider_trait::LlmProvider>,
    instances_cfg: &[C],
    http_client: Option<&reqwest::Client>,
) {
    let instances: Vec<ProviderInstance> = instances_cfg
        .iter()
        .filter(|i| i.enabled())
        .map(|cfg| ProviderInstance {
            name: Arc::from(cfg.name()),
            config: Arc::new(cfg.clone()) as Arc<dyn crate::provider_config::ProviderConfig>,
        })
        .collect();

    if instances.is_empty() {
        return;
    }

    let lb = Arc::new(LoadBalancer::with_client(
        provider_name.to_string(),
        instances,
        http_client.cloned(),
    ));

    // Spawn background tasks with panic logging
    tokio::spawn({
        let lb = lb.clone();
        let name = provider_name.to_string();
        async move {
            let result = std::panic::AssertUnwindSafe(lb.health_recovery_loop())
                .catch_unwind()
                .await;
            match result {
                Ok(()) => tracing::warn!(provider = %name, "health_recovery_loop exited unexpectedly"),
                Err(e) => tracing::error!(provider = %name, panic = ?e, "health_recovery_loop panicked"),
            }
        }
    });
    tokio::spawn({
        let lb = lb.clone();
        let name = provider_name.to_string();
        async move {
            let result = std::panic::AssertUnwindSafe(lb.session_cleanup_loop())
                .catch_unwind()
                .await;
            match result {
                Ok(()) => tracing::warn!(provider = %name, "session_cleanup_loop exited unexpectedly"),
                Err(e) => tracing::error!(provider = %name, panic = ?e, "session_cleanup_loop panicked"),
            }
        }
    });

    registry.register(provider_name.to_string(), provider_impl, lb);
}

/// Build ProviderRegistry from config (replaces build_load_balancers)
pub fn create_provider_registry(
    config: &Config,
    http_client: Option<&reqwest::Client>,
) -> crate::registry::ProviderRegistry {
    use crate::provider_trait::{OpenAIProvider, AnthropicProvider, GeminiProvider};
    use crate::providers::azure_openai::AzureOpenAIProvider;
    use crate::providers::bedrock::BedrockProvider;
    use crate::providers::custom::CustomOpenAIProvider;

    let mut registry = crate::registry::ProviderRegistry::new();

    register_provider(
        &mut registry,
        "openai",
        Arc::new(OpenAIProvider),
        &config.providers.openai,
        http_client,
    );

    register_provider(
        &mut registry,
        "anthropic",
        Arc::new(AnthropicProvider),
        &config.providers.anthropic,
        http_client,
    );

    register_provider(
        &mut registry,
        "gemini",
        Arc::new(GeminiProvider),
        &config.providers.gemini,
        http_client,
    );

    register_provider(
        &mut registry,
        "azure_openai",
        Arc::new(AzureOpenAIProvider),
        &config.providers.azure_openai,
        http_client,
    );

    register_provider(
        &mut registry,
        "bedrock",
        Arc::new(BedrockProvider),
        &config.providers.bedrock,
        http_client,
    );

    // Custom providers: group by provider_id, each group gets its own registry entry
    let mut custom_groups: std::collections::HashMap<String, Vec<crate::config::CustomProviderInstanceConfig>> =
        std::collections::HashMap::new();
    for custom in &config.providers.custom {
        custom_groups
            .entry(custom.provider_id.clone())
            .or_default()
            .push(custom.clone());
    }
    for (provider_id, configs) in custom_groups {
        let key = format!("custom:{}", provider_id);
        register_provider(
            &mut registry,
            &key,
            Arc::new(CustomOpenAIProvider),
            &configs,
            http_client,
        );
    }

    registry
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
    if config.providers.azure_openai.iter().any(|p| p.enabled) {
        count += 1;
    }
    if config.providers.bedrock.iter().any(|p| p.enabled) {
        count += 1;
    }
    // Count unique custom provider_ids
    let custom_ids: std::collections::HashSet<&str> = config.providers.custom.iter()
        .filter(|p| p.enabled)
        .map(|p| p.provider_id.as_str())
        .collect();
    count += custom_ids.len();
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
                azure_openai: vec![],
                bedrock: vec![],
                custom: vec![],
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
        let registry = Arc::new(arc_swap::ArcSwap::from_pointee(
            create_provider_registry(&config, Some(&http_client))
        ));

        let app_state = handlers::chat_completions::AppState {
            config: config_swap.clone(),
            router,
            http_client,
            registry: registry.clone(),
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
