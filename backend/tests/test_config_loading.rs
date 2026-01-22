use llm_gateway::config_db::load_config;
use sqlx::SqlitePool;

#[tokio::test]
async fn test_load_config_from_empty_db() -> anyhow::Result<()> {
    // Create in-memory database
    let pool = SqlitePool::connect(":memory:").await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Load config (should not fail even with empty database)
    let config = load_config(&pool).await?;

    // Verify defaults
    assert_eq!(config.server.host, "0.0.0.0");
    assert_eq!(config.server.port, 8080);
    assert_eq!(config.api_keys.len(), 0);
    assert_eq!(config.providers.openai.len(), 0);

    // Verify routing config singleton was initialized
    assert_eq!(
        config.routing.default_provider,
        Some("openai".to_string())
    );
    assert!(config.routing.discovery.enabled);

    println!("✓ Successfully loaded config from empty database");
    Ok(())
}

#[tokio::test]
async fn test_load_config_with_data() -> anyhow::Result<()> {
    use sha2::{Digest, Sha256};

    // Create in-memory database
    let pool = SqlitePool::connect(":memory:").await?;

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Insert test API key
    let test_key = "sk-test-123";
    let key_hash = format!("{:x}", Sha256::digest(test_key.as_bytes()));
    sqlx::query(
        r#"
        INSERT INTO api_keys (key_hash, key_prefix, name, enabled)
        VALUES (?, ?, ?, 1)
        "#,
    )
    .bind(&key_hash)
    .bind("sk-test-")
    .bind("test-key")
    .execute(&pool)
    .await?;

    // Insert test routing rule
    sqlx::query(
        r#"
        INSERT INTO routing_rules (prefix, provider, priority, enabled)
        VALUES ('gpt-', 'openai', 1, 1)
        "#,
    )
    .execute(&pool)
    .await?;

    // Insert test provider instance
    sqlx::query(
        r#"
        INSERT INTO provider_instances
        (provider, name, enabled, api_key_encrypted, base_url, timeout_seconds, priority, weight, failure_timeout_seconds)
        VALUES ('openai', 'openai-test', 1, ?, 'https://api.openai.com/v1', 300, 1, 100, 60)
        "#,
    )
    .bind("sk-openai-test-encrypted")
    .execute(&pool)
    .await?;

    // Insert test Anthropic provider with extra_config
    let anthropic_extra_config = serde_json::json!({
        "api_version": "2024-01-01",
        "cache": {
            "auto_cache_system": false,
            "min_system_tokens": 2048,
            "auto_cache_tools": true
        }
    });
    sqlx::query(
        r#"
        INSERT INTO provider_instances
        (provider, name, enabled, api_key_encrypted, base_url, timeout_seconds, priority, weight, failure_timeout_seconds, extra_config)
        VALUES ('anthropic', 'anthropic-test', 1, ?, 'https://api.anthropic.com/v1', 300, 1, 100, 60, ?)
        "#,
    )
    .bind("sk-ant-test-encrypted")
    .bind(anthropic_extra_config.to_string())
    .execute(&pool)
    .await?;

    // Load config
    let config = load_config(&pool).await?;

    // Verify API keys
    assert_eq!(config.api_keys.len(), 1);
    assert_eq!(config.api_keys[0].name, "test-key");
    assert_eq!(config.api_keys[0].key, key_hash);
    assert!(config.api_keys[0].enabled);

    // Verify routing rules
    assert_eq!(config.routing.rules.get("gpt-"), Some(&"openai".to_string()));

    // Verify OpenAI provider
    assert_eq!(config.providers.openai.len(), 1);
    assert_eq!(config.providers.openai[0].name, "openai-test");
    assert!(config.providers.openai[0].enabled);

    // Verify Anthropic provider with extra_config
    assert_eq!(config.providers.anthropic.len(), 1);
    assert_eq!(config.providers.anthropic[0].name, "anthropic-test");
    assert_eq!(config.providers.anthropic[0].api_version, "2024-01-01");
    assert!(!config.providers.anthropic[0].cache.auto_cache_system);
    assert_eq!(config.providers.anthropic[0].cache.min_system_tokens, 2048);
    assert!(config.providers.anthropic[0].cache.auto_cache_tools);

    println!("✓ Successfully loaded config with test data");
    Ok(())
}
