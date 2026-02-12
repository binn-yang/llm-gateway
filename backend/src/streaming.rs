use crate::{
    converters,
    models::{
        anthropic::StreamEvent, gemini::GenerateContentResponse, openai::ChatCompletionChunk,
    },
};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{Stream, StreamExt};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};

/// Usage information extracted from streaming response
#[derive(Debug, Clone)]
pub struct StreamUsage {
    pub request_id: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

/// Streaming usage tracker with completion notification
///
/// This tracker supports both OpenAI-style (single chunk with complete usage)
/// and Anthropic-style (usage split across message_start and message_delta) streams.
///
/// # Performance
/// - Channel send: ~1μs (lock-free fast path)
/// - Channel receive: Zero CPU until notification
/// - No polling, no sleeping, no busy-waiting
#[derive(Clone)]
pub struct StreamingUsageTracker {
    request_id: String,
    /// Partial usage state (for Anthropic's split usage)
    input_tokens: Arc<Mutex<Option<u64>>>,
    output_tokens: Arc<Mutex<Option<u64>>>,
    /// Cache token tracking (for Anthropic prompt caching)
    cache_creation_input_tokens: Arc<Mutex<Option<u64>>>,
    cache_read_input_tokens: Arc<Mutex<Option<u64>>>,
    /// Completion notification: sender completes when stream ends
    completion_tx: Arc<tokio::sync::watch::Sender<bool>>,
    /// Accumulated chunks for body logging
    accumulated_chunks: Arc<Mutex<Vec<String>>>,
    max_chunks: usize,
    max_total_size: usize,
}

impl StreamingUsageTracker {
    /// Create new tracker
    pub fn new(request_id: String) -> Self {
        let (completion_tx, _) = tokio::sync::watch::channel(false);
        Self {
            request_id,
            input_tokens: Arc::new(Mutex::new(None)),
            output_tokens: Arc::new(Mutex::new(None)),
            cache_creation_input_tokens: Arc::new(Mutex::new(None)),
            cache_read_input_tokens: Arc::new(Mutex::new(None)),
            completion_tx: Arc::new(completion_tx),
            accumulated_chunks: Arc::new(Mutex::new(Vec::new())),
            max_chunks: 1000,
            max_total_size: 1_000_000, // 1MB
        }
    }

    /// Set OpenAI-style usage (single chunk with complete usage)
    pub fn set_usage(&self, prompt: u64, completion: u64) {
        *self.input_tokens.lock().unwrap() = Some(prompt);
        *self.output_tokens.lock().unwrap() = Some(completion);
        self.notify_complete();
    }

    /// Set Anthropic-style input tokens (from message_start)
    pub fn set_input_tokens(&self, tokens: u64) {
        *self.input_tokens.lock().unwrap() = Some(tokens);
        // Don't notify yet - still waiting for output_tokens
    }

    /// Set Anthropic-style output tokens (from message_delta)
    pub fn set_output_tokens(&self, tokens: u64) {
        *self.output_tokens.lock().unwrap() = Some(tokens);
        self.notify_complete();
    }

    /// Set Anthropic-style input usage with cache tokens (from message_start)
    ///
    /// This method sets all input-related tokens at once:
    /// - `input`: Regular input tokens
    /// - `cache_creation`: Tokens used to create prompt cache (optional)
    /// - `cache_read`: Tokens read from prompt cache (optional)
    pub fn set_input_usage(&self, input: u64, cache_creation: Option<u64>, cache_read: Option<u64>) {
        *self.input_tokens.lock().unwrap() = Some(input);
        *self.cache_creation_input_tokens.lock().unwrap() = cache_creation;
        *self.cache_read_input_tokens.lock().unwrap() = cache_read;
        // Don't notify yet - still waiting for output_tokens
    }

