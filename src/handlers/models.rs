use axum::{extract::State, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};

use crate::handlers::chat_completions::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelObject>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelObject {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub owned_by: String,
}

/// Handle /v1/models endpoint
/// Returns list of all available models from configuration
pub async fn list_models(State(state): State<AppState>) -> impl IntoResponse {
    let models: Vec<ModelObject> = state
        .router
        .available_models()
        .into_iter()
        .map(|model_name| ModelObject {
            id: model_name,
            object: "model".to_string(),
            created: chrono::Utc::now().timestamp() as u64,
            owned_by: "llm-gateway".to_string(),
        })
        .collect();

    Json(ModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, ApiKeyConfig, Config, MetricsConfig, ModelConfig, ProviderConfig,
        ProvidersConfig, ServerConfig,
    };
    use crate::router::ModelRouter;
    use std::collections::HashMap;

    fn create_test_state() -> AppState {
        let mut models = HashMap::new();
        models.insert(
            "gpt-4".to_string(),
            ModelConfig {
                provider: "openai".to_string(),
                api_model: "gpt-4".to_string(),
            },
        );
        models.insert(
            "claude-3-5-sonnet".to_string(),
            ModelConfig {
                provider: "anthropic".to_string(),
                api_model: "claude-3-5-sonnet-20241022".to_string(),
            },
        );

        let config = Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                log_format: "json".to_string(),
            },
            api_keys: vec![ApiKeyConfig {
                key: "sk-test".to_string(),
                name: "test".to_string(),
                enabled: true,
            }],
            models,
            providers: ProvidersConfig {
                openai: ProviderConfig {
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                },
                anthropic: AnthropicConfig {
                    enabled: true,
                    api_key: "test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                },
                gemini: ProviderConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                },
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "/metrics".to_string(),
                include_api_key_hash: true,
            },
        };

        let config = Arc::new(config);
        let router = Arc::new(ModelRouter::new(config.clone()));
        let http_client = reqwest::Client::new();

        AppState {
            config,
            router,
            http_client,
        }
    }

    #[tokio::test]
    async fn test_list_models() {
        let state = create_test_state();
        let response = list_models(State(state)).await.into_response();

        assert_eq!(response.status(), 200);
    }
}
