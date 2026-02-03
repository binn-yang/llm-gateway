use llm_gateway::{
    config::{
        ApiKeyConfig, AnthropicInstanceConfig, CacheConfig, Config, DiscoveryConfig,
        MetricsSnapshotConfig, ObservabilityConfig, ObservabilityPerformanceConfig,
        ObservabilityRetentionConfig, ProviderInstanceConfig, ProvidersConfig,
        RoutingConfig, ServerConfig,
    },
};
use std::collections::HashMap;

/// 创建压力测试用的 Config
///
/// # 参数
/// - `mock_openai_url`: OpenAI mock 服务器 URL
/// - `mock_anthropic_url`: Anthropic mock 服务器 URL
/// - `num_instances_per_provider`: 每个 provider 的实例数量
/// - `num_api_keys`: 生成的 API key 数量
pub fn create_stress_test_config(
    mock_openai_url: &str,
    mock_anthropic_url: &str,
    num_instances_per_provider: usize,
    num_api_keys: usize,
) -> Config {
    let mut routing_rules = HashMap::new();
    routing_rules.insert("gpt-".to_string(), "openai".to_string());
    routing_rules.insert("claude-".to_string(), "anthropic".to_string());

    // 生成 API keys
    let api_keys = (0..num_api_keys)
        .map(|i| ApiKeyConfig {
            key: format!("test-key-{:04}", i),
            name: format!("test-{:04}", i),
            enabled: true,
        })
        .collect();

    // 生成 OpenAI 实例
    let openai_instances = (0..num_instances_per_provider)
        .map(|i| ProviderInstanceConfig {
            name: format!("openai-instance-{}", i),
            enabled: true,
            api_key: Some("mock-key".to_string()),
            base_url: mock_openai_url.to_string(),
            timeout_seconds: 30,
            priority: (i / (num_instances_per_provider / 3).max(1)) as u32 + 1,  // 均匀分布优先级
            failure_timeout_seconds: 60,
            weight: 100,
            auth_mode: llm_gateway::config::AuthMode::Bearer,
            oauth_provider: None,
        })
        .collect();

    // 生成 Anthropic 实例
    let anthropic_instances = (0..num_instances_per_provider)
        .map(|i| AnthropicInstanceConfig {
            name: format!("anthropic-instance-{}", i),
            enabled: true,
            api_key: Some("mock-key".to_string()),
            base_url: mock_anthropic_url.to_string(),
            timeout_seconds: 30,
            api_version: "2023-06-01".to_string(),
            priority: (i / (num_instances_per_provider / 3).max(1)) as u32 + 1,
            failure_timeout_seconds: 60,
            weight: 100,
            cache: CacheConfig {
                auto_cache_system: true,
                min_system_tokens: 1024,
                auto_cache_tools: true,
            },
            auth_mode: llm_gateway::config::AuthMode::Bearer,
            oauth_provider: None,
        })
        .collect();

    Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,  // 使用随机可用端口
            log_level: "warn".to_string(),  // 减少日志噪音
            log_format: "json".to_string(),
        },
        api_keys,
        routing: RoutingConfig {
            rules: routing_rules,
            default_provider: Some("openai".to_string()),
            discovery: DiscoveryConfig {
                enabled: false,  // 压力测试时禁用 discovery
                cache_ttl_seconds: 3600,
                refresh_on_startup: false,
                providers_with_listing: vec![],
            },
        },
        providers: ProvidersConfig {
            openai: openai_instances,
            anthropic: anthropic_instances,
            gemini: vec![],
        },
        oauth_providers: vec![],
        observability: ObservabilityConfig {
            enabled: false,  // 压力测试时禁用观测
            database_path: ":memory:".to_string(),
            performance: ObservabilityPerformanceConfig {
                batch_size: 100,
                flush_interval_ms: 100,
                max_buffer_size: 10000,
            },
            retention: ObservabilityRetentionConfig {
                logs_days: 7,
                spans_days: 7,
                metrics_snapshots_days: 30,
                cleanup_hour: 3,
            },
            metrics_snapshot: MetricsSnapshotConfig {
                enabled: false,
                interval_seconds: 300,
            },
        },
    }
}

/// 创建单实例配置(用于基准延迟测试)
pub fn create_single_instance_config(
    mock_openai_url: &str,
) -> Config {
    create_stress_test_config(mock_openai_url, "", 1, 1)
}

