use serde::{Deserialize, Serialize};

/// OAuth token with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    /// Access token for API calls
    pub access_token: String,
    /// Refresh token for obtaining new access tokens
    pub refresh_token: Option<String>,
    /// Token expiration timestamp (Unix timestamp)
    pub expires_at: i64,
    /// Token type (usually "Bearer")
    pub token_type: String,
    /// OAuth scopes
    pub scope: String,
    /// Token creation timestamp
    pub created_at: i64,
    /// Last refresh timestamp
    pub last_refreshed_at: i64,
    /// Anthropic-specific: Organization metadata
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub organization: Option<serde_json::Value>,
    /// Anthropic-specific: Account metadata
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub account: Option<serde_json::Value>,
    /// Anthropic-specific: Subscription information
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subscription_info: Option<serde_json::Value>,
}

/// OAuth token response from provider
#[derive(Debug, Clone, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    pub token_type: String,
    pub scope: Option<String>,
    /// Anthropic-specific: Organization metadata
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub organization: Option<serde_json::Value>,
    /// Anthropic-specific: Account metadata
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub account: Option<serde_json::Value>,
    /// Anthropic-specific: Subscription information
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub subscription_info: Option<serde_json::Value>,
}
