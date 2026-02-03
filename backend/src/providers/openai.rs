use crate::{
    config::ProviderInstanceConfig,
    error::AppError,
    models::openai::ChatCompletionRequest,
};
use reqwest::Client;
use std::time::Duration;

/// Call OpenAI Chat Completions API
pub async fn chat_completions(
    client: &Client,
    config: &ProviderInstanceConfig,
    request: ChatCompletionRequest,
    oauth_token: Option<&str>,
) -> Result<reqwest::Response, AppError> {
    let url = format!("{}/chat/completions", config.base_url);

    let mut req = client
        .post(&url)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(config.timeout_seconds));

    // Add authentication header
    if let Some(token) = oauth_token {
        req = req.header("Authorization", format!("Bearer {}", token));
    } else if let Some(api_key) = &config.api_key {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    } else {
        return Err(AppError::ConfigError(
            "No authentication credentials provided".to_string()
        ));
    }

    let response = req
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
    use crate::models::openai::ChatMessage;

    fn create_test_config() -> ProviderInstanceConfig {
        ProviderInstanceConfig {
            name: "test-instance".to_string(),
            enabled: true,
            api_key: Some("sk-test-key".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            timeout_seconds: 30,
            priority: 1,
            failure_timeout_seconds: 60,
            weight: 100,
            auth_mode: crate::config::AuthMode::Bearer,
            oauth_provider: None,
        }
    }

    fn create_test_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: crate::models::openai::MessageContent::Text("Hello!".to_string()),
                name: None,
                tool_calls: None,
            }],
            max_tokens: Some(10),
            temperature: Some(0.7),
            top_p: None,
            n: None,
            stream: Some(false),
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            seed: None,
            logprobs: None,
            top_logprobs: None,
            logit_bias: None,
            service_tier: None,
        }
    }

    #[tokio::test]
    async fn test_chat_completions_request_format() {
        // This test verifies the request is properly formatted
        // We can't actually call the API without a valid key, but we can test the structure
        let _config = create_test_config();
        let request = create_test_request();

        // Verify serialization works
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello!"));
    }
}
