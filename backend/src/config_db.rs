/// Configuration database module
///
/// This module handles loading configuration from SQLite database.
/// Configuration is split into:
/// - Database-only: api_keys, routing, providers (managed via Web UI)
/// - File-based: server, observability (with built-in defaults, overridable via config.toml)

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use std::collections::HashMap;

use crate::config::{
    AnthropicInstanceConfig, ApiKeyConfig, CacheConfig, Config, DiscoveryConfig,
    ObservabilityConfig, ProviderInstanceConfig, ProvidersConfig, RoutingConfig, ServerConfig,
};

/// Load complete configuration from database and file sources
///
/// Loading strategy:
/// 1. Load server and observability from config.toml (if exists) or use built-in defaults
/// 2. Load api_keys, routing, providers from database (may be empty on first run)
pub async fn load_config(db_pool: &SqlitePool) -> Result<Config> {
    // 1. Load file-based config (server, observability)
    let (server, observability) = load_file_config();

    // 2. Load database-only config
    let api_keys = load_api_keys_from_db(db_pool).await?;
    let routing = load_routing_from_db(db_pool).await?;
    let providers = load_providers_from_db(db_pool).await?;

    // 3. Warn if database config is empty (expected on first run)
    if api_keys.is_empty() {
        tracing::warn!("No API keys configured in database. Please add via Web UI.");
    }
    if providers.openai.is_empty()
        && providers.anthropic.is_empty()
        && providers.gemini.is_empty()
    {
        tracing::warn!("No provider instances configured in database. Please add via Web UI.");
    }

    Ok(Config {
        server,
        api_keys,
        routing,
        providers,
        observability,
    })
}

/// Load server and observability config from config.toml or use defaults
///
/// Priority: config.toml > built-in defaults
fn load_file_config() -> (ServerConfig, ObservabilityConfig) {
    match std::fs::read_to_string("config.toml") {
        Ok(content) => {
            match toml::from_str::<toml::Value>(&content) {
                Ok(toml_value) => {
                    // Extract server config (fallback to default if not present)
                    let server = toml_value
                        .get("server")
                        .and_then(|v| toml::from_str(&v.to_string()).ok())
                        .unwrap_or_else(|| {
                            tracing::debug!("server section not found in config.toml, using defaults");
                            ServerConfig::default()
                        });

                    // Extract observability config (fallback to default if not present)
                    let observability = toml_value
                        .get("observability")
                        .and_then(|v| toml::from_str(&v.to_string()).ok())
                        .unwrap_or_else(|| {
                            tracing::debug!("observability section not found in config.toml, using defaults");
                            ObservabilityConfig::default()
                        });

                    tracing::info!("Loaded server and observability config from config.toml");
                    (server, observability)
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to parse config.toml, using built-in defaults"
                    );
                    (ServerConfig::default(), ObservabilityConfig::default())
                }
            }
        }
        Err(_) => {
            tracing::info!(
                "No config.toml found, using built-in defaults for server and observability"
            );
            (ServerConfig::default(), ObservabilityConfig::default())
        }
    }
}

/// Load API keys from database
///
/// Returns empty vec if no keys are configured (not an error)
async fn load_api_keys_from_db(db_pool: &SqlitePool) -> Result<Vec<ApiKeyConfig>> {
    #[derive(sqlx::FromRow)]
    struct ApiKeyRow {
        key_hash: String,
        name: String,
        enabled: i64,
    }

    let rows = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT key_hash, name, enabled
        FROM api_keys
        WHERE deleted_at IS NULL
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(db_pool)
    .await
    .context("Failed to load API keys from database")?;

    let api_keys = rows
        .into_iter()
        .map(|row| ApiKeyConfig {
            // Store hash as the "key" - auth middleware will hash incoming tokens to compare
            key: row.key_hash,
            name: row.name,
            enabled: row.enabled != 0,
        })
        .collect();

    Ok(api_keys)
}

/// Load routing configuration from database
async fn load_routing_from_db(db_pool: &SqlitePool) -> Result<RoutingConfig> {
    // 1. Load routing rules
    #[derive(sqlx::FromRow)]
    struct RoutingRuleRow {
        prefix: String,
        provider: String,
    }

    let rule_rows = sqlx::query_as::<_, RoutingRuleRow>(
        r#"
        SELECT prefix, provider
        FROM routing_rules
        WHERE enabled = 1 AND deleted_at IS NULL
        ORDER BY priority ASC
        "#,
    )
    .fetch_all(db_pool)
    .await
    .context("Failed to load routing rules from database")?;

    let rules: HashMap<String, String> = rule_rows
        .into_iter()
        .map(|row| (row.prefix, row.provider))
        .collect();

    // 2. Load global routing config
    #[derive(sqlx::FromRow)]
    struct RoutingConfigRow {
        default_provider: Option<String>,
        discovery_enabled: i64,
        discovery_cache_ttl_seconds: i64,
        discovery_refresh_on_startup: i64,
        discovery_providers_with_listing: String, // JSON array
    }

    let config_row = sqlx::query_as::<_, RoutingConfigRow>(
        r#"
        SELECT
            default_provider,
            discovery_enabled,
            discovery_cache_ttl_seconds,
            discovery_refresh_on_startup,
            discovery_providers_with_listing
        FROM routing_config
        WHERE id = 1
        "#,
    )
    .fetch_one(db_pool)
    .await
    .context("Failed to load routing config from database")?;

    // Parse JSON array for providers_with_listing
    let providers_with_listing: Vec<String> =
        serde_json::from_str(&config_row.discovery_providers_with_listing)
            .unwrap_or_else(|_| vec!["openai".to_string()]);

    Ok(RoutingConfig {
        rules,
        default_provider: config_row.default_provider,
        discovery: DiscoveryConfig {
            enabled: config_row.discovery_enabled != 0,
            cache_ttl_seconds: config_row.discovery_cache_ttl_seconds as u64,
            refresh_on_startup: config_row.discovery_refresh_on_startup != 0,
            providers_with_listing,
        },
    })
}

