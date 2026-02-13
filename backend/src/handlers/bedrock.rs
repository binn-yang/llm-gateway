use crate::{
    auth::AuthInfo,
    error::AppError,
    handlers::common::{extract_and_validate_model, resolve_oauth_token},
};
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::time::Instant;

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
    let _start = Instant::now();
    let model = extract_and_validate_model(&body)?;
    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let registry = state.registry.load();
    let registered = registry
        .get("bedrock")
        .ok_or_else(|| {
            AppError::ProviderDisabled("Bedrock provider not configured".to_string())
        })?;
    let load_balancer = registered.load_balancer.clone();
    let provider = registered.provider.clone();

    let body_clone = body.clone();
    let http_client = state.http_client.clone();
    let oauth_manager = state.oauth_manager.clone();

    let session_result = crate::retry::execute_with_session(
        load_balancer.as_ref(),
        &auth.api_key_name,
        |instance| {
            let http_client = http_client.clone();
            let body = body_clone.clone();
            let oauth_manager = oauth_manager.clone();
            let provider = provider.clone();
            let model = model.clone();
            async move {
                let oauth_token =
                    resolve_oauth_token(instance.config.as_ref(), &oauth_manager).await?;

                let upstream_req = crate::provider_trait::UpstreamRequest {
                    body,
                    model,
                    stream: is_stream,
                    oauth_token,
                };

                let response = provider
                    .send_request(&http_client, instance.config.as_ref(), upstream_req)
                    .await?;

                let status = response.status();
                if !status.is_success() {
                    let error_text = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(AppError::UpstreamError {
                        status,
                        message: error_text,
                    });
                }

                Ok(response)
            }
        },
    )
    .await?;

    let response = session_result.result?;

    // For now, return response as-is (both streaming and non-streaming)
    // Bedrock streaming uses AWS event stream format, not standard SSE
    let status = response.status();
    let headers = response.headers().clone();
    let body_bytes = response.bytes().await?;
    let mut resp = (status, body_bytes).into_response();
    for (key, value) in headers.iter() {
        if key == "content-type" {
            resp.headers_mut().insert(key, value.clone());
        }
    }
    Ok(resp)
}
