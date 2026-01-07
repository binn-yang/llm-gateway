/// Integration tests for prompt caching functionality (Anthropic)
use llm_gateway::{
    config::CacheConfig,
    converters::openai_to_anthropic,
    models::{
        anthropic::MessageContent as AnthropicMessageContent,
        openai::{
            ChatCompletionRequest, ChatMessage, FunctionDefinition, MessageContent, Tool,
            ToolChoice,
        },
    },
};

#[tokio::test]
async fn test_auto_caching_large_system_prompt() {
    // Test that large system prompts get automatically cached
    let large_system_prompt = "This is a system prompt. ".repeat(300); // ~7200 characters = ~1800 tokens

    let mut request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(large_system_prompt.clone()),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
            },
        ],
        max_tokens: Some(100),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (mut anthropic_req, _) = result.unwrap();

    // Apply auto-caching with default config (min_system_tokens = 1024)
    let cache_config = CacheConfig::default();
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &cache_config);

    // Verify that cache_control was added to system prompt
    assert!(anthropic_req.system.is_some());

    match anthropic_req.system.unwrap() {
        AnthropicMessageContent::Blocks(blocks) => {
            assert!(!blocks.is_empty());
            // Last block should have cache_control
            let last_block = blocks.last().unwrap();
            assert!(
                last_block.cache_control.is_some(),
                "Large system prompt should have cache_control"
            );
            assert_eq!(last_block.cache_control.as_ref().unwrap().cache_type, "ephemeral");
        }
        AnthropicMessageContent::Text(_) => {
            panic!("Large system prompt should be converted to blocks format with cache_control");
        }
    }
}

#[tokio::test]
async fn test_auto_caching_small_system_prompt() {
    // Test that small system prompts are NOT cached
    let small_system_prompt = "You are a helpful assistant.";

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(small_system_prompt.to_string()),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
            },
        ],
        max_tokens: Some(100),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (mut anthropic_req, _) = result.unwrap();

    // Apply auto-caching
    let cache_config = CacheConfig::default();
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &cache_config);

    // Verify that small system prompt was NOT converted to blocks with cache_control
    match anthropic_req.system {
        Some(AnthropicMessageContent::Text(_)) => {
            // This is expected - small prompts stay as text
        }
        Some(AnthropicMessageContent::Blocks(blocks)) => {
            // If it's blocks, it should NOT have cache_control
            for block in blocks {
                assert!(
                    block.cache_control.is_none(),
                    "Small system prompt should not have cache_control"
                );
            }
        }
        None => {}
    }
}

#[tokio::test]
async fn test_auto_caching_tools() {
    // Test that tool definitions get automatically cached
    let tools = vec![
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: Some("Get weather information".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"}
                    }
                })),
            },
        },
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "calculate".to_string(),
                description: Some("Perform calculations".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "expression": {"type": "string"}
                    }
                })),
            },
        },
    ];

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("What's the weather?".to_string()),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(100),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: Some(tools),
        tool_choice: Some(ToolChoice::String("auto".to_string())),
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (mut anthropic_req, _) = result.unwrap();

    // Apply auto-caching with tool caching enabled
    let cache_config = CacheConfig {
        auto_cache_system: true,
        min_system_tokens: 1024,
        auto_cache_tools: true,
    };
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &cache_config);

    // Verify that the last tool has cache_control
    assert!(anthropic_req.tools.is_some());
    let tools = anthropic_req.tools.unwrap();
    assert_eq!(tools.len(), 2);

    let last_tool = &tools[tools.len() - 1];
    assert!(
        last_tool.cache_control.is_some(),
        "Last tool should have cache_control when auto_cache_tools is enabled"
    );
    assert_eq!(last_tool.cache_control.as_ref().unwrap().cache_type, "ephemeral");

    // First tool should NOT have cache_control
    let first_tool = &tools[0];
    assert!(
        first_tool.cache_control.is_none(),
        "Only last tool should have cache_control"
    );
}

#[tokio::test]
async fn test_auto_caching_disabled() {
    // Test that auto-caching can be disabled
    let large_system_prompt = "This is a system prompt. ".repeat(300);

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(large_system_prompt.clone()),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
            },
        ],
        max_tokens: Some(100),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (mut anthropic_req, _) = result.unwrap();

    // Apply auto-caching with caching DISABLED
    let cache_config = CacheConfig {
        auto_cache_system: false, // Disabled
        min_system_tokens: 1024,
        auto_cache_tools: false, // Disabled
    };
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &cache_config);

    // Verify that system prompt does NOT have cache_control
    match anthropic_req.system {
        Some(AnthropicMessageContent::Blocks(blocks)) => {
            for block in blocks {
                assert!(
                    block.cache_control.is_none(),
                    "Should not have cache_control when auto_cache_system is disabled"
                );
            }
        }
        Some(AnthropicMessageContent::Text(_)) => {
            // Text format is fine when caching is disabled
        }
        None => {}
    }
}

#[tokio::test]
async fn test_custom_min_tokens_threshold() {
    // Test configurable min_tokens threshold
    let medium_system_prompt = "This is a prompt. ".repeat(100); // ~1900 characters = ~475 tokens

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(medium_system_prompt.clone()),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
            },
        ],
        max_tokens: Some(100),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (mut anthropic_req, _) = result.unwrap();

    // With default threshold (1024), this should NOT be cached
    let default_config = CacheConfig::default();
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &default_config);

    match &anthropic_req.system {
        Some(AnthropicMessageContent::Blocks(blocks)) => {
            for block in blocks {
                assert!(block.cache_control.is_none());
            }
        }
        _ => {}
    }

    // With lower threshold (256), this SHOULD be cached
    let result = openai_to_anthropic::convert_request(&request).await;
    let (mut anthropic_req, _) = result.unwrap();

    let low_threshold_config = CacheConfig {
        auto_cache_system: true,
        min_system_tokens: 256, // Lower threshold
        auto_cache_tools: false,
    };
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &low_threshold_config);

    match anthropic_req.system {
        Some(AnthropicMessageContent::Blocks(blocks)) => {
            let last_block = blocks.last().unwrap();
            assert!(
                last_block.cache_control.is_some(),
                "Should have cache_control with lower threshold"
            );
        }
        _ => panic!("Expected blocks format with cache_control"),
    }
}

#[tokio::test]
async fn test_caching_with_multimodal_system() {
    // Test caching with system prompt that has blocks (multimodal content)
    use llm_gateway::models::openai::ContentBlock;

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Blocks(vec![
                    ContentBlock::Text {
                        text: "You are an image analysis assistant. ".repeat(200), // Large text
                    },
                    ContentBlock::Text {
                        text: "Always provide detailed descriptions.".to_string(),
                    },
                ]),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello".to_string()),
                name: None,
                tool_calls: None,
            },
        ],
        max_tokens: Some(100),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (mut anthropic_req, _) = result.unwrap();

    // Apply auto-caching
    let cache_config = CacheConfig::default();
    openai_to_anthropic::apply_auto_caching(&mut anthropic_req, &cache_config);

    // Verify that the last block in system has cache_control
    match anthropic_req.system {
        Some(AnthropicMessageContent::Blocks(blocks)) => {
            assert!(blocks.len() >= 2);
            let last_block = blocks.last().unwrap();
            assert!(
                last_block.cache_control.is_some(),
                "Last block should have cache_control for large multimodal system"
            );
        }
        _ => panic!("Expected blocks format"),
    }
}
