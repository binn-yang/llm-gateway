use crate::config::AuthMode;
use std::any::Any;
use std::fmt::Debug;

/// Trait abstracting provider instance configuration.
///
/// Replaces `ProviderInstanceConfigEnum` with a trait object approach,
/// eliminating the need for match arms when adding new providers.
pub trait ProviderConfig: Send + Sync + Debug + 'static {
    fn name(&self) -> &str;
    fn enabled(&self) -> bool;
    fn auth_mode(&self) -> &AuthMode;
    fn api_key(&self) -> Option<&str>;
    fn oauth_provider(&self) -> Option<&str>;
    fn base_url(&self) -> &str;
    fn timeout_seconds(&self) -> u64;
    fn priority(&self) -> u32;
    fn failure_timeout_seconds(&self) -> u64;
    fn weight(&self) -> u32;

    /// Downcast to concrete config type for provider-specific fields.
    fn as_any(&self) -> &dyn Any;
}

static DEFAULT_BEARER: AuthMode = AuthMode::Bearer;

// Implement ProviderConfig for ProviderInstanceConfig (OpenAI, Gemini)
impl ProviderConfig for crate::config::ProviderInstanceConfig {
    fn name(&self) -> &str { &self.name }
    fn enabled(&self) -> bool { self.enabled }
    fn auth_mode(&self) -> &AuthMode { &self.auth_mode }
    fn api_key(&self) -> Option<&str> { self.api_key.as_deref() }
    fn oauth_provider(&self) -> Option<&str> { self.oauth_provider.as_deref() }
    fn base_url(&self) -> &str { &self.base_url }
    fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
    fn priority(&self) -> u32 { self.priority }
    fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
    fn weight(&self) -> u32 { self.weight }
    fn as_any(&self) -> &dyn Any { self }
}

// Implement ProviderConfig for AnthropicInstanceConfig
impl ProviderConfig for crate::config::AnthropicInstanceConfig {
    fn name(&self) -> &str { &self.name }
    fn enabled(&self) -> bool { self.enabled }
    fn auth_mode(&self) -> &AuthMode { &self.auth_mode }
    fn api_key(&self) -> Option<&str> { self.api_key.as_deref() }
    fn oauth_provider(&self) -> Option<&str> { self.oauth_provider.as_deref() }
    fn base_url(&self) -> &str { &self.base_url }
    fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
    fn priority(&self) -> u32 { self.priority }
    fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
    fn weight(&self) -> u32 { self.weight }
    fn as_any(&self) -> &dyn Any { self }
}

// Implement ProviderConfig for AzureOpenAIInstanceConfig
impl ProviderConfig for crate::config::AzureOpenAIInstanceConfig {
    fn name(&self) -> &str { &self.name }
    fn enabled(&self) -> bool { self.enabled }
    fn auth_mode(&self) -> &AuthMode { &self.auth_mode }
    fn api_key(&self) -> Option<&str> { self.api_key.as_deref() }
    fn oauth_provider(&self) -> Option<&str> { self.oauth_provider.as_deref() }
    fn base_url(&self) -> &str {
        // Azure base URL is constructed dynamically from resource_name in the provider
        "https://azure.openai.azure.com"
    }
    fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
    fn priority(&self) -> u32 { self.priority }
    fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
    fn weight(&self) -> u32 { self.weight }
    fn as_any(&self) -> &dyn Any { self }
}

// Implement ProviderConfig for BedrockInstanceConfig
impl ProviderConfig for crate::config::BedrockInstanceConfig {
    fn name(&self) -> &str { &self.name }
    fn enabled(&self) -> bool { self.enabled }
    fn auth_mode(&self) -> &AuthMode {
        // Bedrock uses SigV4, not bearer/oauth
        &DEFAULT_BEARER
    }
    fn api_key(&self) -> Option<&str> {
        // Bedrock doesn't use API keys
        None
    }
    fn oauth_provider(&self) -> Option<&str> { None }
    fn base_url(&self) -> &str {
        // Base URL is constructed dynamically from region in the provider
        ""
    }
    fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
    fn priority(&self) -> u32 { self.priority }
    fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
    fn weight(&self) -> u32 { self.weight }
    fn as_any(&self) -> &dyn Any { self }
}

// Implement ProviderConfig for CustomProviderInstanceConfig
impl ProviderConfig for crate::config::CustomProviderInstanceConfig {
    fn name(&self) -> &str { &self.name }
    fn enabled(&self) -> bool { self.enabled }
    fn auth_mode(&self) -> &AuthMode { &self.auth_mode }
    fn api_key(&self) -> Option<&str> { self.api_key.as_deref() }
    fn oauth_provider(&self) -> Option<&str> { self.oauth_provider.as_deref() }
    fn base_url(&self) -> &str { &self.base_url }
    fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
    fn priority(&self) -> u32 { self.priority }
    fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
    fn weight(&self) -> u32 { self.weight }
    fn as_any(&self) -> &dyn Any { self }
}
