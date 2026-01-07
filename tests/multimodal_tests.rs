/// Integration tests for multimodal support (vision + tools + streaming)
use llm_gateway::{
    models::openai::{
        ChatCompletionRequest, ChatMessage, ContentBlock, ImageUrl, MessageContent, Tool,
        ToolChoice,
    },
    converters::openai_to_anthropic,
};

#[tokio::test]
async fn test_vision_with_text_conversion() {
    // Test converting a request with both text and image to Anthropic format
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "What's in this image?".to_string(),
                },
                ContentBlock::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string(),
                        detail: Some("high".to_string()),
                    },
                },
            ]),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(1024),
        temperature: Some(0.7),
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

    let (anthropic_req, warnings) = result.unwrap();
    assert_eq!(anthropic_req.model, "claude-3-5-sonnet-20241022");
    assert_eq!(anthropic_req.messages.len(), 1);

    // Check that the message has blocks
    match &anthropic_req.messages[0].content {
        llm_gateway::models::anthropic::MessageContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 2);
            assert_eq!(blocks[0].block_type, "text");
            assert_eq!(blocks[1].block_type, "image");
        }
        _ => panic!("Expected blocks format for multimodal content"),
    }

    // No warnings expected for valid content
    assert_eq!(warnings.to_header_value(), None);
}

#[tokio::test]
async fn test_vision_with_tools() {
    // Test combining vision with tool calling
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Analyze this image and extract key information".to_string(),
                },
                ContentBlock::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string(),
                        detail: None,
                    },
                },
            ]),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(2048),
        temperature: Some(0.5),
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
            function: llm_gateway::models::openai::FunctionDefinition {
                name: "extract_data".to_string(),
                description: Some("Extract structured data from image analysis".to_string()),
                parameters: Some(serde_json::json!({
                    "type": "object",
                    "properties": {
                        "objects": {"type": "array", "items": {"type": "string"}},
                        "text": {"type": "string"},
                        "sentiment": {"type": "string", "enum": ["positive", "neutral", "negative"]}
                    },
                    "required": ["objects"]
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
    if let Err(e) = &result {
        eprintln!("Conversion error: {:?}", e);
    }
    assert!(result.is_ok());

    let (anthropic_req, _warnings) = result.unwrap();

    // Verify tools are converted
    assert!(anthropic_req.tools.is_some());
    let tools = anthropic_req.tools.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "extract_data");

    // Verify tool_choice is converted
    assert!(anthropic_req.tool_choice.is_some());
}

#[tokio::test]
async fn test_multiple_images_in_request() {
    // Test handling multiple images in a single request
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Compare these two images".to_string(),
                },
                ContentBlock::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string(),
                        detail: Some("low".to_string()),
                    },
                },
                ContentBlock::Text {
                    text: "versus".to_string(),
                },
                ContentBlock::ImageUrl {
                    image_url: ImageUrl {
                        url: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==".to_string(),
                        detail: Some("high".to_string()),
                    },
                },
            ]),
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

    let (anthropic_req, _) = result.unwrap();

    // Verify all content blocks are preserved
    match &anthropic_req.messages[0].content {
        llm_gateway::models::anthropic::MessageContent::Blocks(blocks) => {
            assert_eq!(blocks.len(), 4);
            assert_eq!(blocks[0].block_type, "text");
            assert_eq!(blocks[1].block_type, "image");
            assert_eq!(blocks[2].block_type, "text");
            assert_eq!(blocks[3].block_type, "image");
        }
        _ => panic!("Expected blocks format"),
    }
}

#[test]
fn test_streaming_event_with_tool_use() {
    // Test converting Anthropic streaming events with tool use to OpenAI format
    use llm_gateway::converters::anthropic_response::convert_stream_event;
    use llm_gateway::models::anthropic::{ContentBlock as AnthropicContentBlock, StreamEvent};

    // Test content_block_start with tool_use
    let event = StreamEvent {
        event_type: "content_block_start".to_string(),
        message: None,
        index: Some(0),
        content_block: Some(AnthropicContentBlock {
            block_type: "tool_use".to_string(),
            text: None,
            source: None,
            id: Some("toolu_123".to_string()),
            name: Some("extract_data".to_string()),
            input: None,
            tool_use_id: None,
            content: None,
            is_error: None,
            cache_control: None,
            thinking: None,
        }),
        delta: None,
        usage: None,
    };

    let chunk = convert_stream_event(&event, "chatcmpl-123");
    assert!(chunk.is_some());

    let chunk = chunk.unwrap();
    assert_eq!(chunk.choices[0].delta.tool_calls.as_ref().unwrap().len(), 1);
    assert_eq!(
        chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0].id,
        Some("toolu_123".to_string())
    );
    assert_eq!(
        chunk.choices[0].delta.tool_calls.as_ref().unwrap()[0]
            .function
            .as_ref()
            .unwrap()
            .name,
        Some("extract_data".to_string())
    );
}

#[test]
fn test_streaming_tool_input_delta() {
    // Test converting tool input JSON deltas
    use llm_gateway::converters::anthropic_response::convert_stream_event;
    use llm_gateway::models::anthropic::{Delta as AnthropicDelta, StreamEvent};

    let event = StreamEvent {
        event_type: "content_block_delta".to_string(),
        message: None,
        index: Some(0),
        content_block: None,
        delta: Some(AnthropicDelta {
            delta_type: "input_json_delta".to_string(),
            text: None,
            stop_reason: None,
            partial_json: Some("{\"objects\":[\"cat\"".to_string()),
        }),
        usage: None,
    };

    let chunk = convert_stream_event(&event, "chatcmpl-123");
    assert!(chunk.is_some());

    let chunk = chunk.unwrap();
    let tool_calls = chunk.choices[0].delta.tool_calls.as_ref().unwrap();
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(
        tool_calls[0].function.as_ref().unwrap().arguments,
        Some("{\"objects\":[\"cat\"".to_string())
    );
}

#[tokio::test]
async fn test_backward_compatibility_simple_text() {
    // Ensure simple text messages still work (backward compatibility)
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Hello, Claude!".to_string()),
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

    let (anthropic_req, _) = result.unwrap();
    assert_eq!(anthropic_req.messages.len(), 1);

    // Simple text should work
    match &anthropic_req.messages[0].content {
        llm_gateway::models::anthropic::MessageContent::Text(text) => {
            assert_eq!(text, "Hello, Claude!");
        }
        _ => panic!("Expected simple text format for text-only message"),
    }
}
