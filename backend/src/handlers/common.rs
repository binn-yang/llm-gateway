use crate::error::AppError;
use crate::provider_config::ProviderConfig;
use std::sync::Arc;

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
