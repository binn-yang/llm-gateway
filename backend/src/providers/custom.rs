use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use crate::provider_trait::{LlmProvider, ProviderProtocol, UpstreamRequest};
use async_trait::async_trait;
use reqwest::Client;

/// Custom OpenAI-compatible provider.
///
/// Reuses OpenAI protocol with additional custom headers.
/// Each provider_id gets registered independently in the registry as "custom:{provider_id}".
pub struct CustomOpenAIProvider;

#[async_trait]
impl LlmProvider for CustomOpenAIProvider {
    fn provider_type(&self) -> &str {
        "custom"
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

        let custom_config = config
            .as_any()
            .downcast_ref::<crate::config::CustomProviderInstanceConfig>();

        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

        // Apply custom headers before auth (auth headers take precedence)
        if let Some(cfg) = custom_config {
            for (key, value) in &cfg.custom_headers {
                req = req.header(key.as_str(), value.as_str());
            }
        }

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
