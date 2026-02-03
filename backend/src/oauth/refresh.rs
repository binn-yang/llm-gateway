use crate::oauth::manager::OAuthManager;
use crate::oauth::token_store::TokenStore;
use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;

/// Start background task for automatic token refresh
pub fn start_auto_refresh_task(
    token_store: Arc<TokenStore>,
    oauth_manager: Arc<OAuthManager>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes

        loop {
            interval.tick().await;

            // Get all providers with tokens
            let providers = token_store.list_providers().await;

            for provider_name in providers {
                if let Ok(token) = token_store.get_token(&provider_name).await {
                    let now = Utc::now().timestamp();
                    let expires_in = token.expires_at - now;

                    // Refresh if expiring within 10 minutes
                    if expires_in < 600 {
                        match oauth_manager.refresh_token(&provider_name).await {
                            Ok(_) => {
                                tracing::info!(
                                    provider = %provider_name,
                                    "Token auto-refreshed successfully"
                                );
                            }
                            Err(e) => {
                                tracing::error!(
                                    provider = %provider_name,
                                    error = %e,
                                    "Token auto-refresh failed"
                                );
                            }
                        }
                    }
                }
            }
        }
    });
}
