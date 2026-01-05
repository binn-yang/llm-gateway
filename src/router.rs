use crate::{config::Config, error::AppError};
use std::sync::Arc;

/// Provider types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    OpenAI,
    Anthropic,
    Gemini,
}

impl Provider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provider::OpenAI => "openai",
            Provider::Anthropic => "anthropic",
            Provider::Gemini => "gemini",
        }
    }
}

impl std::str::FromStr for Provider {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Provider::OpenAI),
            "anthropic" => Ok(Provider::Anthropic),
            "gemini" => Ok(Provider::Gemini),
            _ => Err(AppError::ConfigError(format!("Invalid provider: {}", s))),
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Route information for a model
#[derive(Debug, Clone)]
pub struct RouteInfo {
    /// The provider to use for this model
    pub provider: Provider,
    /// The actual model name to send to the provider's API
    pub api_model: String,
    /// Whether protocol conversion is required (true for non-OpenAI providers when using OpenAI endpoint)
    pub requires_conversion: bool,
}

/// Model router that maps model names to providers
pub struct ModelRouter {
    config: Arc<Config>,
}

impl ModelRouter {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Route a model name to provider information
    /// Used by the OpenAI endpoint (/v1/chat/completions) to determine which provider to use
    pub fn route(&self, model: &str) -> Result<RouteInfo, AppError> {
        // Validate model name to prevent injection attacks
        if model.is_empty() || model.len() > 256 {
            return Err(AppError::ModelNotFound(
                "Invalid model name: must be between 1 and 256 characters".to_string(),
            ));
        }

        // Sanitize model name for security (allow only alphanumeric, dash, dot, underscore)
        let is_valid = model
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_');
        if !is_valid {
            return Err(AppError::ModelNotFound(format!(
                "Invalid model name '{}': only alphanumeric characters, hyphens, dots, and underscores are allowed",
                model
            )));
        }

        // Look up the model in the configuration
        let model_config = self.config.models.get(model).ok_or_else(|| {
            AppError::ModelNotFound(format!(
                "Model '{}' not found in configuration. Available models: {}",
                model,
                self.available_models().join(", ")
            ))
        })?;

        // Parse the provider
        let provider: Provider = model_config.provider.parse()?;

        // Check if the provider is enabled
        match provider {
            Provider::OpenAI => {
                if !self.config.providers.openai.enabled {
                    return Err(AppError::ProviderDisabled(
                        "OpenAI provider is disabled".to_string(),
                    ));
                }
            }
            Provider::Anthropic => {
                if !self.config.providers.anthropic.enabled {
                    return Err(AppError::ProviderDisabled(
                        "Anthropic provider is disabled".to_string(),
                    ));
                }
            }
            Provider::Gemini => {
                if !self.config.providers.gemini.enabled {
                    return Err(AppError::ProviderDisabled(
                        "Gemini provider is disabled".to_string(),
                    ));
                }
            }
        }

        // Determine if conversion is required
        // Only OpenAI models don't need conversion when using OpenAI endpoint
        let requires_conversion = provider != Provider::OpenAI;

        Ok(RouteInfo {
            provider,
            api_model: model_config.api_model.clone(),
            requires_conversion,
        })
    }

    /// Get list of available models
    pub fn available_models(&self) -> Vec<String> {
        self.config.models.keys().cloned().collect()
    }

    /// Get all models for a specific provider
    pub fn models_for_provider(&self, provider: &Provider) -> Vec<String> {
        self.config
            .models
            .iter()
            .filter(|(_, config)| {
                config.provider.parse::<Provider>()
                    .map(|p| &p == provider)
                    .unwrap_or(false)
            })
            .map(|(name, _)| name.clone())
            .collect()
    }
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
        models.insert(
            "claude-3-5-sonnet".to_string(),
            ModelConfig {
                provider: "anthropic".to_string(),
                api_model: "claude-3-5-sonnet-20241022".to_string(),
            },
        );
        models.insert(
            "gemini-1.5-pro".to_string(),
            ModelConfig {
                provider: "gemini".to_string(),
                api_model: "models/gemini-1.5-pro-latest".to_string(),
            },
        );