    /// Set all usage data from message_delta event (final values)
    ///
    /// This is the preferred method for extracting usage from Anthropic streams,
    /// as message_delta contains complete final usage data in all provider implementations
    /// (both official Anthropic API and compatible APIs like GLM).
    ///
    /// # Arguments
    /// - `input`: Input tokens (prompt tokens)
    /// - `output`: Output tokens (completion tokens)
    /// - `cache_creation`: Tokens used to create prompt cache (optional)
    /// - `cache_read`: Tokens read from prompt cache (optional)
    pub fn set_full_usage(
        &self,
        input: u64,
        output: u64,
        cache_creation: Option<u64>,
        cache_read: Option<u64>,
    ) {
        *self.input_tokens.lock().unwrap() = Some(input);
        *self.output_tokens.lock().unwrap() = Some(output);
        *self.cache_creation_input_tokens.lock().unwrap() = cache_creation;
        *self.cache_read_input_tokens.lock().unwrap() = cache_read;
        self.notify_complete();
    }

    /// Mark stream as complete (triggers callback)
    fn notify_complete(&self) {
        let _ = self.completion_tx.send(true);
    }

    /// Wait for stream completion and get final usage
    ///
    /// Returns `None` if usage never arrives (malformed response).
    ///
    /// # Returns
    /// `Some((input_tokens, output_tokens, cache_creation_tokens, cache_read_tokens))`
    /// where cache tokens default to 0 if not present.
    pub async fn wait_for_completion(&self) -> Option<(u64, u64, u64, u64)> {
        let mut rx = self.completion_tx.subscribe();

        // Wait for completion notification (no polling!)
        let timeout_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(300), // 5 minute timeout
            rx.wait_for(|&completed| completed)
        )
        .await;

        if timeout_result.is_err() {
            // Timeout - return None
            return None;
        }

        let input_lock = self.input_tokens.lock().ok()?;
        let output_lock = self.output_tokens.lock().ok()?;
        let cache_creation_lock = self.cache_creation_input_tokens.lock().ok()?;
        let cache_read_lock = self.cache_read_input_tokens.lock().ok()?;

        let input = *input_lock;
        let output = *output_lock;
        let cache_creation = *cache_creation_lock;
        let cache_read = *cache_read_lock;

        // Only input and output are required; cache tokens are optional
        if input.is_none() || output.is_none() {
            return None;
        }

        Some((
            input.unwrap(),
            output.unwrap(),
            cache_creation.unwrap_or(0),  // Default to 0 if not present
            cache_read.unwrap_or(0),      // Default to 0 if not present
        ))
    }

    /// Get request ID
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Accumulate a chunk for body logging (with size limits)
    pub fn accumulate_chunk(&self, chunk: &str) {
        let mut chunks = self.accumulated_chunks.lock().unwrap();

        // Check limits
        if chunks.len() < self.max_chunks {
            let total_size: usize = chunks.iter().map(|c| c.len()).sum();
            if total_size + chunk.len() < self.max_total_size {
                chunks.push(chunk.to_string());
            }
        }
    }

    /// Get accumulated response (all chunks joined)
    pub fn get_accumulated_response(&self) -> String {
        let chunks = self.accumulated_chunks.lock().unwrap();
        chunks.join("")
    }

    /// Get number of accumulated chunks
    pub fn chunks_count(&self) -> usize {
        let chunks = self.accumulated_chunks.lock().unwrap();
        chunks.len()
    }
}

