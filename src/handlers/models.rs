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
/// Returns list of all available models
/// TODO: Implement dynamic model discovery from providers
pub async fn list_models(State(_state): State<AppState>) -> impl IntoResponse {
    // Temporary implementation: return empty list
    // Full model discovery will be implemented in phase 2
    let models: Vec<ModelObject> = vec![];

    Json(ModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, ApiKeyConfig, Config, DiscoveryConfig, MetricsConfig, ProviderConfig,
        ProvidersConfig, RoutingConfig, ServerConfig,
    };
    use crate::router::ModelRouter;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_state() -> AppState {
        let mut routing_rules = HashMap::new();
        routing_rules.insert("gpt-".to_string(), "openai".to_string());
        routing_rules.insert("claude-".to_string(), "anthropic".to_string());

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
            routing: RoutingConfig {
                rules: routing_rules,
                default_provider: Some("openai".to_string()),
                discovery: DiscoveryConfig {
                    enabled: true,
                    cache_ttl_seconds: 3600,
                    refresh_on_startup: true,
                    providers_with_listing: vec!["openai".to_string()],
                },
            },
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

        let config = Arc::new(arc_swap::ArcSwap::new(Arc::new(config)));
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
