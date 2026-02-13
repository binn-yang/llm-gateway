use crate::{
    auth::AuthInfo,
    error::AppError,
    handlers::common::{handle_path_routed_request, PathRouteConfig, ResponseFormat},
};
use axum::{
    extract::{Path, State},
    response::Response,
    Extension, Json,
};

use super::chat_completions::AppState;

/// Handle custom provider path-routed requests: POST /custom/:provider_id/v1/chat/completions
///
/// Bypasses ModelRouter — provider is determined by the URL path.
/// Looks up "custom:{provider_id}" in the registry.
pub async fn handle(
    State(state): State<AppState>,
    Path(provider_id): Path<String>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    handle_path_routed_request(
        state,
        auth,
        body,
        PathRouteConfig {
            registry_key: format!("custom:{}", provider_id),
            provider_override: None,
            streaming_format: ResponseFormat::OpenAISSE,
            not_found_message: format!("Custom provider '{}' not configured", provider_id),
        },
    )
    .await
}
