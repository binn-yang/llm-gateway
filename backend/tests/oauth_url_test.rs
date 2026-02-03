/// 测试 Anthropic OAuth 授权 URL 生成
///
/// 验证:
/// 1. 使用正确的 client_id (9d1c250a-e61b-44d9-88ed-5944d1962f5e)
/// 2. 使用正确的 auth_url (https://claude.ai/oauth/authorize)
/// 3. 包含 code=true 参数
/// 4. Scopes 格式正确（空格分隔）
/// 5. 包含 PKCE 参数

use llm_gateway::{
    config::OAuthProviderConfig,
    oauth::providers::{AnthropicOAuthProvider, traits::OAuthProvider},
};

#[test]
fn test_anthropic_oauth_url_generation() {
    let config = OAuthProviderConfig {
        name: "anthropic".to_string(),
        client_id: "9d1c250a-e61b-44d9-88ed-5944d1962f5e".to_string(),
        client_secret: None,
        auth_url: "https://claude.ai/oauth/authorize".to_string(),
        token_url: "https://console.anthropic.com/v1/oauth/token".to_string(),
        redirect_uri: "https://platform.claude.com/oauth/code/callback".to_string(),
        scopes: vec![
            "org:create_api_key".to_string(),
            "user:profile".to_string(),
            "user:inference".to_string(),
            "user:sessions:claude_code".to_string(),
        ],
        custom_headers: std::collections::HashMap::new(),
    };

    let provider = AnthropicOAuthProvider::new(config.clone());

    // 生成授权 URL
    let code_challenge = "test_challenge";
    let state = "test_state";

    let auth_url = provider
        .get_authorization_url(code_challenge, state)
        .expect("Failed to generate authorization URL");

    println!("Generated auth URL: {}", auth_url);

    // 解析 URL
    let parsed = url::Url::parse(&auth_url).expect("Invalid URL");

    // 验证基础 URL
    assert_eq!(parsed.scheme(), "https");
    assert_eq!(parsed.domain(), Some("claude.ai"));
    assert_eq!(parsed.path(), "/oauth/authorize");

    // 提取查询参数
    let query_params: std::collections::HashMap<_, _> = parsed.query_pairs().collect();

    // 验证必需参数
    assert_eq!(
        query_params.get("client_id"),
        Some(&"9d1c250a-e61b-44d9-88ed-5944d1962f5e".into()),
        "client_id should match official Anthropic OAuth client ID"
    );

    assert_eq!(
        query_params.get("redirect_uri"),
        Some(&"https://platform.claude.com/oauth/code/callback".into()),
        "redirect_uri should match official callback URL"
    );

    assert_eq!(
        query_params.get("response_type"),
        Some(&"code".into()),
        "response_type should be 'code'"
    );

    assert_eq!(
        query_params.get("code_challenge"),
        Some(&code_challenge.into()),
        "code_challenge should be present"
    );

    assert_eq!(
        query_params.get("code_challenge_method"),
        Some(&"S256".into()),
        "code_challenge_method should be 'S256'"
    );

    assert_eq!(
        query_params.get("state"),
        Some(&state.into()),
        "state should match provided value"
    );

    // 验证 code=true 参数 (Anthropic 必需)
    assert_eq!(
        query_params.get("code"),
        Some(&"true".into()),
        "code=true parameter is required by Anthropic"
    );

    // 验证 scopes（空格分隔字符串）
    let scope_str = query_params
        .get("scope")
        .expect("scope parameter should be present");

    let expected_scope = "org:create_api_key user:profile user:inference user:sessions:claude_code";
    assert_eq!(
        scope_str.as_ref(),
        expected_scope,
        "scopes should be space-separated string with all required permissions"
    );

    println!("✓ All URL parameters are correct!");
    println!("✓ client_id: 9d1c250a-e61b-44d9-88ed-5944d1962f5e");
    println!("✓ auth_url: https://claude.ai/oauth/authorize");
    println!("✓ code=true parameter present");
    println!("✓ PKCE parameters correct");
    println!("✓ Scopes: {}", expected_scope);
}

#[test]
fn test_custom_headers_in_config() {
    let mut custom_headers = std::collections::HashMap::new();
    custom_headers.insert("User-Agent".to_string(), "llm-gateway/0.5.0".to_string());

    let config = OAuthProviderConfig {
        name: "anthropic".to_string(),
        client_id: "test-client-id".to_string(),
        client_secret: None,
        auth_url: "https://claude.ai/oauth/authorize".to_string(),
        token_url: "https://console.anthropic.com/v1/oauth/token".to_string(),
        redirect_uri: "http://localhost:54545/callback".to_string(),
        scopes: vec!["api".to_string()],
        custom_headers: custom_headers.clone(),
    };

    // 验证自定义头部存储正确
    assert_eq!(config.custom_headers.len(), 1);
    assert_eq!(
        config.custom_headers.get("User-Agent"),
        Some(&"llm-gateway/0.5.0".to_string())
    );

    println!("✓ Custom headers configuration works correctly");
}
