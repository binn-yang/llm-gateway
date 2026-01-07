/// Integration tests for tool/function calling across providers
use llm_gateway::{
    converters::{anthropic_response, openai_to_anthropic, openai_to_gemini},
    models::{
        anthropic::{ContentBlock as AnthropicContentBlock, MessagesResponse, TokenUsage},
        openai::{
            ChatCompletionRequest, ChatMessage, FunctionDefinition, MessageContent, Tool,
            ToolChoice,
        },
    },
};

#[tokio::test]
async fn test_tool_definition_conversion_anthropic() {
    // Test converting OpenAI tool definitions to Anthropic format
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("What's the weather in San Francisco?".to_string()),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(1024),
        temperature: None,
        top_p: None,
        n: None,
        stream: Some(false),
        stop: None,
        presence_penalty: None,
        frequency_penalty: None,
        logit_bias: None,
        user: None,
        tools: Some(vec![Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "get_weather".to_string(),
                description: Some("Get the current weather in a location".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "location": {
                            "type": "string",
                            "description": "The city and state, e.g. San Francisco, CA"
                        },
                        "unit": {
                            "type": "string",
                            "enum": ["celsius", "fahrenheit"]
                        }
                    },
                    "required": ["location"]
                })),
            },
        }]),
        tool_choice: Some(ToolChoice::String("auto".to_string())),
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (anthropic_req, _) = result.unwrap();

    // Verify tool conversion
    assert!(anthropic_req.tools.is_some());
    let tools = anthropic_req.tools.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "get_weather");
    assert_eq!(
        tools[0].description,
        "Get the current weather in a location"
    );

    // Verify input_schema matches parameters
    let schema = &tools[0].input_schema;
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["location"].is_object());
    assert!(schema["required"].is_array());
}

#[tokio::test]
async fn test_tool_choice_modes() {
    // Test different tool_choice modes conversion
    let base_request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Test".to_string()),
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
        tools: Some(vec![Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "test_tool".to_string(),
                description: Some("Test tool".to_string()),
                parameters: None,
            },
        }]),
        tool_choice: None,
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    // Test auto mode
    let mut request = base_request.clone();
    request.tool_choice = Some(ToolChoice::String("auto".to_string()));
    let (anthropic_req, _) = openai_to_anthropic::convert_request(&request)
        .await
        .unwrap();
    assert!(anthropic_req.tool_choice.is_some());
    if let Some(tool_choice) = anthropic_req.tool_choice {
        match tool_choice {
            llm_gateway::models::anthropic::ToolChoice::Auto { r#type } => {
                assert_eq!(r#type, "auto");
            }
            _ => panic!("Expected Auto tool choice"),
        }
    }

    // Test required mode
    let mut request = base_request.clone();
    request.tool_choice = Some(ToolChoice::String("required".to_string()));
    let (anthropic_req, _) = openai_to_anthropic::convert_request(&request)
        .await
        .unwrap();
    assert!(anthropic_req.tool_choice.is_some());
    if let Some(tool_choice) = anthropic_req.tool_choice {
        match tool_choice {
            llm_gateway::models::anthropic::ToolChoice::Any { r#type } => {
                assert_eq!(r#type, "any");
            }
            _ => panic!("Expected Any tool choice"),
        }
    }

    // Test specific tool mode
    let mut request = base_request.clone();
    request.tool_choice = Some(ToolChoice::Specific {
        r#type: "function".to_string(),
        function: llm_gateway::models::openai::ToolChoiceFunction {
            name: "test_tool".to_string(),
        },
    });
    let (anthropic_req, _) = openai_to_anthropic::convert_request(&request)
        .await
        .unwrap();
    assert!(anthropic_req.tool_choice.is_some());
    if let Some(tool_choice) = anthropic_req.tool_choice {
        match tool_choice {
            llm_gateway::models::anthropic::ToolChoice::Tool { r#type, name } => {
                assert_eq!(r#type, "tool");
                assert_eq!(name, "test_tool");
            }
            _ => panic!("Expected Tool choice with specific tool name"),
        }
    }
}

#[test]
fn test_tool_use_response_conversion() {
    // Test converting Anthropic tool_use response back to OpenAI format
    let anthropic_response = MessagesResponse {
        id: "msg_123".to_string(),
        response_type: "message".to_string(),
        role: "assistant".to_string(),
        content: vec![
            AnthropicContentBlock {
                block_type: "text".to_string(),
                text: Some("I'll check the weather for you.".to_string()),
                source: None,
                id: None,
                name: None,
                input: None,
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            },
            AnthropicContentBlock {
                block_type: "tool_use".to_string(),
                text: None,
                source: None,
                id: Some("toolu_abc123".to_string()),
                name: Some("get_weather".to_string()),
                input: Some(serde_json::json!({
                    "location": "San Francisco, CA",
                    "unit": "fahrenheit"
                })),
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            },
        ],
        model: "claude-3-5-sonnet-20241022".to_string(),
        stop_reason: Some("tool_use".to_string()),
        stop_sequence: None,
        usage: TokenUsage {
            input_tokens: 50,
            output_tokens: 100,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        },
    };

    let result = anthropic_response::convert_response(&anthropic_response);
    assert!(result.is_ok());

    let openai_response = result.unwrap();
    assert_eq!(openai_response.choices.len(), 1);

    let choice = &openai_response.choices[0];

    // Verify text content
    let content_text = choice.message.content.extract_text();
    assert_eq!(content_text, "I'll check the weather for you.");

    // Verify tool calls
    assert!(choice.message.tool_calls.is_some());
    let tool_calls = choice.message.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0].id, "toolu_abc123");
    assert_eq!(tool_calls[0].tool_type, "function");
    assert_eq!(tool_calls[0].function.name, "get_weather");

    // Verify tool arguments
    let args: serde_json::Value = serde_json::from_str(&tool_calls[0].function.arguments).unwrap();
    assert_eq!(args["location"], "San Francisco, CA");
    assert_eq!(args["unit"], "fahrenheit");

    // Verify finish reason
    assert_eq!(choice.finish_reason, Some("tool_use".to_string()));

    // Verify usage
    assert!(openai_response.usage.is_some());
    let usage = openai_response.usage.unwrap();
    assert_eq!(usage.prompt_tokens, 50);
    assert_eq!(usage.completion_tokens, 100);
    assert_eq!(usage.total_tokens, 150);
}

