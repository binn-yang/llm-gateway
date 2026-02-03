use crate::errors::AppError;
use crate::oauth::types::{OAuthToken, OAuthTokenResponse};
use async_trait::async_trait;

/// OAuth provider trait for different OAuth implementations
#[async_trait]
pub trait OAuthProvider: Send + Sync {
    /// Get the authorization URL for the OAuth flow
    fn get_authorization_url(
        &self,
        code_challenge: &str,
        state: &str,
    ) -> Result<String, AppError>;

    /// Exchange authorization code for access token
    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthToken, AppError>;

    /// Refresh an access token using refresh token
    async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthToken, AppError>;
}

/// Convert OAuth token response to OAuthToken
pub fn token_response_to_oauth_token(
    response: OAuthTokenResponse,
    now: i64,
) -> OAuthToken {
    OAuthToken {
        access_token: response.access_token,
        refresh_token: response.refresh_token,
        expires_at: now + response.expires_in,
        token_type: response.token_type,
        scope: response.scope.unwrap_or_default(),
        created_at: now,
        last_refreshed_at: now,
        organization: response.organization,
        account: response.account,
        subscription_info: response.subscription_info,
    }
}