/// Load provider instances from database
async fn load_providers_from_db(db_pool: &SqlitePool) -> Result<ProvidersConfig> {
    #[derive(sqlx::FromRow)]
    struct ProviderInstanceRow {
        provider: String,
        name: String,
        enabled: i64,
        api_key_encrypted: String,
        base_url: String,
        timeout_seconds: i64,
        priority: i64,
        weight: i64,
        failure_timeout_seconds: i64,
        extra_config: Option<String>, // JSON for Anthropic-specific config
    }

    let rows = sqlx::query_as::<_, ProviderInstanceRow>(
        r#"
        SELECT
            provider,
            name,
            enabled,
            api_key_encrypted,
            base_url,
            timeout_seconds,
            priority,
            weight,
            failure_timeout_seconds,
            extra_config
        FROM provider_instances
        WHERE deleted_at IS NULL
        ORDER BY provider, priority ASC
        "#,
    )
    .fetch_all(db_pool)
    .await
    .context("Failed to load provider instances from database")?;

    let mut openai_instances = Vec::new();
    let mut anthropic_instances = Vec::new();
    let mut gemini_instances = Vec::new();

    for row in rows {
        match row.provider.as_str() {
            "openai" => {
                openai_instances.push(ProviderInstanceConfig {
                    name: row.name,
                    enabled: row.enabled != 0,
                    api_key: row.api_key_encrypted,
                    base_url: row.base_url,
                    timeout_seconds: row.timeout_seconds as u64,
                    priority: row.priority as u32,
                    weight: row.weight as u32,
                    failure_timeout_seconds: row.failure_timeout_seconds as u64,
                });
            }
            "anthropic" => {
                // Parse extra_config JSON for Anthropic-specific fields
                let (api_version, cache) = if let Some(json_str) = row.extra_config {
                    parse_anthropic_extra_config(&json_str)
                } else {
                    ("2023-06-01".to_string(), CacheConfig::default())
                };

                anthropic_instances.push(AnthropicInstanceConfig {
                    name: row.name,
                    enabled: row.enabled != 0,
                    api_key: row.api_key_encrypted,
                    base_url: row.base_url,
                    timeout_seconds: row.timeout_seconds as u64,
                    api_version,
                    priority: row.priority as u32,
                    weight: row.weight as u32,
                    failure_timeout_seconds: row.failure_timeout_seconds as u64,
                    cache,
                });
            }
            "gemini" => {
                gemini_instances.push(ProviderInstanceConfig {
                    name: row.name,
                    enabled: row.enabled != 0,
                    api_key: row.api_key_encrypted,
                    base_url: row.base_url,
                    timeout_seconds: row.timeout_seconds as u64,
                    priority: row.priority as u32,
                    weight: row.weight as u32,
                    failure_timeout_seconds: row.failure_timeout_seconds as u64,
                });
            }
            _ => {
                tracing::warn!(provider = %row.provider, "Unknown provider type, skipping");
            }
        }
    }

    Ok(ProvidersConfig {
        openai: openai_instances,
        anthropic: anthropic_instances,
        gemini: gemini_instances,
    })
}

/// Parse Anthropic extra_config JSON
///
/// Expected format:
/// ```json
/// {
///   "api_version": "2023-06-01",
///   "cache": {
///     "auto_cache_system": true,
///     "min_system_tokens": 1024,
///     "auto_cache_tools": false
///   }
/// }
/// ```
fn parse_anthropic_extra_config(json_str: &str) -> (String, CacheConfig) {
    #[derive(serde::Deserialize)]
    struct ExtraConfig {
        api_version: Option<String>,
        cache: Option<CacheConfig>,
    }

    match serde_json::from_str::<ExtraConfig>(json_str) {
        Ok(config) => (
            config.api_version.unwrap_or_else(|| "2023-06-01".to_string()),
            config.cache.unwrap_or_default(),
        ),
        Err(e) => {
            tracing::warn!(
                error = %e,
                json = %json_str,
                "Failed to parse Anthropic extra_config, using defaults"
            );
            ("2023-06-01".to_string(), CacheConfig::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_anthropic_extra_config() {
        // Full config
        let json = r#"{
            "api_version": "2024-01-01",
            "cache": {
                "auto_cache_system": false,
                "min_system_tokens": 2048,
                "auto_cache_tools": true
            }
        }"#;
        let (version, cache) = parse_anthropic_extra_config(json);
        assert_eq!(version, "2024-01-01");
        assert!(!cache.auto_cache_system);
        assert_eq!(cache.min_system_tokens, 2048);
        assert!(cache.auto_cache_tools);

        // Minimal config
        let json = r#"{}"#;
        let (version, cache) = parse_anthropic_extra_config(json);
        assert_eq!(version, "2023-06-01");
        assert!(cache.auto_cache_system); // default

        // Invalid JSON
        let json = r#"invalid"#;
        let (version, cache) = parse_anthropic_extra_config(json);
        assert_eq!(version, "2023-06-01");
        assert!(cache.auto_cache_system); // default
    }

    #[test]
    fn test_default_configs() {
        let server = ServerConfig::default();
        assert_eq!(server.host, "0.0.0.0");
        assert_eq!(server.port, 8080);

        let routing = RoutingConfig::default();
        assert_eq!(routing.default_provider, Some("openai".to_string()));

        let providers = ProvidersConfig::default();
        assert!(providers.openai.is_empty());
        assert!(providers.anthropic.is_empty());
        assert!(providers.gemini.is_empty());
    }
}
