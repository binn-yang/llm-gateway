use crate::{
    converters,
    models::{anthropic::StreamEvent, openai::ChatCompletionChunk},
};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;

/// Convert a reqwest response stream to an SSE stream for OpenAI format
pub fn create_openai_sse_stream(
    response: reqwest::Response,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = response.bytes_stream().map(|chunk_result| {
        match chunk_result {
            Ok(bytes) => {
                // Parse the SSE data
                let text = String::from_utf8_lossy(&bytes);

                // SSE format: "data: {...}\n\n"
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data == "[DONE]" {
                            // End of stream marker
                            return Ok(Event::default().data("[DONE]"));
                        }

                        // Forward the JSON data
                        return Ok(Event::default().data(data.to_string()));
                    }
                }

                // If no valid data found, send empty event
                Ok(Event::default().data(""))
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                Ok(Event::default().data(""))
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Parse usage information from the last chunk
pub fn extract_usage_from_chunk(chunk_json: &str) -> Option<(u64, u64)> {
    if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(chunk_json) {
        if let Some(usage) = chunk.usage {
            return Some((usage.prompt_tokens, usage.completion_tokens));
        }
    }
    None
}

/// Convert Anthropic SSE stream to OpenAI SSE stream
pub fn create_anthropic_sse_stream(
    response: reqwest::Response,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let request_id_clone = request_id.clone();

    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let request_id = request_id_clone.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut events = Vec::new();

                // Parse SSE events
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        // Try to parse as Anthropic event
                        if let Ok(anthropic_event) = serde_json::from_str::<StreamEvent>(data) {
                            // Convert to OpenAI chunk
                            if let Some(openai_chunk) =
                                converters::anthropic_response::convert_stream_event(
                                    &anthropic_event,
                                    &request_id,
                                )
                            {
                                // Serialize to JSON
                                if let Ok(json) = serde_json::to_string(&openai_chunk) {
                                    events.push(Ok(Event::default().data(json)));
                                }
                            }

                            // Check for end of stream
                            if anthropic_event.event_type == "message_stop" {
                                events.push(Ok(Event::default().data("[DONE]")));
                            }
                        }
                    }
                }

                events
            }
            Err(e) => {
                tracing::error!("Stream error: {}", e);
                vec![Ok(Event::default().data(""))]
            }
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::openai::{ChatCompletionChunk, ChunkChoice, Delta, Usage};

    #[test]
    fn test_extract_usage_from_chunk() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1677652288,
            model: "gpt-4".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Some(Usage {
                prompt_tokens: 10,
                completion_tokens: 20,
                total_tokens: 30,
            }),
        };

        let json = serde_json::to_string(&chunk).unwrap();
        let (input, output) = extract_usage_from_chunk(&json).unwrap();
        assert_eq!(input, 10);
        assert_eq!(output, 20);
    }

    #[test]
    fn test_extract_usage_from_chunk_without_usage() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1677652288,
            model: "gpt-4".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".to_string()),
                    content: Some("Hello".to_string()),
                },
                finish_reason: None,
            }],
            usage: None,
        };

        let json = serde_json::to_string(&chunk).unwrap();
        let result = extract_usage_from_chunk(&json);
        assert!(result.is_none());
    }
}

/// 创建原生 Anthropic SSE 流（无协议转换）
/// 直接转发 Anthropic 的 SSE 事件，保持原生格式
pub fn create_native_anthropic_sse_stream(
    response: reqwest::Response,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = response.bytes_stream().map(|chunk_result| {
        match chunk_result {
            Ok(bytes) => {
                // 直接转发原始 SSE 数据
                let text = String::from_utf8_lossy(&bytes);

                // SSE 格式：每行可能是 "event: xxx" 或 "data: {...}"
                // 直接转发整个块，保持 Anthropic 原生格式
                if !text.is_empty() {
                    Ok(Event::default().data(text.to_string()))
                } else {
                    Ok(Event::default().data(""))
                }
            }
            Err(e) => {
                tracing::error!("Native Anthropic stream error: {}", e);
                Ok(Event::default().data(""))
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
