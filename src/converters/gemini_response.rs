use crate::{
    error::AppError,
    models::{
        gemini::{GenerateContentResponse, Part},
        openai::{
            ChatChoice, ChatCompletionResponse, ChatMessage, FunctionCall, MessageContent,
            ToolCall, Usage,
        },
    },
};

/// Convert Gemini GenerateContentResponse to OpenAI ChatCompletionResponse
pub fn convert_response(
    gemini_resp: &GenerateContentResponse,
) -> Result<ChatCompletionResponse, AppError> {
    // Get first candidate
    let candidate = gemini_resp
        .candidates
        .first()
        .ok_or_else(|| {
            tracing::error!(
                "Gemini returned empty candidates. Response: {:?}",
                gemini_resp
            );
            AppError::ConversionError("No candidates in Gemini response".to_string())
        })?;

    // Extract text and tool calls from parts
    let content = extract_text_from_parts(&candidate.content.parts);
    let tool_calls = extract_tool_calls_from_parts(&candidate.content.parts);

    // Map finish reason
    let finish_reason = candidate.finish_reason.as_ref().map(|reason| {
        // Gemini uses STOP, SAFETY, etc. Map to OpenAI equivalents
        match reason.as_str() {
            "STOP" => "stop".to_string(),
            "MAX_TOKENS" => "length".to_string(),
            "SAFETY" => "content_filter".to_string(),
            other => other.to_lowercase(),
        }
    });

    // Convert usage metadata
    let usage = gemini_resp.usage_metadata.as_ref().map(|u| Usage {
        prompt_tokens: u.prompt_token_count,
        completion_tokens: u.candidates_token_count,
        total_tokens: u.total_token_count,
    });

    Ok(ChatCompletionResponse {
        id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
        object: "chat.completion".to_string(),
        created: chrono::Utc::now().timestamp() as u64,
        model: gemini_resp
            .model_version
            .clone()
            .unwrap_or_else(|| "gemini".to_string()),
        choices: vec![ChatChoice {
            index: 0,
            message: ChatMessage {
                role: "assistant".to_string(),
                content: MessageContent::Text(content),
                name: None,
                tool_calls: if tool_calls.is_empty() {
                    None
                } else {
                    Some(tool_calls)
                },
            },
            finish_reason,
            logprobs: None, // Gemini doesn't provide log probabilities
        }],
        usage,
    })
}

/// Extract text from Gemini Parts (handling the Part enum)
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

/// Extract tool calls from Gemini Parts
fn extract_tool_calls_from_parts(parts: &[Part]) -> Vec<ToolCall> {
    parts
        .iter()
        .filter_map(|part| {
            if let Part::FunctionCall { function_call } = part {
                Some(ToolCall {
                    id: format!("call_{}", uuid::Uuid::new_v4()),
                    tool_type: "function".to_string(),
                    function: FunctionCall {
                        name: function_call.name.clone(),
                        arguments: serde_json::to_string(&function_call.args)
                            .unwrap_or_else(|_| "{}".to_string()),
                    },
                })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::gemini::{Candidate, Content, Part, UsageMetadata};

    #[test]
    fn test_convert_response() {
        let gemini_resp = GenerateContentResponse {
            candidates: vec![Candidate {
                content: Content {
                    role: "model".to_string(),
                    parts: vec![Part::Text {
                        text: "Hello! How can I help you?".to_string(),
                    }],
                },
                finish_reason: Some("STOP".to_string()),
                safety_ratings: None,
            }],
            usage_metadata: Some(UsageMetadata {
                prompt_token_count: 10,
                candidates_token_count: 25,
                total_token_count: 35,
            }),
            model_version: Some("gemini-1.5-pro".to_string()),
        };

        let openai_resp = convert_response(&gemini_resp).unwrap();
        assert_eq!(openai_resp.object, "chat.completion");
        assert_eq!(openai_resp.model, "gemini-1.5-pro");
        assert_eq!(openai_resp.choices[0].message.role, "assistant");
        assert_eq!(
            openai_resp.choices[0].message.content.extract_text(),
            "Hello! How can I help you?"
        );
        assert_eq!(openai_resp.choices[0].finish_reason, Some("stop".to_string()));
        assert_eq!(openai_resp.usage.as_ref().unwrap().prompt_tokens, 10);
        assert_eq!(openai_resp.usage.as_ref().unwrap().completion_tokens, 25);
    }

    #[test]
    fn test_convert_response_finish_reason_mapping() {
        let gemini_resp = GenerateContentResponse {
            candidates: vec![Candidate {
                content: Content {
                    role: "model".to_string(),
                    parts: vec![Part::Text {
                        text: "Text".to_string(),
                    }],
                },
                finish_reason: Some("MAX_TOKENS".to_string()),
                safety_ratings: None,
            }],
            usage_metadata: None,
            model_version: None,
        };

        let openai_resp = convert_response(&gemini_resp).unwrap();
        assert_eq!(openai_resp.choices[0].finish_reason, Some("length".to_string()));
    }

    #[test]
    fn test_convert_response_no_candidates() {
        let gemini_resp = GenerateContentResponse {
            candidates: vec![],
            usage_metadata: None,
            model_version: None,
        };

        let result = convert_response(&gemini_resp);
        assert!(result.is_err());
    }
}
