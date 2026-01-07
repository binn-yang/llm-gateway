use crate::{
    error::AppError,
    models::{
        gemini::{GenerateContentResponse, Part},
        openai::{ChatCompletionChunk, ChunkChoice, Delta, Usage},
    },
};

/// Convert Gemini streaming chunk to OpenAI ChatCompletionChunk
/// Gemini sends complete response structure in each chunk (unlike Anthropic's delta events)
pub fn convert_streaming_chunk(
    gemini_chunk: &GenerateContentResponse,
    request_id: &str,
    is_first_chunk: &mut bool,
) -> Result<Option<ChatCompletionChunk>, AppError> {
    // Get first candidate
    let candidate = match gemini_chunk.candidates.first() {
        Some(c) => c,
        None => return Ok(None), // Empty chunk, skip
    };

    // Extract text from parts
    let content = extract_text_from_parts(&candidate.content.parts);

    // On first chunk, send role
    let (role, delta_content) = if *is_first_chunk {
        *is_first_chunk = false;
        (Some("assistant".to_string()), None)
    } else if !content.is_empty() {
        (None, Some(content))
    } else {
        (None, None)
    };

    // Map finish reason
    let finish_reason = candidate.finish_reason.as_ref().map(|reason| {
        match reason.as_str() {
            "STOP" => "stop".to_string(),
            "MAX_TOKENS" => "length".to_string(),
            "SAFETY" => "content_filter".to_string(),
            other => other.to_lowercase(),
        }
    });

    // Convert usage metadata (only in last chunk with finish_reason)
    let usage = if finish_reason.is_some() {
        gemini_chunk.usage_metadata.as_ref().map(|u| Usage {
            prompt_tokens: u.prompt_token_count,
            completion_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        })
    } else {
        None
    };

    Ok(Some(ChatCompletionChunk {
        id: request_id.to_string(),
        object: "chat.completion.chunk".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: gemini_chunk
            .model_version
            .clone()
            .unwrap_or_else(|| "gemini".to_string()),
        choices: vec![ChunkChoice {
            index: 0,
            delta: Delta {
                role,
                content: delta_content,
                tool_calls: None,
            },
            finish_reason,
        }],
        usage,
    }))
}

/// Extract text from Gemini Parts
fn extract_text_from_parts(parts: &[Part]) -> String {
    parts
        .iter()
        .filter_map(|part| {
            if let Part::Text { text } = part {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gemini::{Candidate, Content, UsageMetadata};

    #[test]
    fn test_convert_streaming_chunk_first() {
        let chunk = GenerateContentResponse {
            candidates: vec![Candidate {
                content: Content {
                    role: "model".to_string(),
                    parts: vec![Part::Text {
                        text: "Hello".to_string(),
                    }],
                },
                finish_reason: None,
                safety_ratings: None,
            }],
            usage_metadata: None,
            model_version: Some("gemini-1.5-pro".to_string()),
        };

        let mut is_first = true;
        let result = convert_streaming_chunk(&chunk, "test-id", &mut is_first).unwrap();
        assert!(result.is_some());

        let openai_chunk = result.unwrap();
        assert_eq!(openai_chunk.id, "test-id");
        assert_eq!(openai_chunk.choices[0].delta.role, Some("assistant".to_string()));
        assert_eq!(openai_chunk.choices[0].delta.content, None);
        assert!(!is_first); // Should be set to false
    }

    #[test]
    fn test_convert_streaming_chunk_middle() {
        let chunk = GenerateContentResponse {
            candidates: vec![Candidate {
                content: Content {
                    role: "model".to_string(),
                    parts: vec![Part::Text {
                        text: " world".to_string(),
                    }],
                },
                finish_reason: None,
                safety_ratings: None,
            }],
            usage_metadata: None,
            model_version: Some("gemini-1.5-pro".to_string()),
        };

        let mut is_first = false;
        let result = convert_streaming_chunk(&chunk, "test-id", &mut is_first).unwrap();
        assert!(result.is_some());

        let openai_chunk = result.unwrap();
        assert_eq!(openai_chunk.choices[0].delta.role, None);
        assert_eq!(openai_chunk.choices[0].delta.content, Some(" world".to_string()));
    }

    #[test]
    fn test_convert_streaming_chunk_last() {
        let chunk = GenerateContentResponse {
            candidates: vec![Candidate {
                content: Content {
                    role: "model".to_string(),
                    parts: vec![],
                },
                finish_reason: Some("STOP".to_string()),
                safety_ratings: None,
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: 10,
                candidates_token_count: 20,
                total_token_count: 30,
            }),
            model_version: Some("gemini-1.5-pro".to_string()),
        };

        let mut is_first = false;
        let result = convert_streaming_chunk(&chunk, "test-id", &mut is_first).unwrap();
        assert!(result.is_some());

        let openai_chunk = result.unwrap();
        assert_eq!(openai_chunk.choices[0].finish_reason, Some("stop".to_string()));
        assert!(openai_chunk.usage.is_some());
        assert_eq!(openai_chunk.usage.as_ref().unwrap().prompt_tokens, 10);
    }

    #[test]
    fn test_convert_streaming_chunk_empty_candidates() {
        let chunk = GenerateContentResponse {
            candidates: vec![],
            usage_metadata: None,
            model_version: None,
        };

        let mut is_first = false;
        let result = convert_streaming_chunk(&chunk, "test-id", &mut is_first).unwrap();
        assert!(result.is_none());
    }
}
