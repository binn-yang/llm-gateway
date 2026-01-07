use crate::{
    config::ProviderInstanceConfig,
    error::AppError,
    models::gemini::GenerateContentRequest,
};
use reqwest::Client;
use std::time::Duration;

/// Call Gemini Generate Content API
/// Note: Model name is part of the URL path
pub async fn generate_content(
    client: &Client,
    config: &ProviderInstanceConfig,
    model: &str,
    request: GenerateContentRequest,
    stream: bool,
) -> Result<reqwest::Response, AppError> {
    // Gemini API format: /v1beta/models/{model}:generateContent
    let url = format!("{}/models/{}:generateContent", config.base_url, model);

    let mut builder = client
        .post(&url)
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(config.timeout_seconds))
        .query(&[("key", &config.api_key)]);

    // Add streaming parameter if needed
    if stream {
        builder = builder.query(&[("alt", "sse")]);
    }

    let response = builder.json(&request).send().await?;

    // Check for HTTP errors
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

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gemini::{Content, Part};

    fn create_test_config() -> ProviderInstanceConfig {
        ProviderInstanceConfig {
            name: "test-instance".to_string(),
            enabled: true,
            api_key: "test-key".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
            timeout_seconds: 30,
            priority: 1,
            failure_timeout_seconds: 60,
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
