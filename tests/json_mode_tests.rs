/// Integration tests for JSON mode and structured outputs across providers
use llm_gateway::{
    converters::{openai_to_anthropic, openai_to_gemini},
    models::openai::{
        ChatCompletionRequest, ChatMessage, JsonSchemaSpec, MessageContent, ResponseFormat,
    },
};

#[tokio::test]
async fn test_json_object_mode_anthropic() {
    // Test JSON object mode for Anthropic (system prompt injection approach)
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(
                "List three colors as a JSON object with a 'colors' array".to_string(),
            ),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(200),
        temperature: Some(0.5),
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
        response_format: Some(ResponseFormat::JsonObject),
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (anthropic_req, warnings) = result.unwrap();

    // For Anthropic, JSON mode is implemented via system prompt injection
    // Verify that system prompt was modified to include JSON instruction
    assert!(anthropic_req.system.is_some());

    let system_text = match &anthropic_req.system.unwrap() {
        llm_gateway::models::anthropic::MessageContent::Text(text) => text.clone(),
        llm_gateway::models::anthropic::MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| b.text.as_ref())
            .cloned()
            .collect::<Vec<_>>()
            .join(" "),
    };

    assert!(
        system_text.contains("JSON") || system_text.contains("json"),
        "System prompt should contain JSON instruction"
    );

    // Should have warnings about JSON mode workaround
    assert!(warnings.to_header_value().is_some());
}

#[tokio::test]
async fn test_json_schema_mode_anthropic() {
    // Test JSON schema mode for Anthropic
    let schema = serde_json::json!({
        "name": "color_list",
        "schema": {
            "type": "object",
            "properties": {
                "colors": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "hex": {"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$"}
                        },
                        "required": ["name", "hex"]
                    }
                }
            },
            "required": ["colors"]
        },
        "strict": true
    });

    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("List three primary colors with hex codes".to_string()),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(300),
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
        response_format: Some(ResponseFormat::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: schema["name"].as_str().unwrap().to_string(),
                description: None,
                schema: schema["schema"].clone(),
                strict: Some(true),
            },
        }),
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (anthropic_req, warnings) = result.unwrap();

    // Verify system prompt was injected with schema details
    assert!(anthropic_req.system.is_some());

    // Should have warnings about using workaround
    assert!(warnings.to_header_value().is_some());
}

#[tokio::test]
async fn test_json_mode_gemini_native() {
    // Test JSON mode for Gemini (native support via response_mime_type)
    let request = ChatCompletionRequest {
        model: "gemini-1.5-pro".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("List three fruits as a JSON array".to_string()),
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
        response_format: Some(ResponseFormat::JsonObject),
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_gemini::convert_request(&request).await;
    assert!(result.is_ok());

    let (gemini_req, _warnings) = result.unwrap();

    // Gemini should have native JSON mode support
    assert_eq!(
        gemini_req.generation_config.as_ref().unwrap().response_mime_type,
        Some("application/json".to_string())
    );
}

#[tokio::test]
async fn test_json_schema_mode_gemini_native() {
    // Test JSON schema mode for Gemini (native support)
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "answer": {"type": "string"},
            "confidence": {"type": "number", "minimum": 0, "maximum": 1}
        },
        "required": ["answer", "confidence"]
    });

    let request = ChatCompletionRequest {
        model: "gemini-1.5-pro".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("What is 2+2? Provide answer with confidence.".to_string()),
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
        response_format: Some(ResponseFormat::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: "answer_with_confidence".to_string(),
                description: Some("Answer with confidence score".to_string()),
                schema: schema.clone(),
                strict: Some(true),
            },
        }),
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_gemini::convert_request(&request).await;
    assert!(result.is_ok());

    let (gemini_req, _) = result.unwrap();

    // Verify Gemini uses native schema support
    assert_eq!(
        gemini_req.generation_config.as_ref().unwrap().response_mime_type,
        Some("application/json".to_string())
    );
    assert!(gemini_req.generation_config.as_ref().unwrap().response_schema.is_some());

    let response_schema = gemini_req.generation_config.as_ref().unwrap().response_schema.as_ref().unwrap();
    assert_eq!(response_schema["type"], "object");
    assert_eq!(response_schema["properties"]["answer"]["type"], "string");
    assert_eq!(response_schema["properties"]["confidence"]["type"], "number");
}

#[tokio::test]
async fn test_text_mode_no_json() {
    // Test that text mode (default) doesn't inject JSON instructions
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Tell me a joke".to_string()),
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
        response_format: Some(ResponseFormat::Text),
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    let result = openai_to_anthropic::convert_request(&request).await;
    assert!(result.is_ok());

    let (anthropic_req, _) = result.unwrap();

    // No special handling for text mode
    // System should be None or not contain JSON instructions
    if let Some(system) = anthropic_req.system {
        let system_text = match system {
            llm_gateway::models::anthropic::MessageContent::Text(text) => text,
            llm_gateway::models::anthropic::MessageContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| b.text.as_ref())
                .cloned()
                .collect::<Vec<_>>()
                .join(" "),
        };

        // Should not contain JSON-specific instructions
        assert!(
            !system_text.to_lowercase().contains("json object"),
            "Text mode should not inject JSON instructions"
        );
    }
}

#[tokio::test]
async fn test_complex_schema_with_nested_objects() {
    // Test complex nested schema
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "person": {
                "type": "object",
                "properties": {
                    "name": {"type": "string"},
                    "age": {"type": "integer"},
                    "address": {
                        "type": "object",
                        "properties": {
                            "street": {"type": "string"},
                            "city": {"type": "string"},
                            "zipcode": {"type": "string", "pattern": "^[0-9]{5}$"}
                        },
                        "required": ["city"]
                    }
                },
                "required": ["name"]
            },
            "tags": {
                "type": "array",
                "items": {"type": "string"}
            }
        },
        "required": ["person"]
    });

    let request = ChatCompletionRequest {
        model: "gemini-1.5-pro".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(
                "Create a sample person record for John Doe in New York".to_string(),
            ),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(500),
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
        response_format: Some(ResponseFormat::JsonSchema {
            json_schema: JsonSchemaSpec {
                name: "person_record".to_string(),
                description: Some("Person information with address".to_string()),
                schema: schema.clone(),
                strict: Some(true),
            },
        }),
        seed: None,
        logprobs: None,
        top_logprobs: None,
        service_tier: None,
    };

    // Gemini should handle this with native support
    let result = openai_to_gemini::convert_request(&request).await;
    assert!(result.is_ok());

    let (gemini_req, _) = result.unwrap();
    assert!(gemini_req.generation_config.as_ref().unwrap().response_schema.is_some());

    let response_schema = gemini_req.generation_config.as_ref().unwrap().response_schema.as_ref().unwrap();
    assert_eq!(response_schema["type"], "object");
    assert!(response_schema["properties"]["person"].is_object());
    assert!(response_schema["properties"]["person"]["properties"]["address"].is_object());
}

#[tokio::test]
async fn test_no_response_format_specified() {
    // Test that requests without response_format work normally
    let request = ChatCompletionRequest {
        model: "claude-3-5-sonnet-20241022".to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text("Hello".to_string()),
            name: None,
            tool_calls: None,
        }],
        max_tokens: Some(50),
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

    // Should work without issues
    let (_anthropic_req, _) = result.unwrap();
}
