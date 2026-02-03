//! 安全的日志记录工具
//!
//! 提供敏感信息脱敏功能，确保日志中不会泄露 API keys 等敏感信息。

use regex::Regex;
use std::fmt;

use crate::config::RedactPattern;

/// 脱敏后的 API key 表示
///
/// 只显示前 8 个字符，其余替换为 `***`，用于安全地记录日志
#[derive(Clone, Debug)]
pub struct SensitiveApiKey<'a> {
    inner: &'a str,
}

impl<'a> SensitiveApiKey<'a> {
    /// 创建脱敏的 API key 表示
    ///
    /// # 示例
    /// ```
    /// use llm_gateway::logging::SensitiveApiKey;
    ///
    /// let key = "sk-ant-api123-abcdef123456";
    /// let sanitized = SensitiveApiKey::new(key);
    /// assert_eq!(format!("{}", sanitized), "sk-ant-***");
    /// ```
    pub fn new(key: &'a str) -> Self {
        Self { inner: key }
    }
}

impl<'a> fmt::Display for SensitiveApiKey<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let visible_len = 8.min(self.inner.len());
        if self.inner.len() <= visible_len {
            // 如果 key 太短，全部脱敏
            write!(f, "***")
        } else {
            write!(f, "{}***", &self.inner[..visible_len])
        }
    }
}

/// 检查字符串是否可能是敏感的 API key
///
/// 如果字符串看起来像 API key（以 sk-、pk- 等开头），返回 true
pub fn is_sensitive_key(value: &str) -> bool {
    let sensitive_prefixes = [
        "sk-ant-",
        "sk-",
        "pk-",
        "sess-",
        "acct-",
        "Bearer sk-",
        "Bearer pk-",
    ];

    for prefix in &sensitive_prefixes {
        if value.starts_with(prefix) {
            return true;
        }
    }

    false
}

