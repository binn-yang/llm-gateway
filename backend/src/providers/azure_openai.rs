use crate::auth_utils::{apply_auth, AuthStyle};
use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use crate::provider_trait::{LlmProvider, ProviderProtocol, UpstreamRequest};
use async_trait::async_trait;
use reqwest::Client;

/// Azure OpenAI provider.
///
/// URL pattern: `https://{resource_name}.openai.azure.com/openai/deployments/{deployment}/chat/completions?api-version={api_version}`
/// Auth: `api-key` header (not Bearer)
/// Protocol: OpenAI (no conversion needed)
pub struct AzureOpenAIProvider;

#[async_trait]
impl LlmProvider for AzureOpenAIProvider {
    fn provider_type(&self) -> &str {
        "azure_openai"
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
        let azure_config = config
            .as_any()
            .downcast_ref::<crate::config::AzureOpenAIInstanceConfig>()
            .ok_or_else(|| {
                AppError::ConfigError("Expected AzureOpenAIInstanceConfig".to_string())
            })?;

        // Determine deployment name: model_deployments mapping > deployment_name > model name
        let deployment = azure_config
            .model_deployments
            .get(&request.model)
            .cloned()
            .or_else(|| azure_config.deployment_name.clone())
            .unwrap_or_else(|| request.model.clone());

        let url = format!(
            "https://{}.openai.azure.com/openai/deployments/{}/chat/completions?api-version={}",
            azure_config.resource_name, deployment, azure_config.api_version
        );

        let req = client
            .post(&url)
            .header("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(config.timeout_seconds()));

        // Azure uses api-key header for API key, Bearer for OAuth
        let auth_style = if request.oauth_token.is_some() {
            AuthStyle::Bearer
        } else {
            AuthStyle::ApiKeyHeader
        };
        let req = apply_auth(req, config, request.oauth_token.as_deref(), auth_style)?;

        let response = req.json(&request.body).send().await?;
        Ok(response)
    }

    fn health_check_url(&self, config: &dyn ProviderConfig) -> String {
        let azure_config = config
            .as_any()
            .downcast_ref::<crate::config::AzureOpenAIInstanceConfig>();
        if let Some(cfg) = azure_config {
            format!(
                "https://{}.openai.azure.com/openai/models?api-version={}",
                cfg.resource_name, cfg.api_version
            )
        } else {
            format!("{}/models", config.base_url())
        }
    }
}
