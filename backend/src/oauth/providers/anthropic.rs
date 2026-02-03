use crate::config::OAuthProviderConfig;
use crate::errors::AppError;
use crate::oauth::providers::traits::{OAuthProvider, token_response_to_oauth_token};
use crate::oauth::types::{OAuthToken, OAuthTokenResponse};
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use std::collections::HashMap;

/// Anthropic OAuth provider implementation
pub struct AnthropicOAuthProvider {
    config: OAuthProviderConfig,
    client: Client,
}

impl AnthropicOAuthProvider {
    pub fn new(config: OAuthProviderConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl OAuthProvider for AnthropicOAuthProvider {
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
            .append_pair("scope", &self.config.scopes.join(" "))
            .append_pair("code", "true");  // Anthropic 必需参数

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