/// 对字符串进行脱敏处理（如果包含敏感信息）
///
/// # 示例
/// ```
/// use llm_gateway::logging::{sanitize_log_value, SensitiveApiKey};
///
/// // 敏感值会被脱敏
/// assert_eq!(sanitize_log_value("sk-ant-api123-key"), "sk-ant-***");
///
/// // 普通值不变
/// assert_eq!(sanitize_log_value("my-app-name"), "my-app-name");
/// ```
pub fn sanitize_log_value(value: &str) -> String {
    if is_sensitive_key(value) {
        SensitiveApiKey::new(value).to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_api_key_display() {
        let key = "sk-ant-api123-abcdef123456";
        let sanitized = SensitiveApiKey::new(key);
        // 显示前 8 个字符 + ***
        assert_eq!(format!("{}", sanitized), "sk-ant-a***");
    }

    #[test]
    fn test_sensitive_api_key_short() {
        let key = "sk-abc";
        let sanitized = SensitiveApiKey::new(key);
        assert_eq!(format!("{}", sanitized), "***");
    }

    #[test]
    fn test_is_sensitive_key() {
        // 检测各种 API key 格式
        assert!(is_sensitive_key("sk-ant-api123"));
        assert!(is_sensitive_key("sk-openai123"));
        assert!(is_sensitive_key("pk-test123"));
        assert!(is_sensitive_key("sess-abc123"));
        assert!(is_sensitive_key("Bearer sk-ant-api123"));

        // 普通字符串不是敏感的
        assert!(!is_sensitive_key("my-app-name"));
        assert!(!is_sensitive_key("test-key"));
        assert!(!is_sensitive_key("provider-name"));
    }

    #[test]
    fn test_sanitize_log_value() {
        // 敏感值会被脱敏
        assert_eq!(
            sanitize_log_value("sk-ant-api123-abcdef"),
            "sk-ant-a***"
        );
        assert_eq!(
            sanitize_log_value("sk-openai123"),
            "sk-opena***"
        );

        // 普通值不变
        assert_eq!(sanitize_log_value("my-app"), "my-app");
        assert_eq!(sanitize_log_value("test-provider"), "test-provider");
    }
}

/// Redact sensitive data in body content using configured patterns
///
/// # Arguments
/// * `body` - The body content to redact
/// * `patterns` - List of redaction patterns to apply
///
/// # Returns
/// Redacted body content with sensitive data replaced
pub fn redact_sensitive_data(body: &str, patterns: &[RedactPattern]) -> String {
    let mut redacted = body.to_string();

    for pattern in patterns {
        // Compile regex on-the-fly (consider caching in production)
        if let Ok(regex) = Regex::new(&pattern.pattern) {
            redacted = regex.replace_all(&redacted, &pattern.replacement).to_string();
        }
    }

    redacted
}

/// Truncate body content if it exceeds max size
///
/// # Arguments
/// * `body` - The body content to truncate
/// * `max_size` - Maximum size in bytes
///
/// # Returns
/// Tuple of (truncated_body, was_truncated)
pub fn truncate_body(body: String, max_size: usize) -> (String, bool) {
    if body.len() > max_size {
        (body[..max_size].to_string(), true)
    } else {
        (body, false)
    }
}

/// Extract simple conversation from Anthropic MessagesRequest
/// Returns JSON: {"user_messages": ["text1", "text2", ...]}
pub fn extract_simple_request_anthropic(request: &crate::models::anthropic::MessagesRequest) -> String {
    use crate::models::anthropic::MessageContent;

    let mut user_messages = Vec::new();

    for message in &request.messages {
        if message.role == "user" {
            match &message.content {
                MessageContent::Text(text) => {
                    user_messages.push(text.clone());
                }
                MessageContent::Blocks(blocks) => {
                    for block in blocks {
                        if block.block_type == "text" {
                            if let Some(text) = &block.text {
                                user_messages.push(text.clone());
                            }
                        }
                        // Skip: image, tool_use, tool_result blocks
                    }
                }
            }
        }
    }

    serde_json::json!({
        "user_messages": user_messages
    }).to_string()
}

/// Extract simple conversation from Anthropic MessagesResponse
/// Returns JSON: {"assistant_response": "text", "note": "..."}
pub fn extract_simple_response_anthropic(response: &crate::models::anthropic::MessagesResponse) -> String {
    let mut text_parts = Vec::new();

    for block in &response.content {
        if block.block_type == "text" {
            if let Some(text) = &block.text {
                text_parts.push(text.as_str());
            }
        }
        // Skip: tool_use, thinking blocks
    }

    if text_parts.is_empty() {
        serde_json::json!({
            "assistant_response": "",
            "note": "Response contains only tool calls (excluded in simple mode)"
        }).to_string()
    } else {
        serde_json::json!({
            "assistant_response": text_parts.join("")
        }).to_string()
    }
}

/// Extract simple conversation from streaming SSE response
/// Returns JSON: {"assistant_response": "text", "note": "..."}
pub fn extract_simple_response_streaming(accumulated_response: &str) -> String {
    let mut text_parts = Vec::new();

    // Parse SSE format: "event: xxx\ndata: {...}\n"
    let parts: Vec<&str> = accumulated_response.split("event:").collect();

    for part in parts {
        if part.trim().is_empty() {
            continue;
        }

        // Extract event type and data
        let lines: Vec<&str> = part.lines().collect();
        if lines.len() < 2 {
            continue;
        }

        let event_type = lines[0].trim();

        for line in &lines[1..] {
            if line.starts_with("data: ") {
                let data = &line[6..];

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    // Anthropic: content_block_delta with text_delta
                    if event_type == "content_block_delta" {
                        if let Some(delta) = json.get("delta") {
                            if let Some(delta_type) = delta.get("type").and_then(|v| v.as_str()) {
                                if delta_type == "text_delta" {
                                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                                        text_parts.push(text.to_string());
                                    }
                                }
                                // Skip: tool_use deltas, thinking deltas
                            }
                        }
                    }

                    // OpenAI: choices[].delta.content
                    if let Some(choices) = json.get("choices").and_then(|v| v.as_array()) {
                        if let Some(first) = choices.first() {
                            if let Some(delta) = first.get("delta") {
                                if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                                    text_parts.push(content.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if text_parts.is_empty() {
        serde_json::json!({
            "assistant_response": "",
            "note": "Response contains only tool calls (excluded in simple mode)"
        }).to_string()
    } else {
        serde_json::json!({
            "assistant_response": text_parts.join("")
        }).to_string()
    }
}

/// Extract simple conversation from OpenAI ChatCompletionRequest
/// Returns JSON: {"user_messages": ["text1", "text2", ...]}
pub fn extract_simple_request_openai(request: &crate::models::openai::ChatCompletionRequest) -> String {
    let mut user_messages = Vec::new();

    for message in &request.messages {
        if message.role == "user" {
            // Use the built-in extract_text method
            let text = message.content.extract_text();
            if !text.is_empty() {
                user_messages.push(text);
            }
        }
    }

    serde_json::json!({
        "user_messages": user_messages
    }).to_string()
}

/// Extract simple conversation from OpenAI ChatCompletionResponse
/// Returns JSON: {"assistant_response": "text", "note": "..."}
pub fn extract_simple_response_openai(response: &crate::models::openai::ChatCompletionResponse) -> String {
    let mut text_parts = Vec::new();

    for choice in &response.choices {
        // Use the built-in extract_text method
        let text = choice.message.content.extract_text();
        if !text.is_empty() {
            text_parts.push(text);
        }
        // Skip: tool_calls, function_call
    }

    if text_parts.is_empty() {
        serde_json::json!({
            "assistant_response": "",
            "note": "Response contains only tool calls (excluded in simple mode)"
        }).to_string()
    } else {
        serde_json::json!({
            "assistant_response": text_parts.join("")
        }).to_string()
    }
}

#[cfg(test)]
mod body_logging_tests {
    use super::*;

    #[test]
    fn test_redact_sensitive_data() {
        let patterns = vec![
            RedactPattern {
                pattern: r"sk-[a-zA-Z0-9]{10}".to_string(),
                replacement: "sk-***REDACTED***".to_string(),
            },
            RedactPattern {
                pattern: r"Bearer [a-zA-Z0-9]+".to_string(),
                replacement: "Bearer ***REDACTED***".to_string(),
            },
        ];

        let body = r#"{"api_key": "sk-abcdefghij", "auth": "Bearer token123"}"#;
        let redacted = redact_sensitive_data(body, &patterns);

        assert!(redacted.contains("sk-***REDACTED***"));
        assert!(redacted.contains("Bearer ***REDACTED***"));
        assert!(!redacted.contains("sk-abcdefghij"));
        assert!(!redacted.contains("token123"));
    }

    #[test]
    fn test_truncate_body() {
        let body = "a".repeat(1000);

        // No truncation
        let (result, truncated) = truncate_body(body.clone(), 2000);
        assert_eq!(result.len(), 1000);
        assert!(!truncated);

        // Truncation
        let (result, truncated) = truncate_body(body, 500);
        assert_eq!(result.len(), 500);
        assert!(truncated);
    }
}

#[cfg(test)]
mod simple_mode_tests {
    use super::*;
    use crate::models::anthropic::{MessagesRequest, Message, MessageContent, ContentBlock, MessagesResponse};

    fn create_test_messages_request_with_text() -> MessagesRequest {
        MessagesRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: MessageContent::Text("Hello".to_string()),
                },
                Message {
                    role: "user".to_string(),
                    content: MessageContent::Text("How are you?".to_string()),
                },
            ],
            max_tokens: 100,
            system: None,
            stream: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            metadata: None,
            thinking: None,
        }
    }

    fn create_test_messages_request_with_blocks() -> MessagesRequest {
        MessagesRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: MessageContent::Blocks(vec![
                        ContentBlock {
                            block_type: "text".to_string(),
                            text: Some("Hello".to_string()),
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
                        ContentBlock {
                            block_type: "image".to_string(),
                            text: None,
                            source: None, // Simplified for test
                            id: None,
                            name: None,
                            input: None,
                            tool_use_id: None,
                            content: None,
                            is_error: None,
                            cache_control: None,
                            thinking: None,
                        },
                    ]),
                },
            ],
            max_tokens: 100,
            system: None,
            stream: None,
            temperature: None,
            top_p: None,
            top_k: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            metadata: None,
            thinking: None,
        }
    }

    fn create_test_messages_response_with_text() -> MessagesResponse {
        MessagesResponse {
            id: "msg_123".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            role: "assistant".to_string(),
            content: vec![
                ContentBlock {
                    block_type: "text".to_string(),
                    text: Some("Hello! I'm doing well.".to_string()),
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
            ],
            stop_reason: Some("end_turn".to_string()),
            stop_sequence: None,
            usage: crate::models::anthropic::TokenUsage {
                input_tokens: 10,
                output_tokens: 15,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
            response_type: "message".to_string(),
        }
    }

    fn create_test_messages_response_with_tool_only() -> MessagesResponse {
        MessagesResponse {
            id: "msg_456".to_string(),
            model: "claude-3-5-sonnet-20241022".to_string(),
            role: "assistant".to_string(),
            content: vec![
                ContentBlock {
                    block_type: "tool_use".to_string(),
                    text: None,
                    source: None,
                    id: Some("tool_123".to_string()),
                    name: Some("calculator".to_string()),
                    input: Some(serde_json::json!({"expression": "2+2"})),
                    tool_use_id: None,
                    content: None,
                    is_error: None,
                    cache_control: None,
                    thinking: None,
                },
            ],
            stop_reason: Some("tool_use".to_string()),
            stop_sequence: None,
            usage: crate::models::anthropic::TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
            response_type: "message".to_string(),
        }
    }

    #[test]
    fn test_extract_simple_request_text_only() {
        let request = create_test_messages_request_with_text();
        let result = extract_simple_request_anthropic(&request);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["user_messages"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["user_messages"][0], "Hello");
        assert_eq!(parsed["user_messages"][1], "How are you?");
    }

    #[test]
    fn test_extract_simple_request_with_blocks() {
        let request = create_test_messages_request_with_blocks();
        let result = extract_simple_request_anthropic(&request);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        // Should extract text blocks, skip image blocks
        assert!(parsed["user_messages"].as_array().unwrap().len() >= 1);
        assert_eq!(parsed["user_messages"][0], "Hello");
    }

    #[test]
    fn test_extract_simple_response_text_only() {
        let response = create_test_messages_response_with_text();
        let result = extract_simple_response_anthropic(&response);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert!(parsed["assistant_response"].as_str().unwrap().len() > 0);
        assert!(parsed.get("note").is_none()); // No note when text present
    }

    #[test]
    fn test_extract_simple_response_tool_only() {
        let response = create_test_messages_response_with_tool_only();
        let result = extract_simple_response_anthropic(&response);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["assistant_response"], "");
        assert!(parsed["note"].as_str().unwrap().contains("tool calls"));
    }

    #[test]
    fn test_extract_simple_response_streaming() {
        let sse_data = r#"event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":" world"}}
"#;
        let result = extract_simple_response_streaming(sse_data);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

        assert_eq!(parsed["assistant_response"], "Hello world");
    }
}
