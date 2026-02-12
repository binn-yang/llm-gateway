use anyhow::Result;
use colored::Colorize;
use copypasta::{ClipboardContext, ClipboardProvider};
use indicatif::{ProgressBar, ProgressStyle};
use llm_gateway::{
    config::load_config,
    oauth::{
        callback_server::start_callback_server,
        manager::OAuthManager,
        pkce::generate_pkce_params,
        token_store::TokenStore,
    },
};
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::Arc;

/// 尝试将 URL 复制到剪贴板
fn try_copy_to_clipboard(url: &str) -> bool {
    match ClipboardContext::new() {
        Ok(mut ctx) => {
            if let Ok(_) = ctx.set_contents(url.to_owned()) {
                return true;
            }
        }
        Err(_) => {}
    }
    false
}

/// Execute OAuth login command
pub async fn login(provider: String, port: u16) -> Result<()> {
    println!("{}", format!("🔐 OAuth Login - {}", provider).bold());
    println!();

    // Load configuration
    let config = load_config()?;

    // Verify OAuth provider exists in configuration
    let oauth_config = config.oauth_providers.iter()
        .find(|p| p.name == provider)
        .ok_or_else(|| anyhow::anyhow!("OAuth provider '{}' not found in configuration", provider))?;

    // Initialize token store (in data directory)
    let token_store_path = std::path::PathBuf::from("./data/oauth_tokens.json");

    let token_store = Arc::new(TokenStore::new(token_store_path).await?);

    // Initialize OAuth manager
    let oauth_manager = Arc::new(OAuthManager::new(
        config.oauth_providers.clone(),
        token_store.clone(),
    ));

    // Get provider
    let oauth_provider = oauth_manager
        .get_provider(&provider)
        .map_err(|e| anyhow::anyhow!("Failed to get OAuth provider: {}", e))?;

    // Check if redirect_uri is remote (not localhost)
    let is_remote_callback = !oauth_config.redirect_uri.contains("localhost")
        && !oauth_config.redirect_uri.contains("127.0.0.1");

    if is_remote_callback {
        // 使用手动 URL 复制流程
        manual_callback_flow(
            provider,
            oauth_provider,
            token_store
        ).await
    } else {
        // 使用本地 callback server 流程
        local_callback_flow(
            provider,
            oauth_provider,
            token_store,
            port
        ).await
    }
}

