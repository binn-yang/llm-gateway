use crate::{config::Config, error::AppError};
use std::sync::Arc;

/// Provider types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
    /// Whether protocol conversion is required (true for non-OpenAI providers when using OpenAI endpoint)
    pub requires_conversion: bool,
}

/// Model router that maps model names to providers
pub struct ModelRouter {
    config: Arc<arc_swap::ArcSwap<Config>>,
}

impl ModelRouter {
    pub fn new(config: Arc<arc_swap::ArcSwap<Config>>) -> Self {
        Self { config }
    }

    /// Route a model name to provider information
    /// Uses prefix matching to determine which provider handles the model
    pub fn route(&self, model: &str) -> Result<RouteInfo, AppError> {
        // Validate model name to prevent injection attacks
        if model.is_empty() || model.len() > 256 {
            return Err(AppError::ModelNotFound(
                "Invalid model name: must be between 1 and 256 characters".to_string(),
            ));
        }

        // Sanitize model name for security (allow alphanumeric, dash, dot, underscore, slash)
        let is_valid = model
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '/');
        if !is_valid {
            return Err(AppError::ModelNotFound(format!(
                "Invalid model name '{}': only alphanumeric characters, hyphens, dots, underscores, and slashes are allowed",
                model
            )));
        }

        // Load current configuration
        let config = self.config.load();

        // Match model to provider using prefix matching
        let provider_name = self.match_model_to_provider(model, &config)?;
        let provider: Provider = provider_name.parse()?;

        // Check if the provider has at least one enabled instance
        match provider {
            Provider::OpenAI => {
                if !config.providers.openai.iter().any(|p| p.enabled) {
                    return Err(AppError::ProviderDisabled(
                        "OpenAI provider has no enabled instances".to_string(),
                    ));
                }
            }
            Provider::Anthropic => {
                if !config.providers.anthropic.iter().any(|p| p.enabled) {
                    return Err(AppError::ProviderDisabled(
                        "Anthropic provider has no enabled instances".to_string(),
                    ));
                }
            }
            Provider::Gemini => {
                if !config.providers.gemini.iter().any(|p| p.enabled) {
                    return Err(AppError::ProviderDisabled(
                        "Gemini provider has no enabled instances".to_string(),
                    ));
                }
            }
        }

        // Determine if conversion is required
        // Only OpenAI models don't need conversion when using OpenAI endpoint
        let requires_conversion = provider != Provider::OpenAI;

        Ok(RouteInfo {
            provider,
            requires_conversion,
        })
    }

    /// Match a model name to a provider using prefix matching
    /// Returns the provider name if a match is found
    fn match_model_to_provider(&self, model: &str, config: &Config) -> Result<String, AppError> {
        // Collect and sort routing rules by prefix length (descending) for longest-match-first
        let mut rules: Vec<_> = config.routing.rules.iter().collect();
        rules.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));

        // Try each routing rule (longest prefix first)
        for (prefix, provider) in rules {
            if model.starts_with(prefix.as_str()) {
                tracing::debug!(
                    model = %model,
                    matched_prefix = %prefix,
                    provider = %provider,
                    "Matched model to provider via prefix"
                );
                return Ok(provider.clone());
            }
        }

        // No prefix matched - use default provider if configured
        if let Some(default) = &config.routing.default_provider {
            tracing::debug!(
                model = %model,
                provider = %default,
                "Using default provider for model (no prefix match)"
            );
            Ok(default.clone())
        } else {
            Err(AppError::ModelNotFound(format!(
                "Model '{}' does not match any routing prefix and no default provider is configured",
                model
            )))
        }
    }

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
        routing_rules.insert("o1-".to_string(), "openai".to_string());
        routing_rules.insert("claude-".to_string(), "anthropic".to_string());
        routing_rules.insert("gemini-".to_string(), "gemini".to_string());
        routing_rules.insert("models/gemini-".to_string(), "gemini".to_string());

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
                    enabled: true,
                    api_key: Some("sk-ant-test".to_string()),
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
                    enabled: true,
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
    fn test_route_openai_model() {
        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(create_test_config())));
        let router = ModelRouter::new(config);

        let route = router.route("gpt-4-turbo-2024-04-09").unwrap();
        assert_eq!(route.provider, Provider::OpenAI);
        assert!(!route.requires_conversion); // OpenAI doesn't need conversion
    }

    #[test]
    fn test_route_anthropic_model() {
        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(create_test_config())));
        let router = ModelRouter::new(config);

        let route = router.route("claude-3-5-sonnet-20241022").unwrap();
        assert_eq!(route.provider, Provider::Anthropic);
        assert!(route.requires_conversion); // Anthropic requires conversion
    }

    #[test]
    fn test_route_gemini_model() {
        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(create_test_config())));
        let router = ModelRouter::new(config);

        let route = router.route("gemini-1.5-pro").unwrap();
        assert_eq!(route.provider, Provider::Gemini);
        assert!(route.requires_conversion); // Gemini requires conversion
    }

    #[test]
    fn test_route_gemini_model_with_models_prefix() {
        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(create_test_config())));
        let router = ModelRouter::new(config);

        // Test longest-prefix-first matching (models/gemini- should match before gemini-)
        let route = router.route("models/gemini-1.5-pro-latest").unwrap();
        assert_eq!(route.provider, Provider::Gemini);
        assert!(route.requires_conversion);
    }

    #[test]
    fn test_route_unknown_model_with_default() {
        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(create_test_config())));
        let router = ModelRouter::new(config);

        // Unknown model should fallback to default provider (openai)
        let route = router.route("unknown-model-xyz").unwrap();
        assert_eq!(route.provider, Provider::OpenAI);
    }

    #[test]
    fn test_route_unknown_model_without_default() {
        let mut config = create_test_config();
        config.routing.default_provider = None;

        let router = ModelRouter::new(Arc::new(arc_swap::ArcSwap::new(Arc::new(config))));
        let result = router.route("unknown-model-xyz");
        assert!(result.is_err());
    }

    #[test]
    fn test_route_disabled_provider() {
        let mut config = create_test_config();
        config.providers.anthropic[0].enabled = false;

        let router = ModelRouter::new(Arc::new(arc_swap::ArcSwap::new(Arc::new(config))));
        let result = router.route("claude-3-5-sonnet-20241022");
        assert!(result.is_err());
    }

    #[test]
    fn test_prefix_matching_priority() {
        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(create_test_config())));
        let router = ModelRouter::new(config);

        // Test that gpt- prefix matches for various GPT models
        assert_eq!(router.route("gpt-4").unwrap().provider, Provider::OpenAI);
        assert_eq!(router.route("gpt-3.5-turbo").unwrap().provider, Provider::OpenAI);

        // Test that o1- prefix matches for O1 models
        assert_eq!(router.route("o1-preview").unwrap().provider, Provider::OpenAI);
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
