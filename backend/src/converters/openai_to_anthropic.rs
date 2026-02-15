use crate::{
    error::AppError,
    image_utils,
    models::{
        anthropic::{
            ContentBlock as AnthropicContentBlock, ImageSource, Message,
            MessageContent as AnthropicMessageContent, MessagesRequest,
            Tool as AnthropicTool, ToolChoice as AnthropicToolChoice,
        },
        openai::{
            ChatCompletionRequest, ChatMessage, ContentBlock as OpenAIContentBlock,
            MessageContent as OpenAIMessageContent, ResponseFormat, Tool as OpenAITool,
            ToolChoice as OpenAIToolChoice,
        },
    },
};

/// Convert OpenAI ChatCompletionRequest to Anthropic MessagesRequest
/// Returns (request, warnings) tuple
pub async fn convert_request(
    openai_req: &ChatCompletionRequest,
) -> Result<(MessagesRequest, crate::conversion_warnings::ConversionWarnings), AppError> {
    let mut warnings = crate::conversion_warnings::ConversionWarnings::new();

    // Collect warnings for unsupported OpenAI parameters
    if openai_req.seed.is_some() {
        tracing::warn!("OpenAI parameter 'seed' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("seed", "Anthropic");
    }
    if openai_req.logprobs.is_some() {
        tracing::warn!("OpenAI parameter 'logprobs' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("logprobs", "Anthropic");
    }
    if openai_req.top_logprobs.is_some() {
        tracing::warn!("OpenAI parameter 'top_logprobs' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("top_logprobs", "Anthropic");
    }
    if openai_req.logit_bias.is_some() {
        tracing::warn!("OpenAI parameter 'logit_bias' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("logit_bias", "Anthropic");
    }
    if openai_req.service_tier.is_some() {
        tracing::warn!("OpenAI parameter 'service_tier' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("service_tier", "Anthropic");
    }
    if openai_req.presence_penalty.is_some() {
        tracing::warn!("OpenAI parameter 'presence_penalty' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("presence_penalty", "Anthropic");
    }
    if openai_req.frequency_penalty.is_some() {
        tracing::warn!("OpenAI parameter 'frequency_penalty' not supported by Anthropic, ignoring");
        warnings.add_unsupported_param("frequency_penalty", "Anthropic");
    }
    if let Some(n) = openai_req.n {
        if n > 1 {
            tracing::warn!("OpenAI parameter 'n > 1' (multiple completions) not supported by Anthropic, returning single completion");
            warnings.add_warning(format!("Multiple completions (n={}) not supported by Anthropic, returning single completion", n));
        }
    }
    // Extract system message (Anthropic uses a separate system field)
    let (mut system, messages) = extract_system_message(&openai_req.messages).await?;

    // Apply JSON mode workaround for Anthropic (inject into system prompt)
    if let Some(response_format) = &openai_req.response_format {
        system = Some(apply_json_mode_to_system(system, response_format)?);

        // Add warning for JSON mode workaround
        match response_format {
            crate::models::openai::ResponseFormat::JsonObject => {
                warnings.add_warning(
                    "JSON mode implemented via system prompt injection for Anthropic".to_string()
                );
            }
            crate::models::openai::ResponseFormat::JsonSchema { .. } => {
                warnings.add_warning(
                    "JSON schema mode implemented via system prompt injection for Anthropic (best effort, not guaranteed)".to_string()
                );
            }
            _ => {}
        }
    }

    // Convert messages (filter out system messages)
    let mut anthropic_messages = Vec::new();
    for msg in messages.iter() {
        anthropic_messages.push(Message {
            role: msg.role.clone(),
            content: convert_message_content(&msg.content).await?,
        });
    }

    // Anthropic requires max_tokens, default to 4096 if not provided
    let max_tokens = openai_req.max_tokens.unwrap_or(4096);

    // Anthropic temperature is 0-1, clip if necessary
    let temperature = openai_req.temperature.map(|t| {
        if t > 1.0 {
            tracing::warn!(
                "Temperature {} exceeds Anthropic maximum (1.0), clipping to 1.0",
                t
            );
            1.0
        } else {
            t
        }
    });

    // Convert stop sequences
    let stop_sequences = openai_req.stop.clone();

    let request = MessagesRequest {
        model: openai_req.model.clone(),
        system,
        messages: anthropic_messages,
        max_tokens,
        temperature,
        top_p: openai_req.top_p,
        top_k: None, // OpenAI doesn't have top_k
        stream: openai_req.stream,
        stop_sequences,
        tools: openai_req.tools.as_ref().map(|t| convert_tools(t)),
        tool_choice: openai_req.tool_choice.as_ref().and_then(convert_tool_choice),
        thinking: None, // OpenAI doesn't have extended thinking
        metadata: None, // OpenAI doesn't have request metadata
    };

    Ok((request, warnings))
}

/// Convert OpenAI tools to Anthropic tools
fn convert_tools(openai_tools: &[OpenAITool]) -> Vec<AnthropicTool> {
    openai_tools
        .iter()
        .map(|tool| AnthropicTool {
            name: tool.function.name.clone(),
            description: tool.function.description.clone().unwrap_or_default(),
            input_schema: tool
                .function
                .parameters
                .clone()
                .unwrap_or_else(|| serde_json::json!({"type": "object", "properties": {}})),
            cache_control: None, // Will be set by apply_auto_caching if enabled
        })
        .collect()
}

/// Apply automatic caching to system prompt and tools based on configuration
/// Anthropic's caching requires cache_control on the last block of cacheable content
pub fn apply_auto_caching(
    request: &mut MessagesRequest,
    cache_config: &crate::config::CacheConfig,
) {
    // Auto-cache system prompt if enabled and it's large enough
    if cache_config.auto_cache_system {
        if let Some(ref mut system) = request.system {
            apply_caching_to_system(system, cache_config.min_system_tokens);
        }
    }

    // Auto-cache tools if enabled (cache on the last tool)
    if cache_config.auto_cache_tools {
        if let Some(ref mut tools) = request.tools {
            if let Some(last_tool) = tools.last_mut() {
                last_tool.cache_control = Some(crate::models::anthropic::CacheControl {
                    cache_type: "ephemeral".to_string(),
                });
                tracing::debug!("Applied auto-caching to tools (last tool marked)");
            }
        }
    }
}

/// Apply caching to system prompt by adding cache_control to the last block
fn apply_caching_to_system(system: &mut AnthropicMessageContent, min_tokens: u64) {
    match system {
        AnthropicMessageContent::Text(text) => {
            // Rough estimate: 1 token ≈ 4 characters
            let estimated_tokens = text.len() as u64 / 4;
            if estimated_tokens >= min_tokens {
                // Convert to blocks format and add cache_control
                let block = crate::models::anthropic::ContentBlock {
                    block_type: "text".to_string(),
                    text: Some(text.clone()),
                    source: None,
                    id: None,
                    name: None,
                    input: None,
                    tool_use_id: None,
                    content: None,
                    is_error: None,
                    cache_control: Some(crate::models::anthropic::CacheControl {
                        cache_type: "ephemeral".to_string(),
                    }),
                    thinking: None,
                };
                *system = AnthropicMessageContent::Blocks(vec![block]);
                tracing::debug!(
                    estimated_tokens = estimated_tokens,
                    "Applied auto-caching to system prompt (text → blocks with cache_control)"
                );
            }
        }
        AnthropicMessageContent::Blocks(blocks) => {
            // Estimate total tokens across all blocks
            let mut estimated_tokens = 0u64;
            for block in blocks.iter() {
                if let Some(ref text) = block.text {
                    estimated_tokens += text.len() as u64 / 4;
                }
            }

            if estimated_tokens >= min_tokens {
                // Add cache_control to the last block (Anthropic requirement)
                if let Some(last_block) = blocks.last_mut() {
                    last_block.cache_control = Some(crate::models::anthropic::CacheControl {
                        cache_type: "ephemeral".to_string(),
                    });
                    tracing::debug!(
                        estimated_tokens = estimated_tokens,
                        "Applied auto-caching to system prompt (last block marked)"
                    );
                }
            }
        }
    }
}

/// Apply JSON mode workaround by injecting instructions into system prompt
/// Anthropic doesn't have native response_format support, so we modify the system message
fn apply_json_mode_to_system(
    system: Option<AnthropicMessageContent>,
    response_format: &ResponseFormat,
) -> Result<AnthropicMessageContent, AppError> {
    let json_instruction = match response_format {
        ResponseFormat::Text => {
            // No modification needed for plain text
            return Ok(system.unwrap_or_else(|| AnthropicMessageContent::Text(String::new())));
        }
        ResponseFormat::JsonObject => {
            "\n\nIMPORTANT: You must respond with a valid JSON object. Do not include any text outside the JSON object."
        }
        ResponseFormat::JsonSchema { json_schema } => {
            // For JSON schema, include the schema in the instruction
            let schema_str = serde_json::to_string_pretty(&json_schema.schema)
                .unwrap_or_else(|_| "{}".to_string());
            return Ok(AnthropicMessageContent::Text(format!(
                "{}\n\nIMPORTANT: You must respond with a valid JSON object that strictly adheres to the following schema:\n\n```json\n{}\n```\n\nDo not include any text outside the JSON object.",
                system.as_ref().and_then(|s| match s {
                    AnthropicMessageContent::Text(t) => Some(t.as_str()),
                    _ => None,
                }).unwrap_or(""),
                schema_str
            )));
        }
    };

    // Append JSON instruction to existing system message
    let result = match system {
        Some(AnthropicMessageContent::Text(text)) => {
            AnthropicMessageContent::Text(format!("{}{}", text, json_instruction))
        }
        Some(AnthropicMessageContent::Blocks(mut blocks)) => {
            // Append JSON instruction as a new text block, preserving existing blocks
            // (which may contain images, cache_control, etc.)
            blocks.push(AnthropicContentBlock {
                block_type: "text".to_string(),
                text: Some(json_instruction.to_string()),
                source: None,
                id: None,
                name: None,
                input: None,
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            });
            AnthropicMessageContent::Blocks(blocks)
        }
        None => AnthropicMessageContent::Text(json_instruction.to_string()),
    };

    Ok(result)
}

/// Convert OpenAI tool_choice to Anthropic tool_choice
fn convert_tool_choice(openai_choice: &OpenAIToolChoice) -> Option<AnthropicToolChoice> {
    match openai_choice {
        OpenAIToolChoice::String(s) => match s.as_str() {
            "none" => None, // Anthropic doesn't have explicit "none", just omit tool_choice
            "auto" => Some(AnthropicToolChoice::Auto {
                r#type: "auto".to_string(),
            }),
            "required" => Some(AnthropicToolChoice::Any {
                r#type: "any".to_string(),
            }),
            _ => None,
        },
        OpenAIToolChoice::Specific { function, .. } => Some(AnthropicToolChoice::Tool {
            r#type: "tool".to_string(),
            name: function.name.clone(),
        }),
    }
}

/// Convert OpenAI MessageContent to Anthropic MessageContent
/// Phase 2: Now handles images in addition to text
async fn convert_message_content(
    content: &OpenAIMessageContent,
) -> Result<AnthropicMessageContent, AppError> {
    match content {
        OpenAIMessageContent::Text(text) => Ok(AnthropicMessageContent::Text(text.clone())),
        OpenAIMessageContent::Blocks(blocks) => {
            // Convert each content block
            let mut anthropic_blocks = Vec::new();

            for block in blocks {
                match block {
                    OpenAIContentBlock::Text { text } => {
                        anthropic_blocks.push(AnthropicContentBlock {
                            block_type: "text".to_string(),
                            text: Some(text.clone()),
                            source: None,
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: None,
                            content: None,
                            is_error: None,
                            cache_control: None,
                            thinking: None,
                        });
                    }
                    OpenAIContentBlock::ImageUrl { image_url } => {
                        // Convert OpenAI image_url to Anthropic ImageSource
                        let (mime_type, data) = if image_url.url.starts_with("data:") {
                            // Parse data URL
                            image_utils::parse_data_url(&image_url.url)?
                        } else {
                            // Fetch HTTP(S) URL and convert to base64
                            image_utils::fetch_image_as_base64(&image_url.url).await?
                        };

                        anthropic_blocks.push(AnthropicContentBlock {
                            block_type: "image".to_string(),
                            text: None,
                            source: Some(ImageSource::Base64 {
                                media_type: mime_type,
                                data,
                            }),
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: None,
                            content: None,
                            is_error: None,
                            cache_control: None,
                            thinking: None,
                        });
                    }
                    OpenAIContentBlock::ToolResult { tool_call_id, content } => {
                        // Convert OpenAI tool_result to Anthropic tool_result
                        anthropic_blocks.push(AnthropicContentBlock {
                            block_type: "tool_result".to_string(),
                            text: None,
                            source: None,
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: Some(tool_call_id.clone()),
                            content: Some(serde_json::Value::String(content.clone())),
                            is_error: None,
                            cache_control: None,
                            thinking: None,
                        });
                    }
                    OpenAIContentBlock::ToolUse { .. } => {
                        // ToolUse blocks should not appear in user messages
                        tracing::warn!("ToolUse block in message content, skipping");
                    }
                }
            }

            // If we have blocks, return them, otherwise error
            if !anthropic_blocks.is_empty() {
                Ok(AnthropicMessageContent::Blocks(anthropic_blocks))
            } else {
                Err(AppError::ConversionError(
                    "Message content is empty after conversion (all blocks were unsupported)".to_string()
                ))
            }
        }
    }
}

/// Extract system message from OpenAI messages
/// Returns (system_prompt, remaining_messages)
async fn extract_system_message(
    messages: &[ChatMessage],
) -> Result<(Option<AnthropicMessageContent>, Vec<ChatMessage>), AppError> {
    if let Some(first) = messages.first() {
        if first.role == "system" {
            let system = Some(convert_message_content(&first.content).await?);
            let rest = messages.iter().skip(1).cloned().collect();
            return Ok((system, rest));
        }
    }
    Ok((None, messages.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_extract_system_message() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: OpenAIMessageContent::Text("You are a helpful assistant.".to_string()),
                name: None,
                tool_calls: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: OpenAIMessageContent::Text("Hello!".to_string()),
                name: None,
                tool_calls: None,
            },
        ];

        let (system, remaining) = extract_system_message(&messages).await.unwrap();
        assert!(matches!(system, Some(AnthropicMessageContent::Text(_))));
        if let Some(AnthropicMessageContent::Text(text)) = system {
            assert_eq!(text, "You are a helpful assistant.");
        }
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].role, "user");
    }

    #[tokio::test]
    async fn test_extract_system_message_no_system() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: OpenAIMessageContent::Text("Hello!".to_string()),
            name: None,
            tool_calls: None,
        }];

        let (system, remaining) = extract_system_message(&messages).await.unwrap();
        assert_eq!(system, None);
        assert_eq!(remaining.len(), 1);
    }

    #[tokio::test]
    async fn test_convert_request_basic() {
        let openai_req = ChatCompletionRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: OpenAIMessageContent::Text("You are helpful.".to_string()),
                    name: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: OpenAIMessageContent::Text("Hi!".to_string()),
                    name: None,
                    tool_calls: None,
                },
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: Some(0.9),
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
        };

        let (anthropic_req, _warnings) = convert_request(&openai_req).await.unwrap();
        assert_eq!(anthropic_req.model, "claude-3-5-sonnet");
        assert!(matches!(
            anthropic_req.system,
            Some(AnthropicMessageContent::Text(_))
        ));
        if let Some(AnthropicMessageContent::Text(text)) = &anthropic_req.system {
            assert_eq!(text, "You are helpful.");
        }
        assert_eq!(anthropic_req.messages.len(), 1);
        assert_eq!(anthropic_req.messages[0].role, "user");
        assert_eq!(anthropic_req.max_tokens, 100);
        assert_eq!(anthropic_req.temperature, Some(0.7));
    }

    #[tokio::test]
    async fn test_convert_request_default_max_tokens() {
        let openai_req = ChatCompletionRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: OpenAIMessageContent::Text("Hi!".to_string()),
                name: None,
                tool_calls: None,
            }],
            max_tokens: None, // Not provided
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

        let (anthropic_req, _warnings) = convert_request(&openai_req).await.unwrap();
        assert_eq!(anthropic_req.max_tokens, 4096); // Default
    }

    #[tokio::test]
    async fn test_convert_request_clip_temperature() {
        let openai_req = ChatCompletionRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: OpenAIMessageContent::Text("Hi!".to_string()),
                name: None,
                tool_calls: None,
            }],
            max_tokens: Some(100),
            temperature: Some(1.5), // > 1.0, should be clipped
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

        let (anthropic_req, _warnings) = convert_request(&openai_req).await.unwrap();
        assert_eq!(anthropic_req.temperature, Some(1.0)); // Clipped to 1.0
    }
}
