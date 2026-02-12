use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use crate::provider_trait::{LlmProvider, ProviderProtocol, UpstreamRequest};
use async_trait::async_trait;
use reqwest::Client;

/// OpenAI Responses API provider.
///
/// URL: `{base_url}/responses`
/// Auth: Bearer token
/// Protocol: OpenAI (but request/response schema differs from chat completions)
pub struct OpenAIResponsesProvider;

#[async_trait]
impl LlmProvider for OpenAIResponsesProvider {
    fn provider_type(&self) -> &str {
        "openai_responses"
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
            "{}/responses",
            config.base_url().trim_end_matches('/')
        );

        let mut req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

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
