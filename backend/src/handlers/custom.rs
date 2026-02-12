use crate::{
    auth::AuthInfo,
    error::AppError,
    handlers::common::resolve_oauth_token,
    streaming,
};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Extension, Json,
};
use std::time::Instant;

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
    let _start = Instant::now();
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let registry_key = format!("custom:{}", provider_id);

    let registry = state.registry.load();
    let registered = registry
        .get(&registry_key)
        .ok_or_else(|| {
            AppError::ProviderDisabled(format!(
                "Custom provider '{}' not configured",
                provider_id
            ))
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

    if is_stream {
        let sse_stream = streaming::create_openai_sse_stream(response);
        Ok(sse_stream.into_response())
    } else {
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
}
