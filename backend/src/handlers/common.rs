use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use crate::provider_trait::{LlmProvider, UpstreamRequest};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

use super::chat_completions::AppState;
use crate::auth::AuthInfo;

/// Response format for path-routed requests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseFormat {
    /// OpenAI SSE format for streaming responses
    OpenAISSE,
    /// Direct passthrough (for providers with non-standard streaming like Bedrock)
    Passthrough,
}

/// Configuration for path-routed requests
pub struct PathRouteConfig {
    /// Registry key to look up provider (e.g., "azure_openai", "bedrock", "custom:deepseek")
    pub registry_key: String,
    /// Optional provider override (used by openai_responses to use a different provider implementation)
    pub provider_override: Option<Arc<dyn LlmProvider>>,
    /// Response format for streaming requests
    pub streaming_format: ResponseFormat,
    /// Error message when provider not found
    pub not_found_message: String,
}

/// Generic handler for path-routed requests.
///
/// This eliminates code duplication across azure.rs, bedrock.rs, custom.rs, and openai_responses.rs.
/// All these handlers share the same logic:
/// 1. Extract and validate model
/// 2. Look up provider in registry
/// 3. Execute with session (sticky session + retry)
/// 4. Return SSE stream or passthrough response
pub async fn handle_path_routed_request(
    state: AppState,
    auth: AuthInfo,
    body: serde_json::Value,
    config: PathRouteConfig,
) -> Result<Response, AppError> {
    let model = extract_and_validate_model(&body)?;
    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Look up provider in registry
    let registry = state.registry.load();
    let registered = registry
        .get(&config.registry_key)
        .ok_or_else(|| AppError::ProviderDisabled(config.not_found_message.clone()))?;
    let load_balancer = registered.load_balancer.clone();

    // Use provider override if provided, otherwise use registry provider
    let provider = config
        .provider_override
        .unwrap_or_else(|| registered.provider.clone());

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
                let oauth_token = resolve_oauth_token(instance.config.as_ref(), &oauth_manager).await?;

                let upstream_req = UpstreamRequest {
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

    // Handle response based on format and streaming
    match (config.streaming_format, is_stream) {
        (ResponseFormat::OpenAISSE, true) => {
            // OpenAI-style SSE streaming
            let sse_stream = crate::streaming::create_openai_sse_stream(response);
            Ok(sse_stream.into_response())
        }
        _ => {
            // Non-streaming or passthrough: return response as-is
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
}

/// Extract and validate model name from request body.
///
/// Applies the same security validation as `router.rs` (length 1-256, charset `[a-zA-Z0-9\-._/:]`).
/// Path-routed handlers bypass ModelRouter, so they need this explicit check.
pub fn extract_and_validate_model(body: &serde_json::Value) -> Result<String, AppError> {
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    if model.is_empty() || model.len() > 256 {
        return Err(AppError::ModelNotFound(
            "Invalid model name: must be between 1 and 256 characters".to_string(),
        ));
    }

    let is_valid = model
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '.' || c == '_' || c == '/' || c == ':');
    if !is_valid {
        return Err(AppError::ModelNotFound(format!(
            "Invalid model name '{}': only alphanumeric characters, hyphens, dots, underscores, slashes, and colons are allowed",
            model
        )));
    }

    Ok(model.to_string())
}

/// Send a request via the provider trait and check the response status.
///
/// This wraps `provider.send_request()` with HTTP status code checking,
/// which is required because the trait intentionally does NOT check status
/// (the retry layer depends on `Err(AppError::UpstreamError)` to trigger failover).
pub async fn send_and_check(
    provider: &dyn LlmProvider,
    client: &reqwest::Client,
    config: &dyn ProviderConfig,
    request: UpstreamRequest,
) -> Result<reqwest::Response, AppError> {
    let response = provider.send_request(client, config, request).await?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AppError::UpstreamError {
            status,
            message: error_text,
        });
    }

    Ok(response)
}

/// Resolve OAuth token for a provider instance config.
///
/// Extracted from handler code to reduce duplication.
pub async fn resolve_oauth_token(
    config: &dyn ProviderConfig,
    oauth_manager: &Option<Arc<crate::oauth::OAuthManager>>,
) -> Result<Option<String>, AppError> {
    if *config.auth_mode() != crate::config::AuthMode::OAuth {
        return Ok(None);
    }

    let oauth_provider_name = config
        .oauth_provider()
        .ok_or_else(|| {
            AppError::ConfigError(
                "OAuth mode enabled but no oauth_provider configured".to_string(),
            )
        })?;

    let manager = oauth_manager
        .as_ref()
        .ok_or_else(|| {
            AppError::ConfigError("OAuth mode enabled but no OAuthManager available".to_string())
        })?;

    match manager.get_valid_token(oauth_provider_name).await {
        Ok(token) => {
            tracing::debug!(
                provider = %oauth_provider_name,
                "Using OAuth token for request"
            );
            Ok(Some(token.access_token))
        }
        Err(e) => Err(AppError::ConfigError(format!(
            "Failed to get OAuth token for '{}': {}",
            oauth_provider_name, e
        ))),
    }
}
