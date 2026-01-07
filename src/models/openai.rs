use serde::{Deserialize, Serialize};

/// OpenAI Chat Completion Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionRequest {
    /// Model to use
    pub model: String,
    /// Messages in the conversation
    pub messages: Vec<ChatMessage>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature (0.0 to 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Number of completions to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Whether to stream responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// Stop sequences
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
    /// Presence penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// User identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Tools (functions) available to the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// How the model should use tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Response format (for JSON mode and structured outputs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    /// Seed for deterministic sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Whether to return log probabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// Number of most likely tokens to return at each position (0-20)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    /// Token probability bias (map of token ID to bias value -100 to 100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<std::collections::HashMap<String, f32>>,
    /// Service tier ("auto" or "default")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
}

/// Message content - supports both simple string and multimodal content blocks
/// This uses #[serde(untagged)] for backward compatibility with existing string-only content
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Simple text string format: "Hello"
    Text(String),
    /// Content blocks format for multimodal content: [{\"type\": \"text\", \"text\": \"Hello\"}, {\"type\": \"image_url\", ...}]
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// Get text content if this is a Text variant
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            MessageContent::Blocks(_) => None,
        }
    }

    /// Check if this is text-only content
    pub fn is_text_only(&self) -> bool {
        matches!(self, MessageContent::Text(_))
    }

    /// Get content blocks if this is a Blocks variant
    pub fn blocks(&self) -> Option<&Vec<ContentBlock>> {
        match self {
            MessageContent::Blocks(blocks) => Some(blocks),
            MessageContent::Text(_) => None,
        }
    }

    /// Extract all text content from either variant
    pub fn extract_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Blocks(blocks) => {
                blocks.iter()
                    .filter_map(|block| {
                        if let ContentBlock::Text { text } = block {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("")
            }
        }
    }
}

/// Content block for multimodal messages
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content block
    Text {
        text: String,
    },
    /// Image URL content block
    ImageUrl {
        image_url: ImageUrl,
    },
    /// Tool use block (for function calling responses)
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    /// Tool result block (for function calling)
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

/// Image URL specification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageUrl {
    /// URL to the image (can be http(s):// or data: URL)
    pub url: String,
    /// Image detail level (low, high, auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Tool (function) definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// Tool type (always "function" for now)
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function definition
    pub function: FunctionDefinition,
}

/// Function definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// Function name
    pub name: String,
    /// Function description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema for function parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Tool choice setting
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// "none", "auto", or "required"
    String(String),
    /// Specific tool to use
    Specific { r#type: String, function: ToolChoiceFunction },
}

/// Specific tool choice function
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolChoiceFunction {
    /// Function name to use
    pub name: String,
}

/// Tool call (in assistant response)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool call ID
    pub id: String,
    /// Type (always "function")
    #[serde(rename = "type")]
    pub tool_type: String,
    /// Function call details
    pub function: FunctionCall,
}

/// Function call details
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Function name
    pub name: String,
    /// Function arguments (JSON string)
    pub arguments: String,
}

/// Response format for controlling output structure
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Plain text response (default)
    Text,
    /// JSON object response
    JsonObject,
    /// Structured JSON response with schema
    JsonSchema {
        json_schema: JsonSchemaSpec,
    },
}

/// JSON Schema specification for structured outputs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonSchemaSpec {
    /// Schema name
    pub name: String,
    /// Schema description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema definition
    pub schema: serde_json::Value,
    /// Whether to enforce strict schema adherence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

/// Chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Role: system, user, or assistant
    pub role: String,
    /// Message content (supports both string and multimodal blocks)
    pub content: MessageContent,
    /// Optional name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tool calls (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// OpenAI Chat Completion Response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    /// Unique identifier
    pub id: String,
    /// Object type (always "chat.completion")
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// Completion choices
    pub choices: Vec<ChatChoice>,
    /// Token usage information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// Chat completion choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatChoice {
    /// Choice index
    pub index: u32,
    /// Generated message
    pub message: ChatMessage,
    /// Finish reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    /// Log probabilities (if requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<LogProbsResult>,
}