/// 创建多实例配置(用于负载均衡测试)
///
/// # 优先级分布
/// - 实例 0: priority 1 (weight 100)
/// - 实例 1: priority 2 (weight 200)
/// - 实例 2: priority 2 (weight 100)
///
/// 预期分布: 25%, 50%, 25%
pub fn create_weighted_instance_config(
    mock_url: &str,
) -> Config {
    let mut routing_rules = HashMap::new();
    routing_rules.insert("gpt-".to_string(), "openai".to_string());

    Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            log_level: "warn".to_string(),
            log_format: "json".to_string(),
        },
        api_keys: vec![ApiKeyConfig {
            key: "test-key".to_string(),
            name: "test".to_string(),
            enabled: true,
        }],
        routing: RoutingConfig {
            rules: routing_rules,
            default_provider: Some("openai".to_string()),
            discovery: DiscoveryConfig {
                enabled: false,
                cache_ttl_seconds: 3600,
                refresh_on_startup: false,
                providers_with_listing: vec![],
            },
        },
        providers: ProvidersConfig {
            openai: vec![
                ProviderInstanceConfig {
                    name: "openai-0".to_string(),
                    enabled: true,
                    api_key: Some("mock-key".to_string()),
                    base_url: format!("{}/instance-0", mock_url),
                    timeout_seconds: 30,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: llm_gateway::config::AuthMode::Bearer,
                    oauth_provider: None,
                },
                ProviderInstanceConfig {
                    name: "openai-1".to_string(),
                    enabled: true,
                    api_key: Some("mock-key".to_string()),
                    base_url: format!("{}/instance-1", mock_url),
                    timeout_seconds: 30,
                    priority: 2,
                    failure_timeout_seconds: 60,
                    weight: 200,
                    auth_mode: llm_gateway::config::AuthMode::Bearer,
                    oauth_provider: None,
                },
                ProviderInstanceConfig {
                    name: "openai-2".to_string(),
                    enabled: true,
                    api_key: Some("mock-key".to_string()),
                    base_url: format!("{}/instance-2", mock_url),
                    timeout_seconds: 30,
                    priority: 2,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: llm_gateway::config::AuthMode::Bearer,
                    oauth_provider: None,
                },
            ],
            anthropic: vec![],
            gemini: vec![],
        },
        oauth_providers: vec![],
        observability: ObservabilityConfig {
            enabled: false,
            database_path: ":memory:".to_string(),
            performance: ObservabilityPerformanceConfig {
                batch_size: 100,
                flush_interval_ms: 100,
                max_buffer_size: 10000,
            },
            retention: ObservabilityRetentionConfig {
                logs_days: 7,
                spans_days: 7,
                metrics_snapshots_days: 30,
                cleanup_hour: 3,
            },
            metrics_snapshot: MetricsSnapshotConfig {
                enabled: false,
                interval_seconds: 300,
            },
        },
    }
}

/// 创建故障转移测试配置
///
/// 包含 primary 和 backup 实例,用于测试故障转移逻辑
pub fn create_failover_config(
    primary_url: &str,
    backup_url: &str,
) -> Config {
    let mut routing_rules = HashMap::new();
    routing_rules.insert("gpt-".to_string(), "openai".to_string());

    Config {
        server: ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            log_level: "warn".to_string(),
            log_format: "json".to_string(),
        },
        api_keys: vec![ApiKeyConfig {
            key: "test-key".to_string(),
            name: "test".to_string(),
            enabled: true,
        }],
        routing: RoutingConfig {
            rules: routing_rules,
            default_provider: Some("openai".to_string()),
            discovery: DiscoveryConfig {
                enabled: false,
                cache_ttl_seconds: 3600,
                refresh_on_startup: false,
                providers_with_listing: vec![],
            },
        },
        providers: ProvidersConfig {
            openai: vec![
                ProviderInstanceConfig {
                    name: "openai-primary".to_string(),
                    enabled: true,
                    api_key: Some("mock-key".to_string()),
                    base_url: primary_url.to_string(),
                    timeout_seconds: 30,
                    priority: 1,  // 高优先级
                    failure_timeout_seconds: 5,  // 快速恢复测试
                    weight: 100,
                    auth_mode: llm_gateway::config::AuthMode::Bearer,
                    oauth_provider: None,
                },
                ProviderInstanceConfig {
                    name: "openai-backup".to_string(),
                    enabled: true,
                    api_key: Some("mock-key".to_string()),
                    base_url: backup_url.to_string(),
                    timeout_seconds: 30,
                    priority: 2,  // 低优先级
                    failure_timeout_seconds: 5,
                    weight: 100,
                    auth_mode: llm_gateway::config::AuthMode::Bearer,
                    oauth_provider: None,
                },
            ],
            anthropic: vec![],
            gemini: vec![],
        },
        oauth_providers: vec![],
        observability: ObservabilityConfig {
            enabled: false,
            database_path: ":memory:".to_string(),
            performance: ObservabilityPerformanceConfig {
                batch_size: 100,
                flush_interval_ms: 100,
                max_buffer_size: 10000,
            },
            retention: ObservabilityRetentionConfig {
                logs_days: 7,
                spans_days: 7,
                metrics_snapshots_days: 30,
                cleanup_hour: 3,
            },
            metrics_snapshot: MetricsSnapshotConfig {
                enabled: false,
                interval_seconds: 300,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_stress_test_config() {
        let config = create_stress_test_config(
            "http://mock-openai",
            "http://mock-anthropic",
            3,
            10,
        );

        assert_eq!(config.api_keys.len(), 10);
        assert_eq!(config.providers.openai.len(), 3);
        assert_eq!(config.providers.anthropic.len(), 3);
        assert_eq!(config.server.port, 0);  // 随机端口
    }

    #[test]
    fn test_create_weighted_instance_config() {
        let config = create_weighted_instance_config("http://mock");

        assert_eq!(config.providers.openai.len(), 3);
        assert_eq!(config.providers.openai[0].priority, 1);
        assert_eq!(config.providers.openai[1].priority, 2);
        assert_eq!(config.providers.openai[1].weight, 200);
    }

    #[test]
    fn test_create_failover_config() {
        let config = create_failover_config("http://primary", "http://backup");

        assert_eq!(config.providers.openai.len(), 2);
        assert_eq!(config.providers.openai[0].priority, 1);
        assert_eq!(config.providers.openai[1].priority, 2);
        assert_eq!(config.providers.openai[0].failure_timeout_seconds, 5);
    }
}
