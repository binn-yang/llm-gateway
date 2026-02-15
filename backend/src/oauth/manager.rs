use crate::config::OAuthProviderConfig;
use crate::errors::AppError;
use crate::oauth::providers::{AnthropicOAuthProvider, GoogleOAuthProvider, OAuthProvider};
use crate::oauth::token_store::TokenStore;
use crate::oauth::types::OAuthToken;
use chrono::Utc;
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// OAuth manager for handling OAuth flows
pub struct OAuthManager {
    providers: HashMap<String, Box<dyn OAuthProvider>>,
    token_store: Arc<TokenStore>,
    refresh_locks: Arc<DashMap<String, Arc<Mutex<()>>>>,
}

impl OAuthManager {
    /// Create a new OAuth manager
    pub fn new(
        oauth_configs: Vec<OAuthProviderConfig>,
        token_store: Arc<TokenStore>,
    ) -> Self {
        let mut providers: HashMap<String, Box<dyn OAuthProvider>> = HashMap::new();

        for config in oauth_configs {
            let provider: Box<dyn OAuthProvider> = match config.name.as_str() {
                "anthropic" => Box::new(AnthropicOAuthProvider::new(config.clone())),
                "gemini-cli" | "antigravity" => Box::new(GoogleOAuthProvider::new(config.clone())),
                _ => {
                    tracing::warn!(
                        provider = %config.name,
                        "Unknown OAuth provider, using default Google implementation"
                    );
                    Box::new(GoogleOAuthProvider::new(config.clone()))
                }
            };
            providers.insert(config.name.clone(), provider);
        }

        Self {
            providers,
            token_store,
            refresh_locks: Arc::new(DashMap::new()),
        }
    }

    /// Get a provider by name
    pub fn get_provider(&self, provider_name: &str) -> Result<&dyn OAuthProvider, AppError> {
        self.providers
            .get(provider_name)
            .map(|b| b.as_ref())
            .ok_or_else(|| AppError::OAuthError {
                message: format!("OAuth provider '{}' not found", provider_name),
            })
    }

    /// Get a valid token, refreshing if necessary
    pub async fn get_valid_token(
        &self,
        provider_name: &str,
    ) -> Result<OAuthToken, AppError> {
        let token = self.token_store.get_token(provider_name).await?;

        let now = Utc::now().timestamp();
        let expires_in = token.expires_at - now;

        // Refresh if expiring within 1 minute
        if expires_in < 60 {
            tracing::info!(
                provider = %provider_name,
                expires_in = %expires_in,
                "Token expiring soon, refreshing"
            );
            return self.refresh_token(provider_name).await;
        }

        Ok(token)
    }

    /// Refresh a token
    pub async fn refresh_token(
        &self,
        provider_name: &str,
    ) -> Result<OAuthToken, AppError> {
        // Get or create refresh lock
        let lock = self.refresh_locks
            .entry(provider_name.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        // Acquire lock to prevent concurrent refreshes
        let _guard = lock.lock().await;

        // Check if token was already refreshed by another task
        if let Ok(token) = self.token_store.get_token(provider_name).await {
            let now = Utc::now().timestamp();
            if token.expires_at - now > 60 {
                return Ok(token);
            }
        }

        // Perform refresh
        let provider = self.get_provider(provider_name)?;
        let old_token = self.token_store.get_token(provider_name).await?;

        let refresh_token = old_token.refresh_token.ok_or_else(|| AppError::OAuthError {
            message: format!("No refresh token available for provider '{}'", provider_name),
        })?;

        let new_token = provider.refresh_token(&refresh_token).await?;

        // Save new token
        self.token_store.save_token(provider_name, &new_token).await?;

        tracing::info!(
            provider = %provider_name,
            old_expires_at = %old_token.expires_at,
            new_expires_at = %new_token.expires_at,
            "Token refreshed successfully"
        );

        Ok(new_token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OAuthProviderConfig;
    use crate::oauth::token_store::TokenStore;
    use tempfile::TempDir;

    async fn create_test_manager() -> (OAuthManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage_path = temp_dir.path().join("test_tokens.json");
        let token_store = Arc::new(TokenStore::new(storage_path).await.unwrap());

        let oauth_configs = vec![
            OAuthProviderConfig {
                name: "test_provider".to_string(),
                client_id: "test_client_id".to_string(),
                client_secret: None,
                auth_url: "https://example.com/oauth/authorize".to_string(),
                token_url: "https://example.com/oauth/token".to_string(),
                redirect_uri: "http://localhost:54545/callback".to_string(),
                scopes: vec!["api".to_string()],
                custom_headers: std::collections::HashMap::new(),
            }
        ];

        let manager = OAuthManager::new(oauth_configs, token_store);
        (manager, temp_dir)
    }

    #[tokio::test]
    async fn test_get_provider() {
        let (manager, _temp_dir) = create_test_manager().await;

        // Should find existing provider
        let result = manager.get_provider("test_provider");
        assert!(result.is_ok());

        // Should not find non-existent provider
        let result = manager.get_provider("nonexistent");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_valid_token_not_found() {
        let (manager, _temp_dir) = create_test_manager().await;

        // Should fail when no token exists
        let result = manager.get_valid_token("test_provider").await;
        assert!(result.is_err());
        if let Err(AppError::OAuthError { message }) = result {
            assert!(message.contains("No token found"));
        }
    }
}
