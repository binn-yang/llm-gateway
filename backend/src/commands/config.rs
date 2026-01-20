use anyhow::Result;

use llm_gateway::config::{self, Config};
use tracing::info;

/// Execute the config show command
///
/// Displays the current configuration with secrets masked
pub fn show() -> Result<()> {
    println!("{}", "Loading configuration...");
    info!("Loading configuration for display");

    let cfg = config::load_config()?;
    let sanitized = sanitize_secrets(&cfg);

    println!("{}", "Current Configuration:");
    println!();

    // Serialize to TOML format
    let toml_string = toml::to_string_pretty(&sanitized)?;
    println!("{}", toml_string);

    info!("Configuration displayed successfully");
    Ok(())
}

/// Execute the config validate command
///
/// Validates the configuration file
pub fn validate() -> Result<()> {
    println!("{}", "Validating configuration...");
    info!("Validating configuration file");

    let cfg = config::load_config()?;

    println!("{}", "✓ Configuration is valid");
    println!();
    println!("{}", "Summary:");
    println!("  Routing Rules: {}", cfg.routing.rules.len());
    println!("  API Keys: {}", cfg.api_keys.len());
    println!(
        "  Enabled Providers: {}",
        count_enabled_providers(&cfg)
    );

    info!("Configuration validation successful");
    Ok(())
}

/// Sanitize secrets in configuration for safe display
///
/// This masks API keys to show only first 7 and last 4 characters
fn sanitize_secrets(cfg: &Config) -> Config {
    let mut sanitized = cfg.clone();

    // Mask provider instance API keys
    for instance in &mut sanitized.providers.openai {
        instance.api_key = mask_api_key(&instance.api_key);
    }
    for instance in &mut sanitized.providers.anthropic {
        instance.api_key = mask_api_key(&instance.api_key);
    }
    for instance in &mut sanitized.providers.gemini {
        instance.api_key = mask_api_key(&instance.api_key);
    }

    // Mask gateway API keys
    for key in &mut sanitized.api_keys {
        key.key = mask_api_key(&key.key);
    }

    sanitized
}

/// Mask an API key for safe display
///
/// Shows first 7 and last 4 characters with asterisks in between
/// Example: "sk-1234567890abcdef" -> "sk-1234...cdef"
fn mask_api_key(key: &str) -> String {
    if key.len() <= 11 {
        // Too short to mask meaningfully
        return "***".to_string();
    }

    let prefix = &key[..7];
    let suffix = &key[key.len() - 4..];

    format!("{}...{}", prefix, suffix)
}

/// Count enabled providers
fn count_enabled_providers(cfg: &Config) -> usize {
    let mut count = 0;
    if cfg.providers.openai.iter().any(|p| p.enabled) {
        count += 1;
    }
    if cfg.providers.anthropic.iter().any(|p| p.enabled) {
        count += 1;
    }
    if cfg.providers.gemini.iter().any(|p| p.enabled) {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_api_key() {
        assert_eq!(mask_api_key("sk-1234567890abcdef"), "sk-1234...cdef");
        assert_eq!(
            mask_api_key("sk-ant-api03-very-long-key-1234"),
            "sk-ant-...1234"
        );
        assert_eq!(mask_api_key("short"), "***");
    }

    #[test]
    fn test_count_enabled_providers() {
        use llm_gateway::config::{
            AnthropicInstanceConfig, ObservabilityConfig,
            ProviderInstanceConfig, ProvidersConfig, RoutingConfig, ServerConfig,
        };
        use std::collections::HashMap;

        let cfg = Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                log_format: "json".to_string(),
            },
            api_keys: vec![],
            routing: RoutingConfig {
                rules: HashMap::new(),
                default_provider: None,
                discovery: llm_gateway::config::DiscoveryConfig {
                    enabled: false,
                    cache_ttl_seconds: 3600,
                    refresh_on_startup: false,
                    providers_with_listing: vec![],
                },
            },
            providers: ProvidersConfig {
                openai: vec![ProviderInstanceConfig {
                    name: "test".to_string(),
                    enabled: true,
                    api_key: "test".to_string(),
                    base_url: "test".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "test".to_string(),
                    enabled: true,
                    api_key: "test".to_string(),
                    base_url: "test".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: llm_gateway::config::CacheConfig::default(),
                }],
                gemini: vec![ProviderInstanceConfig {
                    name: "test".to_string(),
                    enabled: true,
                    api_key: "test".to_string(),
                    base_url: "test".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                }],
            },
            observability: ObservabilityConfig::default(),
        };

        assert_eq!(count_enabled_providers(&cfg), 3);
    }
}