/// Wrapper for OpenAI SSE stream that extracts usage information
pub fn create_openai_sse_stream_with_usage(
    response: reqwest::Response,
    request_id: String,
) -> (Sse<impl Stream<Item = Result<Event, Infallible>>>, Arc<Mutex<Option<StreamUsage>>>) {
    let usage_tracker = Arc::new(Mutex::new(None::<StreamUsage>));
    let usage_tracker_clone = usage_tracker.clone();

    let stream = response.bytes_stream().map(move |chunk_result| {
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

                        // Try to extract usage from this chunk
                        if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) {
                            if let Some(usage) = chunk.usage {
                                let mut tracker = usage_tracker_clone.lock().unwrap();
                                *tracker = Some(StreamUsage {
                                    request_id: request_id.clone(),
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                });
                            }
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

    (Sse::new(stream).keep_alive(KeepAlive::default()), usage_tracker)
}

/// Wrapper for OpenAI SSE stream with StreamingUsageTracker
///
/// This version uses the new tracker-based approach that eliminates hardcoded delays
/// and provides reliable completion notification via watch channel.
pub fn create_openai_sse_stream_with_tracker(
    response: reqwest::Response,
    tracker: StreamingUsageTracker,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = response.bytes_stream().map(move |chunk_result| {
        match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);

                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        // Accumulate chunk for body logging
                        tracker.accumulate_chunk(line);

                        if data == "[DONE]" {
                            return Ok(Event::default().data("[DONE]"));
                        }

                        // Parse OpenAI chunk and extract usage
                        if let Ok(chunk) = serde_json::from_str::<ChatCompletionChunk>(data) {
                            // Check for usage in this chunk
                            if let Some(usage) = chunk.usage {
                                tracker.set_usage(usage.prompt_tokens, usage.completion_tokens);
                            }
                        }

                        return Ok(Event::default().data(data.to_string()));
                    }
                }
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

/// Wrapper for Anthropic SSE stream with StreamingUsageTracker
///
/// This version extracts usage from Anthropic's split events:
/// - `message_start`: contains input_tokens
/// - `message_delta`: contains output_tokens
pub fn create_anthropic_sse_stream_with_tracker(
    response: reqwest::Response,
    request_id: String,
    tracker: StreamingUsageTracker,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let tracker = tracker.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut events = Vec::new();

                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(anthropic_event) = serde_json::from_str::<StreamEvent>(data) {
                            // EXTRACT USAGE - Only from message_delta (unified approach)
                            match anthropic_event.event_type.as_str() {
                                "message_start" => {
                                    // message_start no longer extracts tokens
                                    // Only used to mark stream start
                                    tracing::debug!(
                                        request_id = %request_id,
                                        "Received message_start event (token extraction happens in message_delta)"
                                    );
                                }
                                "message_delta" => {
                                    // Log raw JSON payload for debugging
                                    tracing::debug!(
                                        request_id = %request_id,
                                        raw_json = %data,
                                        "Raw message_delta event payload (OpenAI API)"
                                    );

                                    // Extract ALL tokens from message_delta (final values)
                                    if let Some(usage) = &anthropic_event.usage {
                                        tracing::debug!(
                                            request_id = %request_id,
                                            input_tokens = usage.input_tokens,
                                            output_tokens = usage.output_tokens,
                                            cache_creation = ?usage.cache_creation_input_tokens,
                                            cache_read = ?usage.cache_read_input_tokens,
                                            "Extracted all tokens from message_delta (OpenAI API)"
                                        );
                                        tracker.set_full_usage(
                                            usage.input_tokens,
                                            usage.output_tokens,
                                            usage.cache_creation_input_tokens,
                                            usage.cache_read_input_tokens,
                                        );
                                    } else {
                                        tracing::warn!(
                                            request_id = %request_id,
                                            "message_delta has no usage data"
                                        );
                                    }
                                }
                                _ => {}
                            }

                            // Convert to OpenAI format
                            if let Some(openai_chunk) =
                                converters::anthropic_response::convert_stream_event(
                                    &anthropic_event,
                                    &request_id,
                                )
                            {
                                if let Ok(json) = serde_json::to_string(&openai_chunk) {
                                    events.push(Ok(Event::default().data(json)));
                                }
                            }

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

/// Convert Gemini SSE stream to OpenAI SSE stream
/// Gemini sends JSON objects in SSE format, each containing incremental content
pub fn create_gemini_sse_stream(
    response: reqwest::Response,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let request_id = uuid::Uuid::new_v4().to_string();
    let request_id_clone = request_id.clone();

    // Shared state to track if this is the first chunk (for sending role)
    let is_first_chunk = std::sync::Arc::new(std::sync::Mutex::new(true));

    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let request_id = request_id_clone.clone();
        let is_first_chunk = is_first_chunk.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut events = Vec::new();

                // Parse SSE events (Gemini format: "data: {...}\n\n")
                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        // Try to parse as Gemini chunk
                        if let Ok(gemini_chunk) =
                            serde_json::from_str::<GenerateContentResponse>(data)
                        {
                            // Convert to OpenAI chunk
                            let mut is_first = is_first_chunk.lock().unwrap();
                            match converters::gemini_streaming::convert_streaming_chunk(
                                &gemini_chunk,
                                &request_id,
                                &mut is_first,
                            ) {
                                Ok(Some(openai_chunk)) => {
                                    // Serialize to JSON
                                    if let Ok(json) = serde_json::to_string(&openai_chunk) {
                                        events.push(Ok(Event::default().data(json)));
                                    }

                                    // Check for finish_reason to send [DONE]
                                    if openai_chunk.choices[0].finish_reason.is_some() {
                                        events.push(Ok(Event::default().data("[DONE]")));
                                    }
                                }
                                Ok(None) => {
                                    // Empty chunk, skip
                                }
                                Err(e) => {
                                    tracing::error!("Failed to convert Gemini chunk: {}", e);
                                }
                            }
                        } else {
                            tracing::warn!("Failed to parse Gemini chunk: {}", data);
                        }
                    }
                }

                events
            }
            Err(e) => {
                tracing::error!("Gemini stream error: {}", e);
                vec![Ok(Event::default().data(""))]
            }
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Wrapper for Gemini SSE stream with StreamingUsageTracker
///
/// This version extracts usage from Gemini's usage_metadata field,
/// which appears only in the last chunk.
pub fn create_gemini_sse_stream_with_tracker(
    response: reqwest::Response,
    request_id: String,
    tracker: StreamingUsageTracker,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let is_first_chunk = Arc::new(Mutex::new(true));

    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let tracker = tracker.clone();
        let is_first_chunk = is_first_chunk.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut events = Vec::new();

                for line in text.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(gemini_chunk) =
                            serde_json::from_str::<GenerateContentResponse>(data)
                        {
                            // EXTRACT USAGE - Check for usage_metadata
                            if let Some(usage_metadata) = &gemini_chunk.usage_metadata {
                                tracker.set_usage(
                                    usage_metadata.prompt_token_count,
                                    usage_metadata.candidates_token_count,
                                );
                            }

                            // Convert to OpenAI chunk
                            let mut is_first = is_first_chunk.lock().unwrap();
                            match converters::gemini_streaming::convert_streaming_chunk(
                                &gemini_chunk,
                                &request_id,
                                &mut is_first,
                            ) {
                                Ok(Some(openai_chunk)) => {
                                    if let Ok(json) = serde_json::to_string(&openai_chunk) {
                                        events.push(Ok(Event::default().data(json)));
                                    }

                                    if openai_chunk.choices[0].finish_reason.is_some() {
                                        events.push(Ok(Event::default().data("[DONE]")));
                                    }
                                }
                                Ok(None) => {
                                    // Empty chunk, skip
                                }
                                Err(e) => {
                                    tracing::error!("Failed to convert Gemini chunk: {}", e);
                                }
                            }
                        } else {
                            tracing::warn!("Failed to parse Gemini chunk: {}", data);
                        }
                    }
                }
                events
            }
            Err(e) => {
                tracing::error!("Gemini stream error: {}", e);
                vec![Ok(Event::default().data(""))]
            }
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// 创建原生 Gemini SSE 流（透传模式）
/// 不做协议转换，直接转发 Gemini SSE 事件
///
/// 此函数从 SSE 流中提取 usage_metadata（在最后一个 chunk 中）
/// 并透传所有数据到客户端。
pub fn create_native_gemini_sse_stream_with_tracker(
    response: reqwest::Response,
    tracker: StreamingUsageTracker,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use std::sync::{Arc, Mutex};

    let buffer = Arc::new(Mutex::new(String::new()));

    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let buffer = buffer.clone();
        let tracker = tracker.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let chunk_text = String::from_utf8_lossy(&bytes).to_string();
                let mut events = Vec::new();

                // Append to buffer
                let mut buf = buffer.lock().unwrap();
                buf.push_str(&chunk_text);

                // Process complete SSE events (terminated by double newline)
                while let Some(event_end) = buf.find("\n\n") {
                    let event_text = buf[..event_end].to_string();
                    *buf = buf[event_end + 2..].to_string(); // +2 to skip "\n\n"

                    // Accumulate chunk for body logging
                    tracker.accumulate_chunk(&event_text);

                    // Parse Gemini SSE format: "data: {...}\n\n"
                    for line in event_text.lines() {
                        if let Some(data) = line.strip_prefix("data: ") {
                            // 尝试提取 usage_metadata（通常在最后一个 chunk）
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                // 检查是否包含 usage_metadata（通常在最后一个 chunk）
                                if let Some(usage) = json.get("usageMetadata") {
                                    let prompt_tokens = usage.get("promptTokenCount").and_then(|v| v.as_u64());
                                    let candidates_tokens = usage.get("candidatesTokenCount").and_then(|v| v.as_u64());

                                    // 检查两个值是否都存在
                                    if let (Some(prompt), Some(candidates)) = (prompt_tokens, candidates_tokens) {
                                        // 提取到完整的 token 计数
                                        tracker.set_usage(prompt, candidates);
                                    }
                                }
                            }

                            // 透传原始数据
                            events.push(Ok(Event::default().data(data.to_string())));
                        }
                    }
                }

                drop(buf); // Release lock before returning
                events
            }
            Err(e) => {
                tracing::error!("Native Gemini stream error: {}", e);
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
                    tool_calls: None,
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
                    tool_calls: None,
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
    use std::sync::{Arc, Mutex};

    // Shared buffer for handling chunks that span SSE event boundaries
    let buffer = Arc::new(Mutex::new(String::new()));

    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let buffer = buffer.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let chunk_text = String::from_utf8_lossy(&bytes).to_string();
                let mut events = Vec::new();

                // Append to buffer
                let mut buf = buffer.lock().unwrap();
                buf.push_str(&chunk_text);

                // Process complete SSE events (terminated by double newline)
                while let Some(event_end) = buf.find("\n\n") {
                    let event_text = buf[..event_end].to_string();
                    *buf = buf[event_end + 2..].to_string(); // +2 to skip "\n\n"

                    // Parse this complete SSE event
                    let mut current_event_type: Option<String> = None;
                    let mut current_data_lines: Vec<String> = Vec::new();

                    for line in event_text.lines() {
                        if let Some(event_name) = line.strip_prefix("event: ") {
                            current_event_type = Some(event_name.trim().to_string());
                        } else if let Some(data) = line.strip_prefix("data: ") {
                            current_data_lines.push(data.to_string());
                        }
                    }

                    // Build the SSE event
                    if !current_data_lines.is_empty() {
                        let data = current_data_lines.join("\n");
                        let mut event = Event::default().data(data);

                        if let Some(event_type) = current_event_type {
                            event = event.event(event_type);
                        }

                        events.push(Ok(event));
                    }
                }

                drop(buf); // Release lock before returning
                events
            }
            Err(e) => {
                tracing::error!("Native Anthropic stream error: {}", e);
                vec![]
            }
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Wrapper for native Anthropic SSE stream with StreamingUsageTracker
///
/// This version extracts usage from Anthropic's split events (same as via OpenAI API):
/// - `message_start`: contains input_tokens
/// - `message_delta`: contains output_tokens
pub fn create_native_anthropic_sse_stream_with_tracker(
    response: reqwest::Response,
    tracker: StreamingUsageTracker,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let buffer = Arc::new(Mutex::new(String::new()));

    let stream = response.bytes_stream().flat_map(move |chunk_result| {
        let buffer = buffer.clone();
        let tracker = tracker.clone();

        futures::stream::iter(match chunk_result {
            Ok(bytes) => {
                let chunk_text = String::from_utf8_lossy(&bytes).to_string();
                let mut events = Vec::new();

                let mut buf = buffer.lock().unwrap();
                buf.push_str(&chunk_text);

                // Process complete SSE events (terminated by double newline)
                while let Some(event_end) = buf.find("\n\n") {
                    let event_text = buf[..event_end].to_string();
                    *buf = buf[event_end + 2..].to_string(); // +2 to skip "\n\n"

                    // Accumulate chunk for body logging
                    tracker.accumulate_chunk(&event_text);

                    // Parse this complete SSE event
                    let mut current_event_type: Option<String> = None;
                    let mut current_data_lines: Vec<String> = Vec::new();

                    for line in event_text.lines() {
                        if let Some(event_name) = line.strip_prefix("event: ") {
                            current_event_type = Some(event_name.trim().to_string());
                        } else if let Some(data) = line.strip_prefix("data: ") {
                            current_data_lines.push(data.to_string());
                        }
                    }

                    // EXTRACT USAGE from native Anthropic events
                    if let Some(event_type) = &current_event_type {
                        if let Some(data) = current_data_lines.first() {
                            tracing::debug!(
                                request_id = %tracker.request_id(),
                                event_type = %event_type,
                                data_len = data.len(),
                                "Processing SSE event"
                            );

                            // Log raw JSON for message_delta events BEFORE parsing
                            if event_type == "message_delta" {
                                tracing::debug!(
                                    request_id = %tracker.request_id(),
                                    raw_json = %data,
                                    "Raw message_delta JSON (before parsing)"
                                );
                            }

                            match serde_json::from_str::<StreamEvent>(data) {
                                Ok(anthropic_event) => match event_type.as_str() {
                                    "message_start" => {
                                        // message_start no longer extracts tokens
                                        // Only used to mark stream start
                                        tracing::debug!(
                                            request_id = %tracker.request_id(),
                                            "Received message_start event (token extraction happens in message_delta)"
                                        );
                                    }
                                    "message_delta" => {
                                        // Log raw JSON payload for debugging
                                        tracing::debug!(
                                            request_id = %tracker.request_id(),
                                            raw_json = %data,
                                            "Raw message_delta event payload (native API)"
                                        );

                                        // Extract ALL tokens from message_delta (final values)
                                        if let Some(usage) = &anthropic_event.usage {
                                            tracing::debug!(
                                                request_id = %tracker.request_id(),
                                                input_tokens = usage.input_tokens,
                                                output_tokens = usage.output_tokens,
                                                cache_creation = ?usage.cache_creation_input_tokens,
                                                cache_read = ?usage.cache_read_input_tokens,
                                                "Extracted all tokens from message_delta (native API)"
                                            );
                                            tracker.set_full_usage(
                                                usage.input_tokens,
                                                usage.output_tokens,
                                                usage.cache_creation_input_tokens,
                                                usage.cache_read_input_tokens,
                                            );
                                        } else {
                                            tracing::warn!(
                                                request_id = %tracker.request_id(),
                                                "message_delta has no usage data"
                                            );
                                        }
                                    }
                                    _ => {}
                                },
                                Err(e) => {
                                    tracing::warn!(
                                        request_id = %tracker.request_id(),
                                        event_type = %event_type,
                                        error = %e,
                                        raw_data = %data,
                                        "Failed to parse StreamEvent JSON"
                                    );
                                }
                            }
                        }
                    }

                    // Build the SSE event
                    if !current_data_lines.is_empty() {
                        let data = current_data_lines.join("\n");
                        let mut event = Event::default().data(data);

                        if let Some(event_type) = current_event_type {
                            event = event.event(event_type);
                        }

                        events.push(Ok(event));
                    }
                }

                drop(buf); // Release lock before returning
                events
            }
            Err(e) => {
                tracing::error!("Native Anthropic stream error: {}", e);
                vec![Ok(Event::default().data(""))]
            }
        })
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
