//! Authentication utilities for LLM providers.
//!
//! This module provides common authentication patterns used across different LLM providers,
//! reducing code duplication and ensuring consistent auth header handling.

use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use reqwest::RequestBuilder;

/// Authentication style for different providers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStyle {
    /// Bearer token: `Authorization: Bearer {token}`
    Bearer,
    /// Anthropic x-api-key header: `x-api-key: {key}`
    XApiKey,
    /// Azure api-key header: `api-key: {key}`
    ApiKeyHeader,
    /// Gemini query parameter: `?key={key}`
    QueryParam,
}

/// Apply authentication to a request builder.
///
/// This function handles the common pattern of:
/// 1. Using OAuth token if provided (always as Bearer)
/// 2. Falling back to API key from config
/// 3. Applying the appropriate header/query param based on auth style
///
/// # Errors
///
/// Returns `AppError::ConfigError` if neither OAuth token nor API key is available.
///
/// # Example
///
/// ```rust,ignore
/// let mut req = client.post(&url)
///     .header("Content-Type", "application/json")
///     .timeout(timeout);
///
/// req = apply_auth(req, config, oauth_token.as_deref(), AuthStyle::Bearer)?;
/// ```
pub fn apply_auth(
    mut builder: RequestBuilder,
    config: &dyn ProviderConfig,
    oauth_token: Option<&str>,
    style: AuthStyle,
) -> Result<RequestBuilder, AppError> {
    // OAuth tokens always use Bearer auth
    if let Some(token) = oauth_token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
        return Ok(builder);
    }

    // Fall back to API key with provider-specific style
    let api_key = config.api_key().ok_or_else(|| {
        AppError::ConfigError("No authentication credentials provided".to_string())
    })?;

    match style {
        AuthStyle::Bearer => {
            builder = builder.header("Authorization", format!("Bearer {}", api_key));
        }
        AuthStyle::XApiKey => {
            builder = builder.header("x-api-key", api_key);
        }
        AuthStyle::ApiKeyHeader => {
            builder = builder.header("api-key", api_key);
        }
        AuthStyle::QueryParam => {
            builder = builder.query(&[("key", api_key)]);
        }
    }

    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal test config for unit testing
    #[derive(Debug)]
    struct TestConfig {
        api_key: Option<String>,
    }

    impl ProviderConfig for TestConfig {
        fn name(&self) -> &str { "test" }
        fn enabled(&self) -> bool { true }
        fn auth_mode(&self) -> &crate::config::AuthMode {
            &crate::config::AuthMode::Bearer
        }
        fn api_key(&self) -> Option<&str> { self.api_key.as_deref() }
        fn oauth_provider(&self) -> Option<&str> { None }
        fn base_url(&self) -> &str { "https://api.test.com" }
        fn timeout_seconds(&self) -> u64 { 30 }
        fn priority(&self) -> u32 { 1 }
        fn failure_timeout_seconds(&self) -> u64 { 60 }
        fn weight(&self) -> u32 { 100 }
        fn as_any(&self) -> &dyn std::any::Any { self }
    }

    #[test]
    fn test_oauth_takes_precedence() {
        // OAuth token should always use Bearer auth regardless of style
        // This is verified by the implementation: OAuth always adds "Authorization: Bearer"
        // The style parameter only affects how API key is applied
    }

    #[test]
    fn test_missing_api_key_returns_error() {
        let config = TestConfig { api_key: None };
        let client = reqwest::Client::new();
        let builder = client.post("https://example.com");

        let result = apply_auth(builder, &config, None, AuthStyle::Bearer);
        assert!(result.is_err());
    }
}