/// OAuth login with manual URL copy flow (for remote callbacks)
async fn manual_callback_flow(
    provider: String,
    oauth_provider: &Box<dyn llm_gateway::oauth::providers::traits::OAuthProvider>,
    token_store: Arc<TokenStore>,
) -> Result<()> {
    println!("{} {}", "[1/3]".cyan().bold(), "Generating PKCE parameters...");
    let pkce_params = generate_pkce_params();
    println!("  {} PKCE parameters generated", "✓".green());
    println!();

    println!("{} {}", "[2/3]".cyan().bold(), "Opening browser for authentication...");

    // Generate authorization URL
    let auth_url = oauth_provider
        .get_authorization_url(&pkce_params.code_challenge, &pkce_params.state)
        .map_err(|e| anyhow::anyhow!("Failed to generate authorization URL: {}", e))?;

    // Display authorization URL (browser opening removed for better UX)
    println!();
    println!("{}", "━".repeat(80).bright_black());
    println!("{}", "  Authorization URL".cyan().bold());
    println!("{}", "━".repeat(80).bright_black());
    println!();
    println!("  Please open this URL in your browser:");
    println!();
    println!("  {}", auth_url.green().underline());
    println!();

    // Try to copy URL to clipboard
    if try_copy_to_clipboard(&auth_url) {
        println!("  {} URL copied to clipboard!", "✓".green());
    } else {
        println!("  {} Could not auto-copy to clipboard", "ℹ".dimmed());
    }
    println!();

    println!("{}", "━".repeat(80).bright_black());
    println!();
    println!("{}", "After granting authorization:".bold());
    println!("  1. The browser will redirect to a callback page");
    println!("  2. {} from the browser's address bar", "Copy the COMPLETE URL".yellow().bold());
    println!("  3. Paste it below");
    println!();
    println!("  {} The URL should look like:", "Example:".bright_black());
    println!("    {}", "https://platform.claude.com/oauth/code/callback?code=xxx&state=yyy".bright_black());
    println!();
    println!("{}", "━".repeat(80).bright_black());
    println!();

    // Wait for user to paste callback URL
    print!("{} ", "Paste the callback URL here:".bold());
    io::stdout().flush()?;

    let mut callback_url = String::new();
    io::stdin().read_line(&mut callback_url)?;
    let callback_url = callback_url.trim();

    println!();
    println!("{} {}", "[3/3]".cyan().bold(), "Processing authorization...");

    // Parse callback URL
    let parsed_url = url::Url::parse(callback_url)
        .map_err(|e| anyhow::anyhow!("Invalid URL format: {}", e))?;

    // Validate domain (basic security check)
    if let Some(domain) = parsed_url.domain() {
        if !domain.contains("claude.com") && !domain.contains("anthropic.com") {
            return Err(anyhow::anyhow!(
                "Invalid callback URL domain '{}' - expected claude.com or anthropic.com",
                domain
            ));
        }
    } else {
        return Err(anyhow::anyhow!("Callback URL missing domain"));
    }

    // Extract query parameters
    let query_params: HashMap<_, _> = parsed_url.query_pairs().collect();

    let code = query_params.get("code")
        .ok_or_else(|| anyhow::anyhow!("'code' parameter not found in URL"))?;
    let state_from_url = query_params.get("state")
        .ok_or_else(|| anyhow::anyhow!("'state' parameter not found in URL"))?;

    // Verify state parameter (CSRF protection)
    if state_from_url != &pkce_params.state {
        return Err(anyhow::anyhow!(
            "State parameter mismatch - possible CSRF attack or incorrect URL"
        ));
    }

    println!("  {} Authorization parameters validated", "✓".green());

    // Exchange code for token
    println!("  {} Exchanging code for access token...", "→".cyan());
    let token = oauth_provider
        .exchange_code(code, &pkce_params.code_verifier)
        .await
        .map_err(|e| anyhow::anyhow!("Token exchange failed: {}", e))?;

    println!("  {} Token exchange successful", "✓".green());

    // Save token
    token_store
        .save_token(&provider, &token)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to save token: {}", e))?;

    println!("  {} Token saved to ./data/oauth_tokens.json", "✓".green());
    println!();
    println!("{}", "✓ Authentication successful!".green().bold());
    println!();
    println!("  {} Access token obtained", "✓".green());
    println!("  {} Refresh token saved", "✓".green());
    println!("  {} Token expires at: {}", "✓".green(),
        chrono::DateTime::from_timestamp(token.expires_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    );
    println!();
    println!("You can now use provider instances configured with oauth_provider = \"{}\"", provider);

    Ok(())
}

/// OAuth login with local callback server flow
async fn local_callback_flow(
    provider: String,
    oauth_provider: &Box<dyn llm_gateway::oauth::providers::traits::OAuthProvider>,
    token_store: Arc<TokenStore>,
    port: u16,
) -> Result<()> {
    println!("{} {}", "[1/4]".cyan().bold(), "Starting local callback server...");

    // Start callback server
    let (callback_url, rx) = start_callback_server(port).await?;
    println!("  {} Callback server started: {}", "✓".green(), callback_url);
    println!();

    println!("{} {}", "[2/4]".cyan().bold(), "Generating PKCE parameters...");
    let pkce_params = generate_pkce_params();
    println!("  {} PKCE parameters generated", "✓".green());
    println!();

    println!("{} {}", "[3/4]".cyan().bold(), "Opening browser for authentication...");

    // Generate authorization URL
    let auth_url = oauth_provider
        .get_authorization_url(&pkce_params.code_challenge, &pkce_params.state)
        .map_err(|e| anyhow::anyhow!("Failed to generate authorization URL: {}", e))?;

    // Display authorization URL (browser opening removed for better UX)
    println!();
    println!("{}", "━".repeat(80).bright_black());
    println!("{}", "  Authorization URL".cyan().bold());
    println!("{}", "━".repeat(80).bright_black());
    println!();
    println!("  Please open this URL in your browser:");
    println!();
    println!("  {}", auth_url.green().underline());
    println!();

    // Try to copy URL to clipboard
    if try_copy_to_clipboard(&auth_url) {
        println!("  {} URL copied to clipboard!", "✓".green());
    } else {
        println!("  {} Could not auto-copy to clipboard", "ℹ".dimmed());
    }
    println!();

    println!("{} {}", "[4/4]".cyan().bold(), "Waiting for authorization callback...");

    // Create progress spinner
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .unwrap(),
    );
    spinner.set_message("Waiting for callback...");
    spinner.enable_steady_tick(std::time::Duration::from_millis(100));

    // Wait for callback with timeout
    let callback_result = tokio::time::timeout(
        std::time::Duration::from_secs(300), // 5 minutes timeout
        rx,
    )
    .await;

    spinner.finish_and_clear();

    let auth_response = match callback_result {
        Ok(Ok(Ok(response))) => response,
        Ok(Ok(Err(e))) => {
            return Err(anyhow::anyhow!("Authorization failed: {}", e));
        }
        Ok(Err(_)) => {
            return Err(anyhow::anyhow!("Callback channel closed unexpectedly"));
        }
        Err(_) => {
            return Err(anyhow::anyhow!("Authorization timeout (5 minutes)"));
        }
    };

    // Verify state parameter
    if auth_response.state != pkce_params.state {
        return Err(anyhow::anyhow!("State parameter mismatch - possible CSRF attack"));
    }

    println!("  {} Authorization callback received", "✓".green());
    println!();

    // Exchange code for token
    println!("{}", "Exchanging authorization code for access token...".bold());
    let token = oauth_provider
        .exchange_code(&auth_response.code, &pkce_params.code_verifier)
        .await
        .map_err(|e| anyhow::anyhow!("Token exchange failed: {}", e))?;

    // Save token
    token_store
        .save_token(&provider, &token)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to save token: {}", e))?;

    println!();
    println!("{}", "✓ Authentication successful!".green().bold());
    println!();
    println!("  {} Access token obtained", "✓".green());
    println!("  {} Refresh token saved", "✓".green());
    println!("  {} Token expires at: {}", "✓".green(),
        chrono::DateTime::from_timestamp(token.expires_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    );
    println!();
    println!("You can now use provider instances configured with oauth_provider = \"{}\"", provider);

    Ok(())
}

/// Execute OAuth status command
pub async fn status(provider: Option<String>, verbose: bool) -> Result<()> {
    println!("{}", "🔍 OAuth Token Status".bold());
    println!();

    // Initialize token store (in data directory)
    let token_store_path = std::path::PathBuf::from("./data/oauth_tokens.json");

    let token_store = Arc::new(TokenStore::new(token_store_path).await?);

    // Get providers to check
    let providers_to_check = if let Some(p) = provider {
        vec![p]
    } else {
        token_store.list_providers().await
    };

    if providers_to_check.is_empty() {
        println!("  {} No OAuth tokens found", "ℹ".blue());
        println!();
        println!("  Use {} to authenticate", "llm-gateway oauth login <provider>".cyan());
        return Ok(());
    }

    // Display status for each provider
    for provider_name in providers_to_check {
        match token_store.get_token(&provider_name).await {
            Ok(token) => {
                let now = chrono::Utc::now().timestamp();
                let expires_in = token.expires_at - now;
                let is_valid = expires_in > 0;

                println!("{} {}", "Provider:".bold(), provider_name.cyan());

                if is_valid {
                    println!("  {} {}", "Status:".bold(), "✓ Valid".green());
                } else {
                    println!("  {} {}", "Status:".bold(), "✗ Expired".red());
                }

                println!("  {} {}", "Token Type:".bold(), token.token_type);

                let expires_at = chrono::DateTime::from_timestamp(token.expires_at, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                println!("  {} {}", "Expires At:".bold(), expires_at);

                if is_valid {
                    let hours = expires_in / 3600;
                    let minutes = (expires_in % 3600) / 60;
                    println!("  {} {}h {}m", "Time Remaining:".bold(), hours, minutes);
                } else {
                    println!("  {} Token has expired", "⚠".yellow());
                }

                if verbose {
                    println!("  {} {}", "Scopes:".bold(), token.scope);
                    let created_at = chrono::DateTime::from_timestamp(token.created_at, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "Unknown".to_string());
                    println!("  {} {}", "Created At:".bold(), created_at);
                }

                println!();
            }
            Err(_) => {
                println!("{} {}", "Provider:".bold(), provider_name.cyan());
                println!("  {} No token found", "ℹ".blue());
                println!();
            }
        }
    }

    Ok(())
}

/// Execute OAuth refresh command
pub async fn refresh(provider: String) -> Result<()> {
    println!("{}", format!("🔄 Refreshing OAuth Token - {}", provider).bold());
    println!();

    // Load configuration
    let config = load_config()?;

    // Initialize token store (in data directory)
    let token_store_path = std::path::PathBuf::from("./data/oauth_tokens.json");

    let token_store = Arc::new(TokenStore::new(token_store_path).await?);

    // Initialize OAuth manager
    let oauth_manager = Arc::new(OAuthManager::new(
        config.oauth_providers.clone(),
        token_store.clone(),
    ));

    // Refresh token
    println!("Refreshing token...");
    let new_token = oauth_manager
        .refresh_token(&provider)
        .await
        .map_err(|e| anyhow::anyhow!("Token refresh failed: {}", e))?;

    println!();
    println!("{}", "✓ Token refreshed successfully!".green().bold());
    println!();
    println!("  {} New expiration: {}", "✓".green(),
        chrono::DateTime::from_timestamp(new_token.expires_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "Unknown".to_string())
    );

    Ok(())
}

/// Execute OAuth logout command
pub async fn logout(provider: String) -> Result<()> {
    println!("{}", format!("🚪 OAuth Logout - {}", provider).bold());
    println!();

    // Initialize token store (in data directory)
    let token_store_path = std::path::PathBuf::from("./data/oauth_tokens.json");

    let token_store = Arc::new(TokenStore::new(token_store_path).await?);

    // Delete token
    token_store
        .delete_token(&provider)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to delete token: {}", e))?;

    println!("{}", "✓ Token deleted successfully!".green().bold());
    println!();
    println!("  You have been logged out from {}", provider.cyan());

    Ok(())
}
