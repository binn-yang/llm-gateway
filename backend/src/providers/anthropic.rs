// Anthropic provider functions have been migrated to the LlmProvider trait.
// See provider_trait.rs::AnthropicProvider for the new implementation.

#[cfg(test)]
mod tests {
    use crate::config::AnthropicInstanceConfig;
    use crate::models::anthropic::{Message, MessagesRequest};

    fn create_test_config() -> AnthropicInstanceConfig {
        AnthropicInstanceConfig {
            name: "test-instance".to_string(),
            enabled: true,
            auth_mode: crate::config::AuthMode::Bearer,
            api_key: Some("sk-ant-test-key".to_string()),
            oauth_provider: None,
            base_url: "https://api.anthropic.com/v1".to_string(),
            timeout_seconds: 30,
            api_version: "2023-06-01".to_string(),
            priority: 1,
            failure_timeout_seconds: 60,
            weight: 100,
            cache: crate::config::CacheConfig::default(),
        }
    }

    fn create_test_request() -> MessagesRequest {
        MessagesRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            system: Some(crate::models::anthropic::MessageContent::Text("You are helpful".to_string())),
            messages: vec![Message {
                role: "user".to_string(),
                content: crate::models::anthropic::MessageContent::Text("Hello!".to_string()),
            }],
            max_tokens: 1024,
            temperature: Some(0.7),
            top_p: None,
            top_k: None,
            stream: Some(false),
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            thinking: None,
            metadata: None,
        }
    }

    #[tokio::test]
    async fn test_create_message_request_format() {
        let _config = create_test_config();
        let request = create_test_request();

        // Verify serialization works
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-3-5-sonnet"));
        assert!(json.contains("Hello!"));
        assert!(json.contains("max_tokens"));
    }
}
