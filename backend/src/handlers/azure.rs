use crate::{
    auth::AuthInfo,
    error::AppError,
    handlers::common::{handle_path_routed_request, PathRouteConfig, ResponseFormat},
};
use axum::{
    extract::State,
    response::Response,
    Extension, Json,
};

use super::chat_completions::AppState;

/// Handle Azure OpenAI path-routed requests: POST /azure/v1/chat/completions
///
/// Bypasses ModelRouter — provider is determined by the URL path.
pub async fn handle(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    handle_path_routed_request(
        state,
        auth,
        body,
        PathRouteConfig {
            registry_key: "azure_openai".to_string(),
            provider_override: None,
            streaming_format: ResponseFormat::OpenAISSE,
            not_found_message: "Azure OpenAI provider not configured".to_string(),
        },
    )
    .await
}