        Config {
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
                anthropic: AnthropicConfig {
                    enabled: true,
                    api_key: "sk-ant-test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                },
                gemini: ProviderConfig {
                    enabled: true,
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
    fn test_route_openai_model() {
        let config = Arc::new(create_test_config());
        let router = ModelRouter::new(config);

        let route = router.route("gpt-4").unwrap();
        assert_eq!(route.provider, Provider::OpenAI);
        assert_eq!(route.api_model, "gpt-4");
        assert!(!route.requires_conversion); // OpenAI doesn't need conversion
    }

    #[test]
    fn test_route_anthropic_model() {
        let config = Arc::new(create_test_config());
        let router = ModelRouter::new(config);

        let route = router.route("claude-3-5-sonnet").unwrap();
        assert_eq!(route.provider, Provider::Anthropic);
        assert_eq!(route.api_model, "claude-3-5-sonnet-20241022");
        assert!(route.requires_conversion); // Anthropic requires conversion
    }

    #[test]
    fn test_route_gemini_model() {
        let config = Arc::new(create_test_config());
        let router = ModelRouter::new(config);

        let route = router.route("gemini-1.5-pro").unwrap();
        assert_eq!(route.provider, Provider::Gemini);
        assert_eq!(route.api_model, "models/gemini-1.5-pro-latest");
        assert!(route.requires_conversion); // Gemini requires conversion
    }

    #[test]
    fn test_route_unknown_model() {
        let config = Arc::new(create_test_config());
        let router = ModelRouter::new(config);

        let result = router.route("gpt-5");
        assert!(result.is_err());
    }

    #[test]
    fn test_route_disabled_provider() {
        let mut config = create_test_config();
        config.providers.anthropic.enabled = false;

        let router = ModelRouter::new(Arc::new(config));
        let result = router.route("claude-3-5-sonnet");
        assert!(result.is_err());
    }

    #[test]
    fn test_available_models() {
        let config = Arc::new(create_test_config());
        let router = ModelRouter::new(config);

        let models = router.available_models();
        assert_eq!(models.len(), 3);
        assert!(models.contains(&"gpt-4".to_string()));
        assert!(models.contains(&"claude-3-5-sonnet".to_string()));
        assert!(models.contains(&"gemini-1.5-pro".to_string()));
    }

    #[test]
    fn test_models_for_provider() {
        let config = Arc::new(create_test_config());
        let router = ModelRouter::new(config);

        let openai_models = router.models_for_provider(&Provider::OpenAI);
        assert_eq!(openai_models.len(), 1);
        assert!(openai_models.contains(&"gpt-4".to_string()));

        let anthropic_models = router.models_for_provider(&Provider::Anthropic);
        assert_eq!(anthropic_models.len(), 1);
        assert!(anthropic_models.contains(&"claude-3-5-sonnet".to_string()));

        let gemini_models = router.models_for_provider(&Provider::Gemini);
        assert_eq!(gemini_models.len(), 1);
        assert!(gemini_models.contains(&"gemini-1.5-pro".to_string()));
    }

    #[test]
    fn test_provider_from_string() {
        assert_eq!("openai".parse::<Provider>().unwrap(), Provider::OpenAI);
        assert_eq!("anthropic".parse::<Provider>().unwrap(), Provider::Anthropic);
        assert_eq!("gemini".parse::<Provider>().unwrap(), Provider::Gemini);
        assert_eq!("OpenAI".parse::<Provider>().unwrap(), Provider::OpenAI); // case insensitive

        assert!("invalid".parse::<Provider>().is_err());
    }

    #[test]
    fn test_provider_display() {
        assert_eq!(Provider::OpenAI.to_string(), "openai");
        assert_eq!(Provider::Anthropic.to_string(), "anthropic");
        assert_eq!(Provider::Gemini.to_string(), "gemini");
    }
}
