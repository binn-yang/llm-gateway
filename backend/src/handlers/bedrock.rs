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

/// Handle Bedrock path-routed requests: POST /bedrock/v1/messages
///
/// Accepts Anthropic messages format, routes to AWS Bedrock.
/// Note: Bedrock streaming uses AWS event stream format which differs from standard SSE.
/// Non-streaming responses work fully; streaming support may require additional parsing.
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
            registry_key: "bedrock".to_string(),
            provider_override: None,
            streaming_format: ResponseFormat::Passthrough, // Bedrock uses AWS event stream
            not_found_message: "Bedrock provider not configured".to_string(),
        },
    )
    .await
}
