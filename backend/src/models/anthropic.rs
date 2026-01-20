use serde::{Deserialize, Serialize};

/// Anthropic Messages API Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesRequest {
    /// Model to use
    pub model: String,
    /// System prompt (optional) - supports both string and content blocks format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<MessageContent>,
    /// Messages in the conversation
    pub messages: Vec<Message>,
    /// Maximum tokens to generate (required)
    pub max_tokens: u32,
    /// Temperature (0.0 to 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Whether to stream responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Tools available for the model to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Tool choice configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Extended thinking configuration (Claude extended thinking feature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Request metadata (for tracking and filtering)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RequestMetadata>,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
    /// JSON schema for tool input
    pub input_schema: serde_json::Value,
    /// Cache control (for prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Cache control for prompt caching
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheControl {
    /// Cache type (always "ephemeral" for now)
    #[serde(rename = "type")]
    pub cache_type: String,
}

/// Tool choice configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// Auto mode: model decides when to use tools
    Auto { r#type: String },
    /// Any mode: model must use a tool
    Any { r#type: String },
    /// Specific tool: model must use this specific tool
    Tool { r#type: String, name: String },
}

/// Extended thinking configuration (Claude extended thinking feature)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingConfig {
    /// Thinking type (e.g., "enabled")
    #[serde(rename = "type")]
    pub thinking_type: String,
    /// Budget for thinking tokens (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u32>,
}

/// Request metadata for tracking and filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMetadata {
    /// User ID for tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Custom metadata fields
    #[serde(flatten)]
    pub custom: Option<serde_json::Map<String, serde_json::Value>>,
}

/// Message content - supports both string and content blocks format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text string format: "Hello"
    Text(String),
    /// Content blocks format: [{"type": "text", "text": "Hello"}]
    Blocks(Vec<ContentBlock>),
}

/// Message in conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role: "user" or "assistant"
    pub role: String,
    /// Message content (supports both string and blocks format)
    pub content: MessageContent,
}

/// Anthropic Messages API Response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesResponse {
    /// Unique identifier
    pub id: String,
    /// Object type (always "message")
    #[serde(rename = "type")]
    pub response_type: String,
    /// Role (always "assistant")
    pub role: String,
    /// Content blocks
    pub content: Vec<ContentBlock>,
    /// Model used
    pub model: String,
    /// Stop reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Stop sequence (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    /// Token usage
    pub usage: TokenUsage,
}

/// Content block
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentBlock {
    /// Block type (e.g., "text", "image", "tool_use", "tool_result", "thinking")
    #[serde(rename = "type")]
    pub block_type: String,
    /// Text content (for text blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Image source (for image blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ImageSource>,
    /// Tool use ID (for tool_use blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Tool name (for tool_use blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool input (for tool_use blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<serde_json::Value>,
    /// Tool use ID reference (for tool_result blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    /// Tool result content (for tool_result blocks) - can be string or array of content blocks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<serde_json::Value>,
    /// Is error flag (for tool_result blocks)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    /// Cache control (for prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
    /// Thinking field: accepts any format, forwarded as-is to Anthropic API
    /// No type checking - validation is done by Anthropic API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
}

/// Image source for image content blocks
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64-encoded image data
    Base64 {
        media_type: String,
        data: String,
    },
    /// Image URL (note: Anthropic API may not support this, but included for completeness)
    Url {
        url: String,
    },
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Input tokens
    pub input_tokens: u64,
    /// Output tokens
    pub output_tokens: u64,
    /// Cache creation input tokens (for prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u64>,
    /// Cache read input tokens (for prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u64>,
}

/// Streaming event from Anthropic SSE
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    /// Event type
    #[serde(rename = "type")]
    pub event_type: String,
    /// Message data (for message_start, message_delta, message_stop)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<MessageData>,
    /// Content block index (for content_block_start, content_block_delta)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// Content block (for content_block_start)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_block: Option<ContentBlock>,
    /// Delta (for content_block_delta, message_delta)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<Delta>,
    /// Usage (for message_delta, message_stop)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Message data in streaming events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    pub id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,
}

/// Delta for streaming updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    /// Delta type (e.g., "text_delta", "input_json_delta")
    /// Optional because message_delta events don't include this field
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub delta_type: Option<String>,
    /// Text content (for text deltas)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Stop reason (for message_delta)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    /// Partial JSON input (for tool use input_json_delta)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_json: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_messages_request() {
        let request = MessagesRequest {
            model: "claude-3-5-sonnet-20241022".to_string(),
            system: Some(MessageContent::Text("You are a helpful assistant.".to_string())),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("Hello!".to_string()),
            }],
            max_tokens: 1024,
            temperature: Some(0.7),
            top_p: None,
            top_k: None,
            tools: None,
            tool_choice: None,
            stream: Some(false),
            stop_sequences: None,
            thinking: None,
            metadata: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("claude-3-5-sonnet"));
        assert!(json.contains("Hello!"));
        assert!(json.contains("max_tokens"));
    }

    #[test]
    fn test_deserialize_messages_response() {
        let json = r#"{
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "text",
                "text": "Hello! How can I help you?"
            }],
            "model": "claude-3-5-sonnet-20241022",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 25
            }
        }"#;

        let response: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "msg_123");
        assert_eq!(response.content[0].text.as_ref().unwrap(), "Hello! How can I help you?");
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 25);
    }

    #[test]
    fn test_deserialize_stream_event_content_block_delta() {
        let json = r#"{
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello"
            }
        }"#;

        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "content_block_delta");
        assert_eq!(event.delta.as_ref().unwrap().text.as_ref().unwrap(), "Hello");
    }

    #[test]
    fn test_deserialize_stream_event_message_stop() {
        let json = r#"{
            "type": "message_stop"
        }"#;

        let event: StreamEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "message_stop");
    }

    #[test]
    fn test_deserialize_thinking_as_string() {
        let json = r#"{
            "type": "text",
            "text": "hello",
            "thinking": "some thought"
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.thinking.is_some());
        assert_eq!(block.thinking.as_ref().unwrap().as_str(), Some("some thought"));
    }

    #[test]
    fn test_deserialize_thinking_as_object_without_signature() {
        let json = r#"{
            "type": "text",
            "text": "hello",
            "thinking": {
                "thinking": "some thought"
            }
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.thinking.is_some());
        // thinking 被保留为 Value，不会反序列化失败
    }

    #[test]
    fn test_deserialize_thinking_as_full_object() {
        let json = r#"{
            "type": "text",
            "text": "hello",
            "thinking": {
                "thinking": "some thought",
                "signature": "valid_sig"
            }
        }"#;

        let block: ContentBlock = serde_json::from_str(json).unwrap();
        assert!(block.thinking.is_some());
        let thinking = block.thinking.as_ref().unwrap();
        assert!(thinking.get("thinking").is_some());
        assert!(thinking.get("signature").is_some());
    }

    #[test]
    fn test_serialize_preserves_thinking() {
        let block = ContentBlock {
            block_type: "text".to_string(),
            text: Some("hello".to_string()),
            source: None,
            id: None,
            name: None,
            input: None,
            tool_use_id: None,
            content: None,
            is_error: None,
            cache_control: None,
            thinking: Some(serde_json::json!({"custom": "format", "data": 123})),
        };

        let json = serde_json::to_string(&block).unwrap();
        assert!(json.contains("\"thinking\""));
        assert!(json.contains("\"custom\""));
        assert!(json.contains("\"data\""));

        // Verify round-trip
        let deserialized: ContentBlock = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.thinking, block.thinking);
    }
}
