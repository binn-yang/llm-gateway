use crate::{
    error::AppError,
    models::{
        anthropic::{MessagesResponse, StreamEvent},
        openai::{
            ChatChoice, ChatCompletionChunk, ChatCompletionResponse, ChatMessage, ChunkChoice,
            Delta, FunctionCall, ToolCall, Usage,
        },
    },
};

/// Convert Anthropic MessagesResponse to OpenAI ChatCompletionResponse
pub fn convert_response(
    anthropic_resp: &MessagesResponse,
) -> Result<ChatCompletionResponse, AppError> {
    // Iterate through content blocks to extract text and tool calls
    let mut text_content = String::new();
    let mut tool_calls = Vec::new();

    for block in &anthropic_resp.content {
        match block.block_type.as_str() {
            "text" => {
                if let Some(text) = &block.text {
                    text_content.push_str(text);
                }
            }
            "tool_use" => {
                // Convert to OpenAI ToolCall format
                let tool_call = ToolCall {
                    id: block.id.clone().unwrap_or_default(),
                    tool_type: "function".to_string(),
                    function: FunctionCall {
                        name: block.name.clone().unwrap_or_default(),
                        arguments: serde_json::to_string(&block.input)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                };
                tool_calls.push(tool_call);
            }
            _ => {
                // Ignore other block types (image, tool_result, etc.)
            }
        }
    }

    Ok(ChatCompletionResponse {
        id: anthropic_resp.id.clone(),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: anthropic_resp.model.clone(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: crate::models::openai::MessageContent::Text(text_content),
                name: None,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
            },
            finish_reason: anthropic_resp.stop_reason.clone(),
            logprobs: None, // Anthropic doesn't provide log probabilities
        }],
        usage: Some(Usage {
            prompt_tokens: anthropic_resp.usage.input_tokens,
            completion_tokens: anthropic_resp.usage.output_tokens,
            total_tokens: anthropic_resp.usage.input_tokens
                + anthropic_resp.usage.output_tokens,
        }),
    })
}

