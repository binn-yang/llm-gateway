use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use crate::provider_trait::{LlmProvider, UpstreamRequest};
use std::sync::Arc;

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
