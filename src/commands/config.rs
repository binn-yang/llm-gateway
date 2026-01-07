use anyhow::Result;
use colored::Colorize;
use llm_gateway::config::{self, Config};
use tracing::info;

/// Execute the config show command
///
/// Displays the current configuration with secrets masked
pub fn show() -> Result<()> {
    println!("{}", "Loading configuration...".yellow());
    info!("Loading configuration for display");

    let cfg = config::load_config()?;
    let sanitized = sanitize_secrets(&cfg);

    println!("{}", "Current Configuration:".green().bold());
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
    println!("{}", "Validating configuration...".yellow());
    info!("Validating configuration file");

    let cfg = config::load_config()?;

    println!("{}", "✓ Configuration is valid".green());
    println!();
    println!("{}", "Summary:".bold());
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
            AnthropicConfig, ApiKeyConfig, MetricsConfig, ProviderConfig,
            ProvidersConfig, ServerConfig,
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
            models: HashMap::new(),
            providers: ProvidersConfig {
                openai: ProviderConfig {
                    enabled: true,
                    api_key: "test".to_string(),
                    base_url: "test".to_string(),
                    timeout_seconds: 300,
                },
                anthropic: AnthropicConfig {
                    enabled: true,
                    api_key: "test".to_string(),
                    base_url: "test".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                },
                gemini: ProviderConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "test".to_string(),
                    timeout_seconds: 300,
                },
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "/metrics".to_string(),
                include_api_key_hash: true,
            },
        };

        assert_eq!(count_enabled_providers(&cfg), 2);
    }
}
