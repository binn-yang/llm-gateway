use crate::{
    error::AppError,
    models::{
        gemini::{Content, GenerateContentRequest, GenerationConfig, Part, SystemInstruction},
        openai::{ChatCompletionRequest, ChatMessage},
    },
};

/// Convert OpenAI ChatCompletionRequest to Gemini GenerateContentRequest
pub fn convert_request(openai_req: &ChatCompletionRequest) -> Result<GenerateContentRequest, AppError> {
    // Extract system instruction
    let (system_instruction, messages) = extract_system_instruction(&openai_req.messages);

    // Convert messages
    let contents = messages
        .iter()
        .map(|msg| Content {
            // Gemini uses "model" instead of "assistant"
            role: if msg.role == "assistant" {
                "model".to_string()
            } else {
                msg.role.clone()
            },
            parts: vec![Part {
                text: msg.content.clone(),
            }],
        })
        .collect();

    // Generation config
    let generation_config = if openai_req.max_tokens.is_some()
        || openai_req.temperature.is_some()
        || openai_req.top_p.is_some()
        || openai_req.stop.is_some()
    {
        Some(GenerationConfig {
            temperature: openai_req.temperature,
            top_p: openai_req.top_p,
            top_k: None,
            max_output_tokens: openai_req.max_tokens,
            stop_sequences: openai_req.stop.clone(),
        })
    } else {
        None
    };

    Ok(GenerateContentRequest {
        contents,
        system_instruction,
        generation_config,
    })
}

/// Extract system instruction from OpenAI messages
/// Returns (system_instruction, remaining_messages)
fn extract_system_instruction(
    messages: &[ChatMessage],
) -> (Option<SystemInstruction>, Vec<ChatMessage>) {
    if let Some(first) = messages.first() {
        if first.role == "system" {
            let system_instruction = Some(SystemInstruction {
                parts: vec![Part {
                    text: first.content.clone(),
                }],
            });
            let rest = messages.iter().skip(1).cloned().collect();
            return (system_instruction, rest);
        }
    }
    (None, messages.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_system_instruction() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are helpful.".to_string(),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
                name: None,
            },
        ];

        let (system, remaining) = extract_system_instruction(&messages);
        assert!(system.is_some());
        assert_eq!(system.unwrap().parts[0].text, "You are helpful.");
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_convert_request_basic() {
        let openai_req = ChatCompletionRequest {
            model: "gemini-1.5-pro".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "You are helpful.".to_string(),
                    name: None,
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "Hi!".to_string(),
                    name: None,
                },
                ChatMessage {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                    name: None,
                },
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: Some(0.9),
            n: None,
            stream: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let gemini_req = convert_request(&openai_req).unwrap();
        assert!(gemini_req.system_instruction.is_some());
        assert_eq!(gemini_req.contents.len(), 2);
        assert_eq!(gemini_req.contents[0].role, "user");
        assert_eq!(gemini_req.contents[0].parts[0].text, "Hi!");
        assert_eq!(gemini_req.contents[1].role, "model"); // assistant → model
        assert_eq!(gemini_req.contents[1].parts[0].text, "Hello!");
        assert_eq!(
            gemini_req
                .generation_config
                .as_ref()
                .unwrap()
                .max_output_tokens,
            Some(100)
        );
    }

    #[test]
    fn test_convert_request_assistant_to_model() {
        let openai_req = ChatCompletionRequest {
            model: "gemini-1.5-pro".to_string(),
            messages: vec![ChatMessage {
                role: "assistant".to_string(),
                content: "I'm an assistant".to_string(),
                name: None,
            }],
            max_tokens: None,
            temperature: None,
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let gemini_req = convert_request(&openai_req).unwrap();
        assert_eq!(gemini_req.contents[0].role, "model");
    }
}
