use llm_gateway::{
    config::{AuthMode, OAuthProviderConfig, ProviderInstanceConfig, AnthropicInstanceConfig},
    oauth::{OAuthManager, TokenStore, OAuthToken},
};
use std::sync::Arc;
use tempfile::TempDir;
use chrono::Utc;

/// Helper function to create a test OAuth token
fn create_test_oauth_token() -> OAuthToken {
    OAuthToken {
        access_token: "test_access_token_12345".to_string(),
        refresh_token: Some("test_refresh_token_67890".to_string()),
        expires_at: Utc::now().timestamp() + 3600, // Expires in 1 hour
        token_type: "Bearer".to_string(),
        scope: "api".to_string(),
        created_at: Utc::now().timestamp(),
        last_refreshed_at: Utc::now().timestamp(),
        organization: None,
        account: None,
        subscription_info: None,
    }
}

/// Helper function to create a test token store
async fn create_test_token_store() -> (Arc<TokenStore>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let storage_path = temp_dir.path().join("oauth_tokens.json");
    let store = Arc::new(TokenStore::new(storage_path).await.unwrap());
    (store, temp_dir)
}

/// Helper function to create a test OAuth manager
fn create_test_oauth_manager(token_store: Arc<TokenStore>) -> Arc<OAuthManager> {
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

    Arc::new(OAuthManager::new(oauth_configs, token_store))
}

#[tokio::test]
async fn test_oauth_mode_provider_configuration() {
    // Test that provider instances can be configured with OAuth mode
    let oauth_provider_config = ProviderInstanceConfig {
        name: "test-oauth-instance".to_string(),
        enabled: true,
        api_key: None, // No API key in OAuth mode
        base_url: "https://api.example.com".to_string(),
        timeout_seconds: 300,
        priority: 1,
        failure_timeout_seconds: 60,
        weight: 100,
        auth_mode: AuthMode::OAuth,
        oauth_provider: Some("test_provider".to_string()),
    };

    assert_eq!(oauth_provider_config.auth_mode, AuthMode::OAuth);
    assert_eq!(oauth_provider_config.oauth_provider.as_deref(), Some("test_provider"));
    assert!(oauth_provider_config.api_key.is_none());
}

#[tokio::test]
async fn test_bearer_mode_provider_configuration() {
    // Test that provider instances can be configured with Bearer mode
    let bearer_provider_config = ProviderInstanceConfig {
        name: "test-bearer-instance".to_string(),
        enabled: true,
        api_key: Some("sk-test-key".to_string()),
        base_url: "https://api.example.com".to_string(),
        timeout_seconds: 300,
        priority: 1,
        failure_timeout_seconds: 60,
        weight: 100,
        auth_mode: AuthMode::Bearer,
        oauth_provider: None,
    };

    assert_eq!(bearer_provider_config.auth_mode, AuthMode::Bearer);
    assert!(bearer_provider_config.oauth_provider.is_none());
    assert!(bearer_provider_config.api_key.is_some());
}

#[tokio::test]
async fn test_mixed_auth_modes() {
    // Test that both OAuth and Bearer modes can coexist
    let oauth_config = ProviderInstanceConfig {
        name: "oauth-instance".to_string(),
        enabled: true,
        api_key: None,
        base_url: "https://api.example.com".to_string(),
        timeout_seconds: 300,
        priority: 1,
        failure_timeout_seconds: 60,
        weight: 100,
        auth_mode: AuthMode::OAuth,
        oauth_provider: Some("oauth_provider".to_string()),
    };

    let bearer_config = ProviderInstanceConfig {
        name: "bearer-instance".to_string(),
        enabled: true,
        api_key: Some("sk-key".to_string()),
        base_url: "https://api.example.com".to_string(),
        timeout_seconds: 300,
        priority: 2,
        failure_timeout_seconds: 60,
        weight: 100,
        auth_mode: AuthMode::Bearer,
        oauth_provider: None,
    };

    // Both configurations are valid
    assert_eq!(oauth_config.auth_mode, AuthMode::OAuth);
    assert_eq!(bearer_config.auth_mode, AuthMode::Bearer);
}

#[tokio::test]
async fn test_oauth_token_lifecycle() {
    let (token_store, _temp_dir) = create_test_token_store().await;
    let token = create_test_oauth_token();

    // Save token
    token_store.save_token("lifecycle_test", &token).await.unwrap();

    // Retrieve token
    let retrieved = token_store.get_token("lifecycle_test").await.unwrap();
    assert_eq!(retrieved.access_token, token.access_token);
    assert_eq!(retrieved.refresh_token, token.refresh_token);

    // List providers
    let providers = token_store.list_providers().await;
    assert!(providers.contains(&"lifecycle_test".to_string()));

    // Delete token
    token_store.delete_token("lifecycle_test").await.unwrap();

    // Verify deletion
    assert!(token_store.get_token("lifecycle_test").await.is_err());
}

