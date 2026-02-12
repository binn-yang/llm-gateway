// OpenAI provider functions have been migrated to the LlmProvider trait.
// See provider_trait.rs::OpenAIProvider for the new implementation.

#[cfg(test)]
mod tests {
    use crate::config::ProviderInstanceConfig;
    use crate::models::openai::{ChatCompletionRequest, ChatMessage};

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
        let _config = create_test_config();
        let request = create_test_request();

        // Verify serialization works
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello!"));
    }
}
