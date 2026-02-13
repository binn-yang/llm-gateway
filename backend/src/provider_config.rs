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

/// Macro to reduce boilerplate for ProviderConfig trait implementations.
///
/// # Usage
///
/// ```rust,ignore
/// // Standard implementation (uses self.base_url, self.auth_mode, self.api_key)
/// impl_provider_config!(ProviderInstanceConfig);
///
/// // Custom base_url (for providers like Azure where URL is constructed dynamically)
/// impl_provider_config!(AzureOpenAIInstanceConfig, base_url = "https://azure.openai.azure.com");
///
/// // Full override (for providers like Bedrock that don't use standard auth)
/// impl_provider_config!(BedrockInstanceConfig, base_url = "", auth_mode = &DEFAULT_BEARER, api_key = None);
/// ```
macro_rules! impl_provider_config {
    // Standard implementation with all field defaults
    ($config_type:ty) => {
        impl ProviderConfig for $config_type {
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
    };

    // Custom base_url only
    ($config_type:ty, base_url = $base_url:expr) => {
        impl ProviderConfig for $config_type {
            fn name(&self) -> &str { &self.name }
            fn enabled(&self) -> bool { self.enabled }
            fn auth_mode(&self) -> &AuthMode { &self.auth_mode }
            fn api_key(&self) -> Option<&str> { self.api_key.as_deref() }
            fn oauth_provider(&self) -> Option<&str> { self.oauth_provider.as_deref() }
            fn base_url(&self) -> &str { $base_url }
            fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
            fn priority(&self) -> u32 { self.priority }
            fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
            fn weight(&self) -> u32 { self.weight }
            fn as_any(&self) -> &dyn Any { self }
        }
    };

    // Full custom implementation
    ($config_type:ty, base_url = $base_url:expr, auth_mode = $auth_mode:expr, api_key = $api_key:expr) => {
        impl ProviderConfig for $config_type {
            fn name(&self) -> &str { &self.name }
            fn enabled(&self) -> bool { self.enabled }
            fn auth_mode(&self) -> &AuthMode { $auth_mode }
            fn api_key(&self) -> Option<&str> { $api_key }
            fn oauth_provider(&self) -> Option<&str> { None }
            fn base_url(&self) -> &str { $base_url }
            fn timeout_seconds(&self) -> u64 { self.timeout_seconds }
            fn priority(&self) -> u32 { self.priority }
            fn failure_timeout_seconds(&self) -> u64 { self.failure_timeout_seconds }
            fn weight(&self) -> u32 { self.weight }
            fn as_any(&self) -> &dyn Any { self }
        }
    };
}

// Standard implementations (use all default field accessors)
impl_provider_config!(crate::config::ProviderInstanceConfig);
impl_provider_config!(crate::config::AnthropicInstanceConfig);
impl_provider_config!(crate::config::CustomProviderInstanceConfig);

// Azure has a static base_url placeholder (actual URL is built from resource_name)
impl_provider_config!(crate::config::AzureOpenAIInstanceConfig,
    base_url = "https://azure.openai.azure.com"
);

// Bedrock doesn't use API keys or bearer auth (uses SigV4)
impl_provider_config!(crate::config::BedrockInstanceConfig,
    base_url = "",
    auth_mode = &DEFAULT_BEARER,
    api_key = None
);
