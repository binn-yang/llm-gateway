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
use std::sync::Arc;

use super::chat_completions::AppState;

/// Handle OpenAI Responses API: POST /v1/responses
///
/// Transparent passthrough to OpenAI's Responses API endpoint.
/// Uses "openai" provider from registry (same instances as /v1/chat/completions).
/// The OpenAIResponsesProvider sends to {base_url}/responses instead of /chat/completions.
pub async fn handle(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    // Use the dedicated OpenAIResponsesProvider for URL construction
    let provider: Arc<dyn crate::provider_trait::LlmProvider> =
        Arc::new(crate::providers::openai_responses::OpenAIResponsesProvider);

    handle_path_routed_request(
        state,
        auth,
        body,
        PathRouteConfig {
            registry_key: "openai".to_string(),
            provider_override: Some(provider),
            streaming_format: ResponseFormat::OpenAISSE,
            not_found_message: "OpenAI provider not configured".to_string(),
        },
    )
    .await
}