/// Convert Anthropic streaming event to OpenAI chunk
/// Returns None for events that don't map to OpenAI chunks
pub fn convert_stream_event(event: &StreamEvent, request_id: &str) -> Option<ChatCompletionChunk> {
    match event.event_type.as_str() {
        "message_start" => {
            // First chunk with role
            Some(ChatCompletionChunk {
                id: request_id.to_string(),
                object: "chat.completion.chunk".to_string(),
                created: chrono::Utc::now().timestamp() as u64,
                model: event
                    .message
                    .as_ref()
                    .and_then(|m| m.model.clone())
                    .unwrap_or_default(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: Some("assistant".to_string()),
                        content: None,
                        tool_calls: None,
                    },
                    finish_reason: None,
                }],
                usage: None,
            })
        }
        "content_block_start" => {
            // Check if this is a tool use block
            if let Some(content_block) = &event.content_block {
                if content_block.block_type == "tool_use" {
                    // Start of a tool call
                    let index = event.index.unwrap_or(0);
                    return Some(ChatCompletionChunk {
                        id: request_id.to_string(),
                        object: "chat.completion.chunk".to_string(),
                        created: chrono::Utc::now().timestamp() as u64,
                        model: String::new(),
                        choices: vec![ChunkChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: None,
                                tool_calls: Some(vec![crate::models::openai::ToolCallDelta {
                                    index,
                                    id: content_block.id.clone(),
                                    tool_type: Some("function".to_string()),
                                    function: Some(crate::models::openai::FunctionCallDelta {
                                        name: content_block.name.clone(),
                                        arguments: None,
                                    }),
                                }]),
                            },
                            finish_reason: None,
                        }],
                        usage: None,
                    });
                }
            }
            None
        }
        "content_block_delta" => {
            if let Some(delta) = &event.delta {
                // Check if this is text delta or tool input delta
                match delta.delta_type.as_deref() {
                    Some("text_delta") => {
                        // Text content delta
                        if let Some(text) = &delta.text {
                            return Some(ChatCompletionChunk {
                                id: request_id.to_string(),
                                object: "chat.completion.chunk".to_string(),
                                created: chrono::Utc::now().timestamp() as u64,
                                model: String::new(),
                                choices: vec![ChunkChoice {
                                    index: 0,
                                    delta: Delta {
                                        role: None,
                                        content: Some(text.clone()),
                                        tool_calls: None,
                                    },
                                    finish_reason: None,
                                }],
                                usage: None,
                            });
                        }
                    }
                    Some("input_json_delta") => {
                        // Tool input JSON delta
                        if let Some(partial_json) = &delta.partial_json {
                            let index = event.index.unwrap_or(0);
                            return Some(ChatCompletionChunk {
                                id: request_id.to_string(),
                                object: "chat.completion.chunk".to_string(),
                                created: chrono::Utc::now().timestamp() as u64,
                                model: String::new(),
                                choices: vec![ChunkChoice {
                                    index: 0,
                                    delta: Delta {
                                        role: None,
                                        content: None,
                                        tool_calls: Some(vec![crate::models::openai::ToolCallDelta {
                                            index,
                                            id: None,
                                            tool_type: None,
                                            function: Some(crate::models::openai::FunctionCallDelta {
                                                name: None,
                                                arguments: Some(partial_json.clone()),
                                            }),
                                        }]),
                                    },
                                    finish_reason: None,
                                }],
                                usage: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        "message_delta" => {
            // Final chunk with usage and finish reason
            let finish_reason = event.delta.as_ref().and_then(|d| d.stop_reason.clone());
            let usage = event.usage.as_ref().map(|u| Usage {
                prompt_tokens: u.input_tokens,
                completion_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            });

            Some(ChatCompletionChunk {
                id: request_id.to_string(),
                object: "chat.completion.chunk".to_string(),
                created: chrono::Utc::now().timestamp() as u64,
                model: String::new(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: None,
                        tool_calls: None,
                    },
                    finish_reason,
                }],
                usage,
            })
        }
        _ => None, // Ignore other event types
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::anthropic::{ContentBlock, TokenUsage};

    #[test]
    fn test_convert_response() {
        let anthropic_resp = MessagesResponse {
            id: "msg_123".to_string(),
            response_type: "message".to_string(),
            role: "assistant".to_string(),
            content: vec![ContentBlock {
                block_type: "text".to_string(),
                text: Some("Hello! How can I help you?".to_string()),
                source: None,
                id: None,
                name: None,
                input: None,
                tool_use_id: None,
                content: None,
                is_error: None,
                cache_control: None,
                thinking: None,
            }],
            model: "claude-3-5-sonnet-20241022".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 25,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
        };

        let openai_resp = convert_response(&anthropic_resp).unwrap();
        assert_eq!(openai_resp.id, "msg_123");
        assert_eq!(openai_resp.object, "chat.completion");
        assert_eq!(openai_resp.model, "claude-3-5-sonnet-20241022");
        assert_eq!(openai_resp.choices[0].message.role, "assistant");
        assert_eq!(
            openai_resp.choices[0].message.content.extract_text(),
            "Hello! How can I help you?"
        );
        assert_eq!(openai_resp.choices[0].finish_reason, Some("end_turn".to_string()));
        assert_eq!(openai_resp.usage.as_ref().unwrap().prompt_tokens, 10);
        assert_eq!(openai_resp.usage.as_ref().unwrap().completion_tokens, 25);
        assert_eq!(openai_resp.usage.as_ref().unwrap().total_tokens, 35);
    }

    #[test]
    fn test_convert_stream_event_message_start() {
        use crate::models::anthropic::MessageData;

        let event = StreamEvent {
            event_type: "message_start".to_string(),
            message: Some(MessageData {
                id: "msg_123".to_string(),
                message_type: "message".to_string(),
                role: "assistant".to_string(),
                model: Some("claude-3-5-sonnet-20241022".to_string()),
                usage: None,
            }),
            index: None,
            content_block: None,
            delta: None,
            usage: None,
        };

        let chunk = convert_stream_event(&event, "chatcmpl-123").unwrap();
        assert_eq!(chunk.id, "chatcmpl-123");
        assert_eq!(chunk.object, "chat.completion.chunk");
        assert_eq!(chunk.choices[0].delta.role, Some("assistant".to_string()));
        assert_eq!(chunk.choices[0].delta.content, None);
    }

    #[test]
    fn test_convert_stream_event_content_delta() {
        use crate::models::anthropic::Delta as AnthropicDelta;

        let event = StreamEvent {
            event_type: "content_block_delta".to_string(),
            message: None,
            index: Some(0),
            content_block: None,
            delta: Some(AnthropicDelta {
                delta_type: Some("text_delta".to_string()),
                text: Some("Hello".to_string()),
                stop_reason: None,
                partial_json: None,
            }),
            usage: None,
        };

        let chunk = convert_stream_event(&event, "chatcmpl-123").unwrap();
        assert_eq!(chunk.choices[0].delta.content, Some("Hello".to_string()));
        assert_eq!(chunk.choices[0].delta.role, None);
    }

    #[test]
    fn test_convert_stream_event_message_delta() {
        use crate::models::anthropic::Delta as AnthropicDelta;

        let event = StreamEvent {
            event_type: "message_delta".to_string(),
            message: None,
            index: None,
            content_block: None,
            delta: Some(AnthropicDelta {
                delta_type: Some("message_delta".to_string()),
                text: None,
                stop_reason: Some("end_turn".to_string()),
                partial_json: None,
            }),
            usage: Some(TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            }),
        };

        let chunk = convert_stream_event(&event, "chatcmpl-123").unwrap();
        assert_eq!(chunk.choices[0].finish_reason, Some("end_turn".to_string()));
        assert_eq!(chunk.usage.as_ref().unwrap().prompt_tokens, 10);
        assert_eq!(chunk.usage.as_ref().unwrap().completion_tokens, 20);
    }
}
