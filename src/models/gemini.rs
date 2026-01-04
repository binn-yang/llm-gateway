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

/// Part (text content)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub text: String,
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

/// Safety rating
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyRating {
    pub category: String,
    pub probability: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_generate_content_request() {
        let request = GenerateContentRequest {
            contents: vec![Content {
                role: "user".to_string(),
                parts: vec![Part {
                    text: "Hello!".to_string(),
                }],
            }],
            system_instruction: Some(SystemInstruction {
                parts: vec![Part {
                    text: "You are helpful.".to_string(),
                }],
            }),
            generation_config: Some(GenerationConfig {
                temperature: Some(0.7),
                top_p: Some(0.9),
                top_k: None,
                max_output_tokens: Some(1024),
                stop_sequences: None,
            }),
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
        assert_eq!(response.candidates[0].content.parts[0].text, "Hello! How can I help?");
        assert_eq!(
            response.usage_metadata.as_ref().unwrap().prompt_token_count,
            5
        );
        assert_eq!(
            response.usage_metadata.as_ref().unwrap().candidates_token_count,
            10
        );
    }
}
