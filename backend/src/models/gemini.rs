use serde::{Deserialize, Serialize};

/// Gemini Generate Content Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateContentRequest {
    /// Contents (messages)
    pub contents: Vec<Content>,
    /// System instruction (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "systemInstruction")]
    pub system_instruction: Option<SystemInstruction>,
    /// Generation configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "generationConfig")]
    pub generation_config: Option<GenerationConfig>,
    /// Safety settings (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "safetySettings")]
    pub safety_settings: Option<Vec<SafetySetting>>,
    /// Tools (function declarations) available to the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    /// Tool configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "toolConfig")]
    pub tool_config: Option<ToolConfig>,
}

/// System instruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInstruction {
    pub parts: Vec<Part>,
}

/// Content block (message)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Content {
    /// Role: "user" or "model"
    pub role: String,
    /// Parts (text content)
    pub parts: Vec<Part>,
}

/// Part - multimodal content part
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Part {
    /// Text content
    Text {
        text: String,
    },
    /// Inline data (e.g., base64-encoded images)
    InlineData {
        inline_data: InlineData,
    },
    /// Function call (for tool use)
    FunctionCall {
        function_call: FunctionCall,
    },
    /// Function response (for tool results)
    FunctionResponse {
        function_response: FunctionResponse,
    },
}

/// Inline data for images and other binary content
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InlineData {
    pub mime_type: String,
    pub data: String, // base64-encoded
}

/// Function call for tool use
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub args: serde_json::Value,
}

/// Function response for tool results
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FunctionResponse {
    pub name: String,
    pub response: serde_json::Value,
}

/// Generation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "topP")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "topK")]
    pub top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxOutputTokens")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "stopSequences")]
    pub stop_sequences: Option<Vec<String>>,
    /// Response MIME type (e.g., "application/json" for JSON mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "responseMimeType")]
    pub response_mime_type: Option<String>,
    /// Response schema (JSON Schema for structured output)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "responseSchema")]
    pub response_schema: Option<serde_json::Value>,
}

/// Gemini Generate Content Response (non-streaming)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateContentResponse {
    /// Candidates
    pub candidates: Vec<Candidate>,
    /// Usage metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "usageMetadata")]
    pub usage_metadata: Option<UsageMetadata>,
    /// Model version
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "modelVersion")]
    pub model_version: Option<String>,
}

/// Candidate response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    /// Content
    pub content: Content,
    /// Finish reason
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
    /// Safety ratings
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "safetyRatings")]
    pub safety_ratings: Option<Vec<SafetyRating>>,
}

/// Safety rating (in responses)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRating {
    pub category: String,
    pub probability: String,
}

/// Safety setting (in requests)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetySetting {
    /// Harm category (e.g., "HARM_CATEGORY_HATE_SPEECH")
    pub category: String,
    /// Threshold level (e.g., "BLOCK_MEDIUM_AND_ABOVE", "BLOCK_ONLY_HIGH", "BLOCK_LOW_AND_ABOVE", "BLOCK_NONE")
    pub threshold: String,
}

/// Tool definition for function calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Function declarations
    #[serde(rename = "functionDeclarations")]
    pub function_declarations: Vec<FunctionDeclaration>,
}

/// Function declaration (Gemini's tool format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDeclaration {
    /// Function name
    pub name: String,
    /// Function description
    pub description: String,
    /// Parameters schema (JSON Schema)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

/// Tool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfig {
    /// Function calling config
    #[serde(rename = "functionCallingConfig")]
    pub function_calling_config: FunctionCallingConfig,
}

/// Function calling configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallingConfig {
    /// Mode: "AUTO", "ANY", "NONE"
    pub mode: String,
    /// Allowed function names (when mode is "ANY")
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "allowedFunctionNames")]
    pub allowed_function_names: Option<Vec<String>>,
}

/// Usage metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageMetadata {
    #[serde(rename = "promptTokenCount")]
    pub prompt_token_count: u64,
    #[serde(rename = "candidatesTokenCount")]
    pub candidates_token_count: u64,
    #[serde(rename = "totalTokenCount")]
    pub total_token_count: u64,
}

/// countTokens 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountTokensRequest {
    /// Contents to count tokens for
    pub contents: Vec<Content>,
    /// System instruction (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "systemInstruction")]
    pub system_instruction: Option<SystemInstruction>,
}

/// countTokens 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountTokensResponse {
    #[serde(rename = "totalTokens")]
    pub total_tokens: u64,
}

/// 模型信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub version: String,
    #[serde(rename = "displayName")]
    pub display_name: String,
    pub description: String,
    #[serde(rename = "inputTokenLimit")]
    pub input_token_limit: u32,
    #[serde(rename = "outputTokenLimit")]
    pub output_token_limit: u32,
    #[serde(rename = "supportedGenerationMethods")]
    pub supported_generation_methods: Vec<String>,
    #[serde(rename = "temperature")]
    pub temperature: Option<f32>,
    #[serde(rename = "maxTemperature")]
    pub max_temperature: Option<f32>,
    #[serde(rename = "topK")]
    pub top_k: Option<u32>,
    #[serde(rename = "topP")]
    pub top_p: Option<f32>,
}

/// listModels 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListModelsResponse {
    pub models: Vec<ModelInfo>,
    #[serde(rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

/// getModel 响应（单个模型详情）
pub type GetModelResponse = ModelInfo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_generate_content_request() {
        let request = GenerateContentRequest {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part::Text {
                    text: "Hello!".to_string(),
                }],
            }],
            system_instruction: Some(SystemInstruction {
                parts: vec![Part::Text {
                    text: "You are helpful.".to_string(),
                }],
            }),
            generation_config: Some(GenerationConfig {
                temperature: Some(0.7),
                top_p: Some(0.9),
                top_k: None,
                max_output_tokens: Some(1024),
                stop_sequences: None,
                response_mime_type: None,
                response_schema: None,
            }),
            safety_settings: None,
            tools: None,
            tool_config: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("Hello!"));
        assert!(json.contains("You are helpful"));
        assert!(json.contains("generationConfig"));
    }

    #[test]
    fn test_deserialize_generate_content_response() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "text": "Hello! How can I help?"
                    }]
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 10,
                "totalTokenCount": 15
            },
            "modelVersion": "gemini-1.5-pro"
        }"#;

        let response: GenerateContentResponse = serde_json::from_str(json).unwrap();
        if let Part::Text { text } = &response.candidates[0].content.parts[0] {
            assert_eq!(text, "Hello! How can I help?");
        } else {
            panic!("Expected Text part");
        }
        assert_eq!(
            response.usage_metadata.as_ref().unwrap().prompt_token_count,
            5
        );
        assert_eq!(
            response.usage_metadata.as_ref().unwrap().candidates_token_count,
            10
        );
    }

    #[test]
    fn test_part_backward_compatibility() {
        // Test that simple text parts still deserialize correctly
        let json = r#"{"text": "Hello world"}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        if let Part::Text { text } = part {
            assert_eq!(text, "Hello world");
        } else {
            panic!("Expected Text part");
        }
    }

    #[test]
    fn test_part_inline_data() {
        // Test inline data for images
        let json = r#"{"inline_data": {"mime_type": "image/jpeg", "data": "base64data"}}"#;
        let part: Part = serde_json::from_str(json).unwrap();
        if let Part::InlineData { inline_data } = part {
            assert_eq!(inline_data.mime_type, "image/jpeg");
            assert_eq!(inline_data.data, "base64data");
        } else {
            panic!("Expected InlineData part");
        }
    }
}