#[tokio::test]
async fn test_oauth_manager_with_multiple_providers() {
    let (token_store, _temp_dir) = create_test_token_store().await;

    let oauth_configs = vec![
        OAuthProviderConfig {
            name: "provider1".to_string(),
            client_id: "client1".to_string(),
            client_secret: None,
            auth_url: "https://provider1.com/oauth/authorize".to_string(),
            token_url: "https://provider1.com/oauth/token".to_string(),
            redirect_uri: "http://localhost:54545/callback".to_string(),
            scopes: vec!["api".to_string()],
            custom_headers: std::collections::HashMap::new(),
        },
        OAuthProviderConfig {
            name: "provider2".to_string(),
            client_id: "client2".to_string(),
            client_secret: None,
            auth_url: "https://provider2.com/oauth/authorize".to_string(),
            token_url: "https://provider2.com/oauth/token".to_string(),
            redirect_uri: "http://localhost:54545/callback".to_string(),
            scopes: vec!["api".to_string()],
            custom_headers: std::collections::HashMap::new(),
        }
    ];

    let manager = OAuthManager::new(oauth_configs, token_store);

    // Both providers should be available
    assert!(manager.get_provider("provider1").is_ok());
    assert!(manager.get_provider("provider2").is_ok());
    assert!(manager.get_provider("nonexistent").is_err());
}

#[tokio::test]
async fn test_anthropic_provider_with_oauth() {
    // Test Anthropic-specific provider configuration with OAuth
    let anthropic_oauth_config = AnthropicInstanceConfig {
        name: "anthropic-oauth".to_string(),
        enabled: true,
        api_key: None, // No API key in OAuth mode
        base_url: "https://api.anthropic.com/v1".to_string(),
        timeout_seconds: 300,
        api_version: "2023-06-01".to_string(),
        priority: 1,
        failure_timeout_seconds: 60,
        weight: 100,
        cache: llm_gateway::config::CacheConfig::default(),
        auth_mode: AuthMode::OAuth,
        oauth_provider: Some("anthropic".to_string()),
    };

    assert_eq!(anthropic_oauth_config.auth_mode, AuthMode::OAuth);
    assert_eq!(anthropic_oauth_config.oauth_provider.as_deref(), Some("anthropic"));
    assert!(anthropic_oauth_config.api_key.is_none());
}

#[tokio::test]
async fn test_token_expiration_check() {
    let (token_store, _temp_dir) = create_test_token_store().await;
    let oauth_manager = create_test_oauth_manager(token_store.clone());

    // Create an expired token
    let expired_token = OAuthToken {
        access_token: "expired_token".to_string(),
        refresh_token: Some("refresh_token".to_string()),
        expires_at: Utc::now().timestamp() - 3600, // Expired 1 hour ago
        token_type: "Bearer".to_string(),
        scope: "api".to_string(),
        created_at: Utc::now().timestamp() - 7200,
        last_refreshed_at: Utc::now().timestamp() - 7200,
        organization: None,
        account: None,
        subscription_info: None,
    };

    token_store.save_token("test_provider", &expired_token).await.unwrap();

    // get_valid_token should detect expiration
    // Note: In real implementation, it would trigger refresh, but here we just test detection
    let result = oauth_manager.get_valid_token("test_provider").await;

    // Should fail because token is expired and we can't actually refresh it in test
    // (no real OAuth server to call)
    assert!(result.is_err());
}

#[tokio::test]
async fn test_token_near_expiration() {
    let (token_store, _temp_dir) = create_test_token_store().await;

    // Create a token expiring in 30 seconds (< 1 minute)
    let near_expiry_token = OAuthToken {
        access_token: "near_expiry_token".to_string(),
        refresh_token: Some("refresh_token".to_string()),
        expires_at: Utc::now().timestamp() + 30, // Expires in 30 seconds
        token_type: "Bearer".to_string(),
        scope: "api".to_string(),
        created_at: Utc::now().timestamp(),
        last_refreshed_at: Utc::now().timestamp(),
        organization: None,
        account: None,
        subscription_info: None,
    };

    token_store.save_token("near_expiry_provider", &near_expiry_token).await.unwrap();

    // Token should be retrievable but flagged as near expiration
    let retrieved = token_store.get_token("near_expiry_provider").await.unwrap();
    let now = Utc::now().timestamp();
    let expires_in = retrieved.expires_at - now;

    assert!(expires_in < 60); // Less than 1 minute
    assert!(expires_in > 0);  // But not yet expired
}
