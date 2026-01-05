use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub api_keys: Vec<ApiKeyConfig>,
    pub models: HashMap<String, ModelConfig>,
    pub providers: ProvidersConfig,
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub log_format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiKeyConfig {
    pub key: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelConfig {
    pub provider: String,
    pub api_model: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProvidersConfig {
    pub openai: ProviderConfig,
    pub anthropic: AnthropicConfig,
    pub gemini: ProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicConfig {
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,
    pub api_version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub include_api_key_hash: bool,
}

pub fn load_config() -> anyhow::Result<Config> {
    let config = config::Config::builder()
        .add_source(config::File::with_name("config"))
        .add_source(config::Environment::with_prefix("LLM_GATEWAY").separator("__"))
        .build()?;

    let cfg: Config = config.try_deserialize()?;
    validate_config(&cfg)?;

    Ok(cfg)
}

fn validate_config(cfg: &Config) -> anyhow::Result<()> {
    // Validate at least one provider is enabled
    if !cfg.providers.openai.enabled
        && !cfg.providers.anthropic.enabled
        && !cfg.providers.gemini.enabled
    {
        anyhow::bail!("At least one provider must be enabled");
    }

    // Validate at least one API key is configured
    if cfg.api_keys.is_empty() {
        anyhow::bail!("At least one API key must be configured");
    }

    // Validate all API keys have names
    for key in &cfg.api_keys {
        if key.name.is_empty() {
            anyhow::bail!("API key name cannot be empty");
        }
    }

    // Validate model mappings have valid providers
    for (model_name, model_config) in &cfg.models {
        match model_config.provider.as_str() {
            "openai" => {
                if !cfg.providers.openai.enabled {
                    anyhow::bail!("Model '{}' uses OpenAI provider, but OpenAI is disabled", model_name);
                }
            }
            "anthropic" => {
                if !cfg.providers.anthropic.enabled {
                    anyhow::bail!("Model '{}' uses Anthropic provider, but Anthropic is disabled", model_name);
                }
            }
            "gemini" => {
                if !cfg.providers.gemini.enabled {
                    anyhow::bail!("Model '{}' uses Gemini provider, but Gemini is disabled", model_name);
                }
            }
            _ => anyhow::bail!("Model '{}' has invalid provider: {}", model_name, model_config.provider),
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_config_requires_enabled_provider() {
        let mut cfg = create_test_config();
        cfg.providers.openai.enabled = false;
        cfg.providers.anthropic.enabled = false;
        cfg.providers.gemini.enabled = false;

        let result = validate_config(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("At least one provider must be enabled"));
    }

    #[test]
    fn test_validate_config_requires_api_keys() {
        let mut cfg = create_test_config();
        cfg.api_keys.clear();

        let result = validate_config(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("At least one API key must be configured"));
    }

    fn create_test_config() -> Config {
        Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                log_format: "json".to_string(),
            },
            api_keys: vec![ApiKeyConfig {
                key: "test-key".to_string(),
                name: "test".to_string(),
                enabled: true,
            }],
            models: HashMap::new(),
            providers: ProvidersConfig {
                openai: ProviderConfig {
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                },
                anthropic: AnthropicConfig {
                    enabled: false,
                    api_key: "sk-ant-test".to_string(),
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
}
