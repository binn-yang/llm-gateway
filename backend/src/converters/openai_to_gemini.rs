use crate::{
    error::AppError,
    image_utils,
    models::{
        gemini::{
            Content, GenerateContentRequest, GenerationConfig, InlineData, Part,
            SystemInstruction,
        },
        openai::{
            ChatCompletionRequest, ChatMessage, ContentBlock as OpenAIContentBlock,
            MessageContent as OpenAIMessageContent, ResponseFormat,
        },
    },
};

/// Convert OpenAI ChatCompletionRequest to Gemini GenerateContentRequest
/// Returns (request, warnings) tuple
pub async fn convert_request(
    openai_req: &ChatCompletionRequest,
) -> Result<(GenerateContentRequest, crate::conversion_warnings::ConversionWarnings), AppError> {
    let mut warnings = crate::conversion_warnings::ConversionWarnings::new();

    // Collect warnings for unsupported OpenAI parameters
    if openai_req.seed.is_some() {
        tracing::warn!("OpenAI parameter 'seed' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("seed", "Gemini");
    }
    if openai_req.logprobs.is_some() {
        tracing::warn!("OpenAI parameter 'logprobs' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("logprobs", "Gemini");
    }
    if openai_req.top_logprobs.is_some() {
        tracing::warn!("OpenAI parameter 'top_logprobs' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("top_logprobs", "Gemini");
    }
    if openai_req.logit_bias.is_some() {
        tracing::warn!("OpenAI parameter 'logit_bias' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("logit_bias", "Gemini");
    }
    if openai_req.service_tier.is_some() {
        tracing::warn!("OpenAI parameter 'service_tier' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("service_tier", "Gemini");
    }
    if openai_req.presence_penalty.is_some() {
        tracing::warn!("OpenAI parameter 'presence_penalty' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("presence_penalty", "Gemini");
    }
    if openai_req.frequency_penalty.is_some() {
        tracing::warn!("OpenAI parameter 'frequency_penalty' not supported by Gemini, ignoring");
        warnings.add_unsupported_param("frequency_penalty", "Gemini");
    }
    if let Some(n) = openai_req.n {
        if n > 1 {
            tracing::warn!("OpenAI parameter 'n > 1' (multiple completions) not supported by Gemini, returning single completion");
            warnings.add_warning(format!("Multiple completions (n={}) not supported by Gemini, returning single completion", n));
        }
    }
    // Extract system instruction
    let (system_instruction, messages) = extract_system_instruction(&openai_req.messages).await?;

    // Convert messages
    let mut contents = Vec::new();
    for msg in messages.iter() {
        let parts = convert_message_content(&msg.content).await?;
        contents.push(Content {
            // Gemini uses "model" instead of "assistant"
            role: if msg.role == "assistant" {
                "model".to_string()
            } else {
                msg.role.clone()
            },
            parts,
        });
    }

    // Generation config
    let generation_config = if openai_req.max_tokens.is_some()
        || openai_req.temperature.is_some()
        || openai_req.top_p.is_some()
        || openai_req.stop.is_some()
        || openai_req.response_format.is_some()
    {
        let (response_mime_type, response_schema) = match &openai_req.response_format {
            Some(ResponseFormat::JsonObject) => {
                (Some("application/json".to_string()), None)
            }
            Some(ResponseFormat::JsonSchema { json_schema }) => (
                Some("application/json".to_string()),
                Some(json_schema.schema.clone()),
            ),
            _ => (None, None),
        };

        Some(GenerationConfig {
            temperature: openai_req.temperature,
            top_p: openai_req.top_p,
            top_k: None,
            max_output_tokens: openai_req.max_tokens,
            stop_sequences: openai_req.stop.clone(),
            response_mime_type,
            response_schema,
        })
    } else {
        None
    };

    // Convert tools
    let (tools, tool_config) = if let Some(openai_tools) = &openai_req.tools {
        let gemini_tools = convert_tools(openai_tools);
        let gemini_tool_config = openai_req
            .tool_choice
            .as_ref()
            .and_then(convert_tool_config);
        (Some(vec![gemini_tools]), gemini_tool_config)
    } else {
        (None, None)
    };

    let request = GenerateContentRequest {
        contents,
        system_instruction,
        generation_config,
        safety_settings: None, // OpenAI doesn't have safety settings
        tools,
        tool_config,
    };

    Ok((request, warnings))
}

/// Convert OpenAI MessageContent to Gemini Parts
/// Phase 2: Now handles images in addition to text
async fn convert_message_content(content: &OpenAIMessageContent) -> Result<Vec<Part>, AppError> {
    match content {
        OpenAIMessageContent::Text(text) => Ok(vec![Part::Text { text: text.clone() }]),
        OpenAIMessageContent::Blocks(blocks) => {
            let mut parts = Vec::new();

            for block in blocks {
                match block {
                    OpenAIContentBlock::Text { text } => {
                        parts.push(Part::Text { text: text.clone() });
                    }
                    OpenAIContentBlock::ImageUrl { image_url } => {
                        // Convert OpenAI image_url to Gemini InlineData
                        let (mime_type, data) = if image_url.url.starts_with("data:") {
                            // Parse data URL
                            image_utils::parse_data_url(&image_url.url)?
                        } else {
                            // Fetch HTTP(S) URL and convert to base64
                            image_utils::fetch_image_as_base64(&image_url.url).await?
                        };

                        parts.push(Part::InlineData {
                            inline_data: InlineData {
                                mime_type,
                                data,
                            },
                        });
                    }
                    OpenAIContentBlock::ToolUse { .. } | OpenAIContentBlock::ToolResult { .. } => {
                        // Tool conversion will be added in Phase 3
                        tracing::warn!("Tool use blocks not yet supported, skipping");
                    }
                }
            }

            // If we have parts, return them, otherwise error
            if !parts.is_empty() {
                Ok(parts)
            } else {
                Err(AppError::ConversionError(
                    "Message content is empty after conversion (all blocks were unsupported)".to_string()
                ))
            }
        }
    }
}

/// Extract system instruction from OpenAI messages
/// Returns (system_instruction, remaining_messages)
async fn extract_system_instruction(
    messages: &[ChatMessage],
) -> Result<(Option<SystemInstruction>, Vec<ChatMessage>), AppError> {
    if let Some(first) = messages.first() {
        if first.role == "system" {
            let parts = convert_message_content(&first.content).await?;
            let system_instruction = Some(SystemInstruction { parts });
            let rest = messages.iter().skip(1).cloned().collect();
            return Ok((system_instruction, rest));
        }
    }
    Ok((None, messages.to_vec()))
}

/// Convert OpenAI tools to Gemini tools
fn convert_tools(openai_tools: &[crate::models::openai::Tool]) -> crate::models::gemini::Tool {
    use crate::models::gemini::{FunctionDeclaration, Tool as GeminiTool};

    let function_declarations = openai_tools
        .iter()
        .map(|tool| FunctionDeclaration {
            name: tool.function.name.clone(),
            description: tool
                .function
                .description
                .clone()
                .unwrap_or_else(|| "No description provided".to_string()),
            parameters: tool.function.parameters.clone(),
        })
        .collect();

    GeminiTool {
        function_declarations,
    }
}

/// Convert OpenAI tool_choice to Gemini tool_config
fn convert_tool_config(
    openai_choice: &crate::models::openai::ToolChoice,
) -> Option<crate::models::gemini::ToolConfig> {
    use crate::models::gemini::{FunctionCallingConfig, ToolConfig};
    use crate::models::openai::ToolChoice as OpenAIToolChoice;

    match openai_choice {
        OpenAIToolChoice::String(s) => match s.as_str() {
            "none" => Some(ToolConfig {
                function_calling_config: FunctionCallingConfig {
                    mode: "NONE".to_string(),
                    allowed_function_names: None,
                },
            }),
            "auto" => Some(ToolConfig {
                function_calling_config: FunctionCallingConfig {
                    mode: "AUTO".to_string(),
                    allowed_function_names: None,
                },
            }),
            "required" => Some(ToolConfig {
                function_calling_config: FunctionCallingConfig {
                    mode: "ANY".to_string(),
                    allowed_function_names: None,
                },
            }),
            _ => None,
        },
        OpenAIToolChoice::Specific { function, .. } => Some(ToolConfig {
            function_calling_config: FunctionCallingConfig {
                mode: "ANY".to_string(),
                allowed_function_names: Some(vec![function.name.clone()]),
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::openai::MessageContent;

    #[tokio::test]
    async fn test_extract_system_instruction() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text("You are helpful.".to_string()),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Hello!".to_string()),
                name: None,
                tool_calls: None,
            },
        ];

        let (system, remaining) = extract_system_instruction(&messages).await.unwrap();
        assert!(system.is_some());
        if let Part::Text { text } = &system.unwrap().parts[0] {
            assert_eq!(text, "You are helpful.");
        } else {
            panic!("Expected Text part");
        }
        assert_eq!(remaining.len(), 1);
    }

    #[tokio::test]
    async fn test_convert_request_basic() {
        let openai_req = ChatCompletionRequest {
            model: "gemini-1.5-pro".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text("You are helpful.".to_string()),
                    name: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::Text("Hi!".to_string()),
                    name: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: MessageContent::Text("Hello!".to_string()),
                    name: None,
                    tool_calls: None,
                },
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: Some(0.9),
            n: None,
            stream: None,
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
        };

        let (gemini_req, _warnings) = convert_request(&openai_req).await.unwrap();
        assert!(gemini_req.system_instruction.is_some());
        assert_eq!(gemini_req.contents.len(), 2);
        assert_eq!(gemini_req.contents[0].role, "user");
        if let Part::Text { text } = &gemini_req.contents[0].parts[0] {
            assert_eq!(text, "Hi!");
        } else {
            panic!("Expected Text part");
        }
        assert_eq!(gemini_req.contents[1].role, "model"); // assistant → model
        if let Part::Text { text } = &gemini_req.contents[1].parts[0] {
            assert_eq!(text, "Hello!");
        } else {
            panic!("Expected Text part");
        }
        assert_eq!(
            gemini_req
                .generation_config
                .as_ref()
                .unwrap()
                .max_output_tokens,
            Some(100)
        );
    }

    #[tokio::test]
    async fn test_convert_request_assistant_to_model() {
        let openai_req = ChatCompletionRequest {
            model: "gemini-1.5-pro".to_string(),
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text("I'm an assistant".to_string()),
                name: None,
                tool_calls: None,
            }],
            max_tokens: None,
            temperature: None,
            top_p: None,
            n: None,
            stream: None,
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
        };

        let (gemini_req, _warnings) = convert_request(&openai_req).await.unwrap();
        assert_eq!(gemini_req.contents[0].role, "model");
    }
}
