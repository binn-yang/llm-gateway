// Gemini generate_content has been migrated to the LlmProvider trait.
// See provider_trait.rs::GeminiProvider for the new implementation.
//
// Remaining functions (count_tokens, list_models, get_model) are Gemini-specific
// helper endpoints that don't fit the LlmProvider trait pattern.

use crate::{
    config::ProviderInstanceConfig,
    error::AppError,
    models::gemini::{CountTokensRequest, CountTokensResponse, GetModelResponse, ListModelsResponse},
};
use reqwest::Client;
use std::time::Duration;

/// Call Gemini Count Tokens API
pub async fn count_tokens(
    client: &Client,
    config: &ProviderInstanceConfig,
    model: &str,
    request: &CountTokensRequest,
    oauth_token: Option<&str>,
) -> Result<CountTokensResponse, AppError> {
    let url = format!("{}/models/{}:countTokens", config.base_url, model);

    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(config.timeout_seconds));

    if let Some(token) = oauth_token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    } else if let Some(api_key) = &config.api_key {
        builder = builder.query(&[("key", api_key.as_str())]);
    } else {
        return Err(AppError::ConfigError(
            "No authentication credentials provided".to_string()
        ));
    }

    let response = builder.json(&request).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AppError::UpstreamError {
            status,
            message: error_text,
        });
    }

    Ok(response.json().await?)
}

/// List all available models
pub async fn list_models(
    client: &Client,
    config: &ProviderInstanceConfig,
    oauth_token: Option<&str>,
) -> Result<ListModelsResponse, AppError> {
    let url = format!("{}/models", config.base_url.trim_end_matches('/'));

    let mut builder = client
        .get(&url)
        .timeout(Duration::from_secs(config.timeout_seconds));

    if let Some(token) = oauth_token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    } else if let Some(api_key) = &config.api_key {
        builder = builder.query(&[("key", api_key.as_str())]);
    } else {
        return Err(AppError::ConfigError(
            "No authentication credentials provided".to_string()
        ));
    }

    let response = builder.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AppError::UpstreamError {
            status,
            message: error_text,
        });
    }

    Ok(response.json().await?)
}

/// Get a single model's details
pub async fn get_model(
    client: &Client,
    config: &ProviderInstanceConfig,
    model: &str,
    oauth_token: Option<&str>,
) -> Result<GetModelResponse, AppError> {
    let url = format!("{}/models/{}", config.base_url.trim_end_matches('/'), model);

    let mut builder = client
        .get(&url)
        .timeout(Duration::from_secs(config.timeout_seconds));

    if let Some(token) = oauth_token {
        builder = builder.header("Authorization", format!("Bearer {}", token));
    } else if let Some(api_key) = &config.api_key {
        builder = builder.query(&[("key", api_key.as_str())]);
    } else {
        return Err(AppError::ConfigError(
            "No authentication credentials provided".to_string()
        ));
    }

    let response = builder.send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AppError::UpstreamError {
            status,
            message: error_text,
        });
    }

    Ok(response.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gemini::{Content, GenerateContentRequest, Part};

    fn create_test_config() -> ProviderInstanceConfig {
        ProviderInstanceConfig {
            name: "test-instance".to_string(),
            enabled: true,
            api_key: Some("test-key".to_string()),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            timeout_seconds: 30,
            priority: 1,
            failure_timeout_seconds: 60,
            weight: 100,
            auth_mode: crate::config::AuthMode::Bearer,
            oauth_provider: None,
        }
    }

    fn create_test_request() -> GenerateContentRequest {
        GenerateContentRequest {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part::Text {
                    text: "Hello!".to_string(),
                }],
            }],
            system_instruction: None,
            generation_config: None,
            safety_settings: None,
            tools: None,
            tool_config: None,
        }
    }

    #[tokio::test]
    async fn test_generate_content_request_format() {
        let _config = create_test_config();
        let request = create_test_request();

        // Verify serialization works
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Hello!"));
        assert!(json.contains("contents"));
    }
}
