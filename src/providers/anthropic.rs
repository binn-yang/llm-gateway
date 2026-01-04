use crate::{
    config::AnthropicConfig,
    error::AppError,
    models::anthropic::MessagesRequest,
};
use reqwest::Client;
use std::time::Duration;

/// Call Anthropic Messages API
pub async fn create_message(
    client: &Client,
    config: &AnthropicConfig,
    request: MessagesRequest,
) -> Result<reqwest::Response, AppError> {
    let url = format!("{}/messages", config.base_url);

    let response = client
        .post(&url)
        .header("x-api-key", &config.api_key)
        .header("anthropic-version", &config.api_version)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(config.timeout_seconds))
        .json(&request)
        .send()
        .await?;

    // Check for HTTP errors
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AppError::UpstreamError {
            status,
            message: error_text,
        });
    }

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::anthropic::Message;

    fn create_test_config() -> AnthropicConfig {
        AnthropicConfig {
            enabled: true,
            api_key: "sk-ant-test-key".to_string(),
            base_url: "https://api.anthropic.com/v1".to_string(),
            timeout_seconds: 30,
            api_version: "2023-06-01".to_string(),
        }
    }

    fn create_test_request() -> MessagesRequest {
        MessagesRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            system: Some("You are helpful.".to_string()),
            messages: vec![Message {
                role: "user".to_string(),
                content: "Hello!".to_string(),
            }],
            max_tokens: 1024,
            temperature: Some(0.7),
            top_p: None,
            top_k: None,
            stream: Some(false),
            stop_sequences: None,
        }
    }

    #[tokio::test]
    async fn test_create_message_request_format() {
        let config = create_test_config();
        let request = create_test_request();

        // Verify serialization works
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-3-5-sonnet"));
        assert!(json.contains("Hello!"));
        assert!(json.contains("max_tokens"));
    }
}