/// Log probabilities result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogProbsResult {
    /// Log probabilities for each token
    pub content: Vec<TokenLogProbs>,
}

/// Log probabilities for a single token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLogProbs {
    /// The token
    pub token: String,
    /// Log probability
    pub logprob: f32,
    /// Byte positions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
    /// Top alternative tokens with their log probabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<Vec<TopLogProb>>,
}

/// Top alternative token with log probability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopLogProb {
    /// The token
    pub token: String,
    /// Log probability
    pub logprob: f32,
    /// Byte positions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<Vec<u8>>,
}

/// Token usage information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    /// Input/prompt tokens
    pub prompt_tokens: u64,
    /// Output/completion tokens
    pub completion_tokens: u64,
    /// Total tokens
    pub total_tokens: u64,
}

/// OpenAI Streaming Response Chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionChunk {
    /// Unique identifier
    pub id: String,
    /// Object type (always "chat.completion.chunk")
    pub object: String,
    /// Creation timestamp
    pub created: u64,
    /// Model used
    pub model: String,
    /// Choices with delta
    pub choices: Vec<ChunkChoice>,
    /// Usage information (only in last chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

/// Streaming chunk choice
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkChoice {
    /// Choice index
    pub index: u32,
    /// Delta (incremental content)
    pub delta: Delta,
    /// Finish reason
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Delta content for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    /// Role (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Incremental content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Tool calls (for streaming tool use)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallDelta>>,
}

/// Tool call delta for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    /// Index of the tool call
    pub index: u32,
    /// Tool call ID (only in first chunk of this tool call)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Tool type (always "function")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "type")]
    pub tool_type: Option<String>,
    /// Function details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<FunctionCallDelta>,
}

/// Function call delta for streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallDelta {
    /// Function name (only in first chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Partial arguments (JSON string fragments)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_chat_completion_request() {
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: MessageContent::Text("You are a helpful assistant.".to_string()),
                    name: None,
                    tool_calls: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: MessageContent::Text("Hello!".to_string()),
                    name: None,
                    tool_calls: None,
                },
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: None,
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

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("gpt-4"));
        assert!(json.contains("Hello!"));
    }

    #[test]
    fn test_deserialize_chat_completion_response() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 9,
                "total_tokens": 19
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.choices[0].message.content.extract_text(), "Hello! How can I help you?");
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 19);
    }

    #[test]
    fn test_message_content_backward_compatibility() {
        // Test that simple string content still works (backward compatibility)
        let json = r#"{"role":"user","content":"Hello"}"#;
        let message: ChatMessage = serde_json::from_str(json).unwrap();
        assert!(message.content.is_text_only());
        assert_eq!(message.content.as_text().unwrap(), "Hello");
    }

    #[test]
    fn test_message_content_blocks() {
        // Test multimodal content blocks
        let json = r#"{"role":"user","content":[{"type":"text","text":"Hello"},{"type":"image_url","image_url":{"url":"https://example.com/image.jpg"}}]}"#;
        let message: ChatMessage = serde_json::from_str(json).unwrap();
        assert!(!message.content.is_text_only());
        assert_eq!(message.content.blocks().unwrap().len(), 2);
    }

    #[test]
    fn test_extract_text() {
        // Test extracting text from blocks
        let content = MessageContent::Blocks(vec![
            ContentBlock::Text { text: "Hello ".to_string() },
            ContentBlock::Text { text: "world".to_string() },
        ]);
        assert_eq!(content.extract_text(), "Hello world");
    }

    #[test]
    fn test_deserialize_streaming_chunk() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "Hello"
                },
                "finish_reason": null
            }]
        }"#;

        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.id, "chatcmpl-123");
        assert_eq!(chunk.choices[0].delta.content.as_ref().unwrap(), "Hello");
    }
}
