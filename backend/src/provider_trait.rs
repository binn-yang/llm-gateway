use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use async_trait::async_trait;
use reqwest::Client;

/// Provider native protocol type — determines whether conversion is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderProtocol {
    /// OpenAI chat completions format
    OpenAI,
    /// Anthropic messages format
    Anthropic,
    /// Gemini generateContent format
    Gemini,
}

/// Upstream request context (protocol-agnostic)
pub struct UpstreamRequest {
    /// Request body already in the target protocol format
    pub body: serde_json::Value,
    /// Model name
    pub model: String,
    /// Whether this is a streaming request
    pub stream: bool,
    /// OAuth token (already resolved by handler layer)
    pub oauth_token: Option<String>,
}

/// Unified provider interface for sending requests to upstream LLM APIs.
///
/// Each provider implementation encapsulates:
/// - URL construction
/// - Authentication (Bearer, x-api-key, query param, SigV4, etc.)
/// - Request format specifics
#[async_trait]
pub trait LlmProvider: Send + Sync + 'static {
    /// Provider type name (e.g. "openai", "anthropic", "gemini")
    fn provider_type(&self) -> &str;

    /// Native protocol this provider speaks
    fn native_protocol(&self) -> ProviderProtocol;

    /// Core method: send request to upstream, return raw Response.
    ///
    /// The `body` in `UpstreamRequest` is already in the correct protocol format
    /// for this provider. The implementation is responsible for:
    /// - Building the correct URL from config + model
    /// - Adding authentication headers
    /// - Setting timeouts
    /// - Sending the request
    ///
    /// Error checking (status codes) is NOT done here — the retry layer handles that.
    async fn send_request(
        &self,
        client: &Client,
        config: &dyn ProviderConfig,
        request: UpstreamRequest,
    ) -> Result<reqwest::Response, AppError>;

    /// Health check URL for active health probing.
    /// Default: `{base_url}/models`
    fn health_check_url(&self, config: &dyn ProviderConfig) -> String {
        format!("{}/models", config.base_url().trim_end_matches('/'))
    }
}

// ============================================================
// Built-in Provider Implementations
// ============================================================

/// OpenAI provider (also used for OpenAI-compatible services)
pub struct OpenAIProvider;

#[async_trait]
impl LlmProvider for OpenAIProvider {
    fn provider_type(&self) -> &str {
        "openai"
    }

    fn native_protocol(&self) -> ProviderProtocol {
        ProviderProtocol::OpenAI
    }

    async fn send_request(
        &self,
        client: &Client,
        config: &dyn ProviderConfig,
        request: UpstreamRequest,
    ) -> Result<reqwest::Response, AppError> {
        let url = format!(
            "{}/chat/completions",
            config.base_url().trim_end_matches('/')
        );

        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

        // Bearer authentication
        if let Some(token) = &request.oauth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        } else if let Some(api_key) = config.api_key() {
            req = req.header("Authorization", format!("Bearer {}", api_key));
        } else {
            return Err(AppError::ConfigError(
                "No authentication credentials provided".to_string(),
            ));
        }

        let response = req.json(&request.body).send().await?;
        Ok(response)
    }
}

/// Anthropic provider
pub struct AnthropicProvider;

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn provider_type(&self) -> &str {
        "anthropic"
    }

    fn native_protocol(&self) -> ProviderProtocol {
        ProviderProtocol::Anthropic
    }

    async fn send_request(
        &self,
        client: &Client,
        config: &dyn ProviderConfig,
        request: UpstreamRequest,
    ) -> Result<reqwest::Response, AppError> {
        let url = format!("{}/messages", config.base_url().trim_end_matches('/'));

        // Get api_version via downcast to AnthropicInstanceConfig
        let api_version = config
            .as_any()
            .downcast_ref::<crate::config::AnthropicInstanceConfig>()
            .map(|c| c.api_version.as_str())
            .unwrap_or("2023-06-01");

        let mut req = client
            .post(&url)
            .header("anthropic-version", api_version)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

        // Anthropic uses x-api-key header (not Bearer) for API key auth
        if let Some(token) = &request.oauth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        } else if let Some(api_key) = config.api_key() {
            req = req.header("x-api-key", api_key);
        } else {
            return Err(AppError::ConfigError(
                "No authentication credentials provided".to_string(),
            ));
        }

        let response = req.json(&request.body).send().await?;
        Ok(response)
    }
}

/// Gemini provider
pub struct GeminiProvider;

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn provider_type(&self) -> &str {
        "gemini"
    }

    fn native_protocol(&self) -> ProviderProtocol {
        ProviderProtocol::Gemini
    }

    async fn send_request(
        &self,
        client: &Client,
        config: &dyn ProviderConfig,
        request: UpstreamRequest,
    ) -> Result<reqwest::Response, AppError> {
        let action = if request.stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let url = format!(
            "{}/models/{}:{}",
            config.base_url().trim_end_matches('/'),
            request.model,
            action
        );

        let mut builder = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

        // Gemini uses query param for API key, Bearer for OAuth
        if let Some(token) = &request.oauth_token {
            builder = builder.header("Authorization", format!("Bearer {}", token));
        } else if let Some(api_key) = config.api_key() {
            builder = builder.query(&[("key", api_key)]);
        } else {
            return Err(AppError::ConfigError(
                "No authentication credentials provided".to_string(),
            ));
        }

        if request.stream {
            builder = builder.query(&[("alt", "sse")]);
        }

        let response = builder.json(&request.body).send().await?;
        Ok(response)
    }
}
