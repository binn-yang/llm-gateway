use crate::config::OAuthProviderConfig;
use crate::errors::AppError;
use crate::oauth::providers::traits::{OAuthProvider, token_response_to_oauth_token};
use crate::oauth::types::{OAuthToken, OAuthTokenResponse};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;

/// Google OAuth provider implementation
/// Supports gemini-cli and antigravity OAuth applications
pub struct GoogleOAuthProvider {
    config: OAuthProviderConfig,
    client: Client,
}

impl GoogleOAuthProvider {
    pub fn new(config: OAuthProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl OAuthProvider for GoogleOAuthProvider {
    fn get_authorization_url(
        &self,
        code_challenge: &str,
        state: &str,
    ) -> Result<String, AppError> {
        let mut url = url::Url::parse(&self.config.auth_url)
            .map_err(|e| AppError::OAuthError {
                message: format!("Invalid auth URL: {}", e),
            })?;

        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("code_challenge", code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", state)
            .append_pair("scope", &self.config.scopes.join(" "));

        Ok(url.to_string())
    }

    async fn exchange_code(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<OAuthToken, AppError> {
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", &self.config.redirect_uri);
        params.insert("client_id", &self.config.client_id);
        params.insert("code_verifier", code_verifier);

        // Add client_secret if provided (required for web applications)
        if let Some(ref secret) = self.config.client_secret {
            params.insert("client_secret", secret);
        }

        let mut request_builder = self
            .client
            .post(&self.config.token_url)
            .form(&params);

        // 应用配置中的自定义请求头
        for (key, value) in &self.config.custom_headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| AppError::OAuthError {
                message: format!("Token exchange request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::OAuthError {
                message: format!("Token exchange failed: {}", error_text),
            });
        }

        let token_response: OAuthTokenResponse = response
            .json()
            .await
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to parse token response: {}", e),
            })?;

        let now = Utc::now().timestamp();
        Ok(token_response_to_oauth_token(token_response, now))
    }

    async fn refresh_token(&self, refresh_token: &str) -> Result<OAuthToken, AppError> {
        let mut params = HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", refresh_token);
        params.insert("client_id", &self.config.client_id);

        // Add client_secret if provided (required for web applications)
        if let Some(ref secret) = self.config.client_secret {
            params.insert("client_secret", secret);
        }

        let mut request_builder = self
            .client
            .post(&self.config.token_url)
            .form(&params);

        // 应用配置中的自定义请求头
        for (key, value) in &self.config.custom_headers {
            request_builder = request_builder.header(key, value);
        }

        let response = request_builder
            .send()
            .await
            .map_err(|e| AppError::OAuthError {
                message: format!("Token refresh request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AppError::OAuthError {
                message: format!("Token refresh failed: {}", error_text),
            });
        }

        let token_response: OAuthTokenResponse = response
            .json()
            .await
            .map_err(|e| AppError::OAuthError {
                message: format!("Failed to parse token response: {}", e),
            })?;

        let now = Utc::now().timestamp();
        Ok(token_response_to_oauth_token(token_response, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_authorization_url() {
        let config = OAuthProviderConfig {
            name: "gemini-cli".to_string(),
            client_id: "test_client_id".to_string(),
            client_secret: None,
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
            token_url: "https://oauth2.googleapis.com/token".to_string(),
            redirect_uri: "https://codeassist.google.com/authcode".to_string(),
            scopes: vec!["https://www.googleapis.com/auth/cloud-platform".to_string()],
            custom_headers: HashMap::new(),
        };

        let provider = GoogleOAuthProvider::new(config);
        let url = provider
            .get_authorization_url("test_challenge", "test_state")
            .unwrap();

        assert!(url.contains("client_id=test_client_id"));
        assert!(url.contains("code_challenge=test_challenge"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("state=test_state"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcloud-platform"));
    }
}