#[test]
fn test_multiple_tool_calls_in_response() {
    // Test handling multiple tool calls in a single response
    let anthropic_response = MessagesResponse {
        id: "msg_456".to_string(),
        response_type: "message".to_string(),
        role: "assistant".to_string(),
        content: vec![
            AnthropicContentBlock {
                block_type: "text".to_string(),
                text: Some("I'll check both locations.".to_string()),
                source: None,
                id: None,
                name: None,
                input: None,
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            },
            AnthropicContentBlock {
                block_type: "tool_use".to_string(),
                text: None,
                source: None,
                id: Some("toolu_1".to_string()),
                name: Some("get_weather".to_string()),
                input: Some(serde_json::json!({"location": "San Francisco, CA"})),
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            },
            AnthropicContentBlock {
                block_type: "tool_use".to_string(),
                text: None,
                source: None,
                id: Some("toolu_2".to_string()),
                name: Some("get_weather".to_string()),
                input: Some(serde_json::json!({"location": "New York, NY"})),
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            },
        ],
        model: "claude-3-5-sonnet-20241022".to_string(),
        stop_reason: Some("tool_use".to_string()),
        stop_sequence: None,
        usage: TokenUsage {
            input_tokens: 60,
            output_tokens: 120,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        },
    };

    let openai_response = anthropic_response::convert_response(&anthropic_response).unwrap();
    let tool_calls = openai_response.choices[0]
        .message
        .tool_calls
        .as_ref()
        .unwrap();

    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].id, "toolu_1");
    assert_eq!(tool_calls[1].id, "toolu_2");
}

#[tokio::test]
async fn test_gemini_tool_conversion() {
    // Test OpenAI → Gemini tool conversion
    let request = ChatCompletionRequest {
        model: "gemini-1.5-pro".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Calculate 5 + 3".to_string()),
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
        tools: Some(vec![Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "calculator".to_string(),
                description: Some("Perform basic arithmetic".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operation": {"type": "string", "enum": ["add", "subtract", "multiply", "divide"]},
                        "a": {"type": "number"},
                        "b": {"type": "number"}
                    },
                    "required": ["operation", "a", "b"]
                })),
            },
        }]),
        tool_choice: Some(ToolChoice::String("auto".to_string())),
        response_format: None,
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_gemini::convert_request(&request).await;
    assert!(result.is_ok());

    let (gemini_req, _) = result.unwrap();

    // Verify tools are converted to function_declarations
    assert!(gemini_req.tools.is_some());
    let tools = gemini_req.tools.unwrap();
    assert_eq!(tools.len(), 1);

    let func_decls = &tools[0].function_declarations;
    assert_eq!(func_decls.len(), 1);
    assert_eq!(func_decls[0].name, "calculator");
    assert_eq!(
        func_decls[0].description,
        "Perform basic arithmetic"
    );

    // Verify parameters schema
    assert!(func_decls[0].parameters.is_some());
    let params = func_decls[0].parameters.as_ref().unwrap();
    assert_eq!(params["type"], "object");
    assert!(params["properties"]["operation"].is_object());
}

#[tokio::test]
async fn test_tool_result_message_conversion() {
    // Test converting tool result messages
    use llm_gateway::models::openai::ContentBlock;

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![
            ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text("I'll get the weather.".to_string()),
                name: None,
                tool_calls: Some(vec![llm_gateway::models::openai::ToolCall {
                    id: "toolu_123".to_string(),
                    tool_type: "function".to_string(),
                    function: llm_gateway::models::openai::FunctionCall {
                        name: "get_weather".to_string(),
                        arguments: "{\"location\":\"SF\"}".to_string(),
                    },
                }]),
            },
            ChatMessage {
                role: "tool".to_string(),
                content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                    tool_call_id: "toolu_123".to_string(),
                    content: "72°F and sunny".to_string(),
                }]),
                name: Some("get_weather".to_string()),
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

    // This should convert successfully
    let result = openai_to_anthropic::convert_request(&request).await;
    if let Err(e) = &result {
        eprintln!("Conversion error: {:?}", e);
    }
    assert!(result.is_ok());
}
