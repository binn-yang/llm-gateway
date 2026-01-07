use crate::{
    error::AppError,
    models::{
        anthropic::{MessagesResponse, StreamEvent},
        openai::{
            ChatChoice, ChatCompletionChunk, ChatCompletionResponse, ChatMessage, ChunkChoice,
            Delta, Usage,
        },
    },
};

/// Convert Anthropic MessagesResponse to OpenAI ChatCompletionResponse
pub fn convert_response(anthropic_resp: &MessagesResponse) -> Result<ChatCompletionResponse, AppError> {
    // Extract text content from first content block
    let content = anthropic_resp
        .content
        .first()
        .and_then(|block| block.text.clone())
        .unwrap_or_default();

    Ok(ChatCompletionResponse {
        id: anthropic_resp.id.clone(),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: anthropic_resp.model.clone(),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content,
                name: None,
            },
            finish_reason: anthropic_resp.stop_reason.clone(),
        }],
        usage: Some(Usage {
            prompt_tokens: anthropic_resp.usage.input_tokens,
            completion_tokens: anthropic_resp.usage.output_tokens,
            total_tokens: anthropic_resp.usage.input_tokens + anthropic_resp.usage.output_tokens,
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
                    },
                    finish_reason: None,
                }],
                usage: None,
            })
        }
        "content_block_delta" => {
            // Content delta
            let text = event.delta.as_ref().and_then(|d| d.text.clone());
            if text.is_some() {
                Some(ChatCompletionChunk {
                    id: request_id.to_string(),
                    object: "chat.completion.chunk".to_string(),
                    created: chrono::Utc::now().timestamp() as u64,
                    model: String::new(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: Delta {
                            role: None,
                            content: text,
                        },
                        finish_reason: None,
                    }],
                    usage: None,
                })
            } else {
                None
            }
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
                text: "Hello! How can I help you?".to_string(),
            }],
            model: "claude-3-5-sonnet-20241022".to_string(),
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: TokenUsage {
                input_tokens: 10,
                output_tokens: 25,
            },
        };

        let openai_resp = convert_response(&anthropic_resp).unwrap();
        assert_eq!(openai_resp.id, "msg_123");
        assert_eq!(openai_resp.object, "chat.completion");
        assert_eq!(openai_resp.model, "claude-3-5-sonnet-20241022");
        assert_eq!(openai_resp.choices[0].message.role, "assistant");
        assert_eq!(
            openai_resp.choices[0].message.content,
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
                delta_type: "text_delta".to_string(),
                text: Some("Hello".to_string()),
                stop_reason: None,
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
                delta_type: "message_delta".to_string(),
                text: None,
                stop_reason: Some("end_turn".to_string()),
            }),
            usage: Some(TokenUsage {
                input_tokens: 10,
                output_tokens: 20,
            }),
        };

        let chunk = convert_stream_event(&event, "chatcmpl-123").unwrap();
        assert_eq!(chunk.choices[0].finish_reason, Some("end_turn".to_string()));
        assert_eq!(chunk.usage.as_ref().unwrap().prompt_tokens, 10);
        assert_eq!(chunk.usage.as_ref().unwrap().completion_tokens, 20);
    }
}
