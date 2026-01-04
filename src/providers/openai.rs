use crate::{
    config::ProviderConfig,
    error::AppError,
    models::openai::ChatCompletionRequest,
};
use reqwest::Client;
use std::time::Duration;

/// Call OpenAI Chat Completions API
pub async fn chat_completions(
    client: &Client,
    config: &ProviderConfig,
    request: ChatCompletionRequest,
) -> Result<reqwest::Response, AppError> {
    let url = format!("{}/chat/completions", config.base_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
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
    use crate::models::openai::ChatMessage;

    fn create_test_config() -> ProviderConfig {
        ProviderConfig {
            enabled: true,
            api_key: "sk-test-key".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            timeout_seconds: 30,
        }
    }

    fn create_test_request() -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
                name: None,
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
        }
    }

    #[tokio::test]
    async fn test_chat_completions_request_format() {
        // This test verifies the request is properly formatted
        // We can't actually call the API without a valid key, but we can test the structure
        let config = create_test_config();
        let request = create_test_request();

        // Verify serialization works
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello!"));
    }
}
