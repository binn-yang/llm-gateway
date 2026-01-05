use crate::{
    error::AppError,
    models::{
        anthropic::{Message, MessagesRequest},
        openai::{ChatCompletionRequest, ChatMessage},
    },
};

/// Convert OpenAI ChatCompletionRequest to Anthropic MessagesRequest
pub fn convert_request(openai_req: &ChatCompletionRequest) -> Result<MessagesRequest, AppError> {
    // Extract system message (Anthropic uses a separate system field)
    let (system, messages) = extract_system_message(&openai_req.messages);

    // Convert messages (filter out system messages)
    let anthropic_messages = messages
        .iter()
        .map(|msg| Message {
            role: msg.role.clone(),
            content: msg.content.clone(),
        })
        .collect();

    // Anthropic requires max_tokens, default to 4096 if not provided
    let max_tokens = openai_req.max_tokens.unwrap_or(4096);

    // Anthropic temperature is 0-1, clip if necessary
    let temperature = openai_req.temperature.map(|t| {
        if t > 1.0 {
            tracing::warn!(
                "Temperature {} exceeds Anthropic maximum (1.0), clipping to 1.0",
                t
            );
            1.0
        } else {
            t
        }
    });

    // Convert stop sequences
    let stop_sequences = openai_req.stop.clone();

    Ok(MessagesRequest {
        model: openai_req.model.clone(),
        system,
        messages: anthropic_messages,
        max_tokens,
        temperature,
        top_p: openai_req.top_p,
        top_k: None, // OpenAI doesn't have top_k
        stream: openai_req.stream,
        stop_sequences,
    })
}

/// Extract system message from OpenAI messages
/// Returns (system_prompt, remaining_messages)
fn extract_system_message(messages: &[ChatMessage]) -> (Option<String>, Vec<ChatMessage>) {
    if let Some(first) = messages.first() {
        if first.role == "system" {
            let system = Some(first.content.clone());
            let rest = messages.iter().skip(1).cloned().collect();
            return (system, rest);
        }
    }
    (None, messages.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_system_message() {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: "You are a helpful assistant.".to_string(),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Hello!".to_string(),
                name: None,
            },
        ];

        let (system, remaining) = extract_system_message(&messages);
        assert_eq!(system, Some("You are a helpful assistant.".to_string()));
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].role, "user");
    }

    #[test]
    fn test_extract_system_message_no_system() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello!".to_string(),
            name: None,
        }];

        let (system, remaining) = extract_system_message(&messages);
        assert_eq!(system, None);
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn test_convert_request_basic() {
        let openai_req = ChatCompletionRequest {
            model: "claude-3-5-sonnet".to_string(),
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
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: Some(0.9),
            n: None,
            stream: Some(false),
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let anthropic_req = convert_request(&openai_req).unwrap();
        assert_eq!(anthropic_req.model, "claude-3-5-sonnet");
        assert_eq!(anthropic_req.system, Some("You are helpful.".to_string()));
        assert_eq!(anthropic_req.messages.len(), 1);
        assert_eq!(anthropic_req.messages[0].role, "user");
        assert_eq!(anthropic_req.max_tokens, 100);
        assert_eq!(anthropic_req.temperature, Some(0.7));
    }

    #[test]
    fn test_convert_request_default_max_tokens() {
        let openai_req = ChatCompletionRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi!".to_string(),
                name: None,
            }],
            max_tokens: None, // Not provided
            temperature: None,
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let anthropic_req = convert_request(&openai_req).unwrap();
        assert_eq!(anthropic_req.max_tokens, 4096); // Default
    }

    #[test]
    fn test_convert_request_clip_temperature() {
        let openai_req = ChatCompletionRequest {
            model: "claude-3-5-sonnet".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "Hi!".to_string(),
                name: None,
            }],
            max_tokens: Some(100),
            temperature: Some(1.5), // > 1.0, should be clipped
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
        };

        let anthropic_req = convert_request(&openai_req).unwrap();
        assert_eq!(anthropic_req.temperature, Some(1.0)); // Clipped to 1.0
    }
}
