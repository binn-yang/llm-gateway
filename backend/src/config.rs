use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub server: ServerConfig,
    pub api_keys: Vec<ApiKeyConfig>,
    pub routing: RoutingConfig,
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub observability: ObservabilityConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub log_format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiKeyConfig {
    pub key: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RoutingConfig {
    pub rules: HashMap<String, String>,  // prefix -> provider name
    pub default_provider: Option<String>,
    pub discovery: DiscoveryConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub cache_ttl_seconds: u64,
    pub refresh_on_startup: bool,
    pub providers_with_listing: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub openai: Vec<ProviderInstanceConfig>,
    #[serde(default)]
    pub anthropic: Vec<AnthropicInstanceConfig>,
    #[serde(default)]
    pub gemini: Vec<ProviderInstanceConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderInstanceConfig {
    pub name: String,
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,

    #[serde(default = "default_priority")]
    pub priority: u32,

    #[serde(default = "default_failure_timeout")]
    pub failure_timeout_seconds: u64,

    /// Weight for weighted random selection (default: 100)
    /// Higher weight = more likely to be selected
    #[serde(default = "default_weight")]
    pub weight: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnthropicInstanceConfig {
    pub name: String,
    pub enabled: bool,
    pub api_key: String,
    pub base_url: String,
    pub timeout_seconds: u64,
    pub api_version: String,

    #[serde(default = "default_priority")]
    pub priority: u32,

    #[serde(default = "default_failure_timeout")]
    pub failure_timeout_seconds: u64,

    /// Weight for weighted random selection (default: 100)
    /// Higher weight = more likely to be selected
    #[serde(default = "default_weight")]
    pub weight: u32,

    /// Prompt caching configuration
    #[serde(default)]
    pub cache: CacheConfig,
}

/// Prompt caching configuration for Anthropic
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    /// Enable auto-caching for system prompts
    #[serde(default = "default_auto_cache_system")]
    pub auto_cache_system: bool,

    /// Minimum system prompt tokens to trigger caching
    #[serde(default = "default_min_system_tokens")]
    pub min_system_tokens: u64,

    /// Enable auto-caching for tool definitions
    #[serde(default = "default_auto_cache_tools")]
    pub auto_cache_tools: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            auto_cache_system: default_auto_cache_system(),
            min_system_tokens: default_min_system_tokens(),
            auto_cache_tools: default_auto_cache_tools(),
        }
    }
}

fn default_auto_cache_system() -> bool {
    true
}

fn default_min_system_tokens() -> u64 {
    1024
}

fn default_auto_cache_tools() -> bool {
    true
}

fn default_priority() -> u32 {
    1
}

fn default_failure_timeout() -> u64 {
    60
}

fn default_weight() -> u32 {
    100
}

/// Observability configuration (logs, traces, metrics persistence)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityConfig {
    /// Enable observability features (default: false)
    #[serde(default)]
    pub enabled: bool,

    /// SQLite database path (default: "./data/observability.db")
    #[serde(default = "default_database_path")]
    pub database_path: String,

    /// Performance tuning
    #[serde(default)]
    pub performance: ObservabilityPerformanceConfig,

    /// Data retention policies
    #[serde(default)]
    pub retention: ObservabilityRetentionConfig,

    /// Metrics snapshot configuration
    #[serde(default)]
    pub metrics_snapshot: MetricsSnapshotConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            database_path: default_database_path(),
            performance: ObservabilityPerformanceConfig::default(),
            retention: ObservabilityRetentionConfig::default(),
            metrics_snapshot: MetricsSnapshotConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityPerformanceConfig {
    /// Number of log entries per batch (default: 100)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,

    /// Max time before flushing batch in milliseconds (default: 100)
    #[serde(default = "default_flush_interval_ms")]
    pub flush_interval_ms: u64,

    /// Max buffer size for ring buffer (default: 10000)
    #[serde(default = "default_max_buffer_size")]
    pub max_buffer_size: usize,
}

impl Default for ObservabilityPerformanceConfig {
    fn default() -> Self {
        Self {
            batch_size: default_batch_size(),
            flush_interval_ms: default_flush_interval_ms(),
            max_buffer_size: default_max_buffer_size(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObservabilityRetentionConfig {
    /// Logs retention in days (default: 7)
    #[serde(default = "default_logs_days")]
    pub logs_days: u64,

    /// Spans retention in days (default: 7)
    #[serde(default = "default_spans_days")]
    pub spans_days: u64,

    /// Metrics snapshots retention in days (default: 30)
    #[serde(default = "default_metrics_snapshots_days")]
    pub metrics_snapshots_days: u64,

    /// Hour of day to run cleanup (0-23, default: 3 for 3am)
    #[serde(default = "default_cleanup_hour")]
    pub cleanup_hour: u8,
}

impl Default for ObservabilityRetentionConfig {
    fn default() -> Self {
        Self {
            logs_days: default_logs_days(),
            spans_days: default_spans_days(),
            metrics_snapshots_days: default_metrics_snapshots_days(),
            cleanup_hour: default_cleanup_hour(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MetricsSnapshotConfig {
    /// Enable periodic metrics snapshots (default: true)
    #[serde(default = "default_metrics_snapshot_enabled")]
    pub enabled: bool,

    /// Snapshot interval in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_snapshot_interval_seconds")]
    pub interval_seconds: u64,
}

impl Default for MetricsSnapshotConfig {
    fn default() -> Self {
        Self {
            enabled: default_metrics_snapshot_enabled(),
            interval_seconds: default_snapshot_interval_seconds(),
        }
    }
}

// Default value functions for observability config
fn default_database_path() -> String {
    "./data/observability.db".to_string()
}

fn default_batch_size() -> usize {
    100
}

fn default_flush_interval_ms() -> u64 {
    100
}

fn default_max_buffer_size() -> usize {
    10000
}

fn default_logs_days() -> u64 {
    7
}

fn default_spans_days() -> u64 {
    7
}

fn default_metrics_snapshots_days() -> u64 {
    30
}

fn default_cleanup_hour() -> u8 {
    3
}

fn default_metrics_snapshot_enabled() -> bool {
    true
}

fn default_snapshot_interval_seconds() -> u64 {
    300
}

pub fn load_config() -> anyhow::Result<Config> {
    let config = config::Config::builder()
        .add_source(config::File::with_name("config"))
        .add_source(config::Environment::with_prefix("LLM_GATEWAY").separator("__"))
        .build()?;

    let cfg: Config = config.try_deserialize()?;
    validate_config(&cfg)?;

    Ok(cfg)
}

fn validate_config(cfg: &Config) -> anyhow::Result<()> {
    // Validate at least one enabled provider instance exists
    let has_enabled_openai = cfg.providers.openai.iter().any(|p| p.enabled);
    let has_enabled_anthropic = cfg.providers.anthropic.iter().any(|p| p.enabled);
    let has_enabled_gemini = cfg.providers.gemini.iter().any(|p| p.enabled);

    if !has_enabled_openai && !has_enabled_anthropic && !has_enabled_gemini {
        anyhow::bail!("At least one provider instance must be enabled");
    }

    // Validate instance names are unique within each provider type
    validate_unique_instance_names(&cfg.providers.openai, "OpenAI")?;
    validate_unique_instance_names(&cfg.providers.anthropic, "Anthropic")?;
    validate_unique_instance_names(&cfg.providers.gemini, "Gemini")?;

    // Validate instance-specific constraints
    for instance in &cfg.providers.openai {
        validate_instance(instance, "OpenAI")?;
    }
    for instance in &cfg.providers.anthropic {
        validate_anthropic_instance(instance)?;
    }
    for instance in &cfg.providers.gemini {
        validate_instance(instance, "Gemini")?;
    }

    // Validate at least one API key is configured
    if cfg.api_keys.is_empty() {
        anyhow::bail!("At least one API key must be configured");
    }

    // Validate all API keys have names
    for key in &cfg.api_keys {
        if key.name.is_empty() {
            anyhow::bail!("API key name cannot be empty");
        }
    }

    // Validate routing rules reference enabled providers
    for (prefix, provider_name) in &cfg.routing.rules {
        match provider_name.as_str() {
            "openai" => {
                if !has_enabled_openai {
                    anyhow::bail!("Routing rule '{}' uses OpenAI provider, but no OpenAI instances are enabled", prefix);
                }
            }
            "anthropic" => {
                if !has_enabled_anthropic {
                    anyhow::bail!("Routing rule '{}' uses Anthropic provider, but no Anthropic instances are enabled", prefix);
                }
            }
            "gemini" => {
                if !has_enabled_gemini {
                    anyhow::bail!("Routing rule '{}' uses Gemini provider, but no Gemini instances are enabled", prefix);
                }
            }
            _ => anyhow::bail!("Routing rule '{}' has invalid provider: {}", prefix, provider_name),
        }
    }

    // Validate default provider if set
    if let Some(default_provider) = &cfg.routing.default_provider {
        match default_provider.as_str() {
            "openai" => {
                if !has_enabled_openai {
                    anyhow::bail!("Default provider 'openai' has no enabled instances");
                }
            }
            "anthropic" => {
                if !has_enabled_anthropic {
                    anyhow::bail!("Default provider 'anthropic' has no enabled instances");
                }
            }
            "gemini" => {
                if !has_enabled_gemini {
                    anyhow::bail!("Default provider 'gemini' has no enabled instances");
                }
            }
            _ => anyhow::bail!("Invalid default provider: {}", default_provider),
        }
    }

    Ok(())
}

fn validate_unique_instance_names<T>(instances: &[T], provider_name: &str) -> anyhow::Result<()>
where
    T: InstanceName,
{
    let mut names = std::collections::HashSet::new();
    for instance in instances {
        let name = instance.get_name();
        if name.is_empty() {
            anyhow::bail!("{} instance name cannot be empty", provider_name);
        }
        if !names.insert(name) {
            anyhow::bail!("{} instance name '{}' is duplicated", provider_name, name);
        }
    }
    Ok(())
}

fn validate_instance(instance: &ProviderInstanceConfig, provider_name: &str) -> anyhow::Result<()> {
    if instance.priority < 1 {
        anyhow::bail!("{} instance '{}': priority must be >= 1", provider_name, instance.name);
    }
    Ok(())
}

fn validate_anthropic_instance(instance: &AnthropicInstanceConfig) -> anyhow::Result<()> {
    if instance.priority < 1 {
        anyhow::bail!("Anthropic instance '{}': priority must be >= 1", instance.name);
    }
    if instance.api_version.is_empty() {
        anyhow::bail!("Anthropic instance '{}': api_version cannot be empty", instance.name);
    }
    Ok(())
}

trait InstanceName {
    fn get_name(&self) -> &str;
}

impl InstanceName for ProviderInstanceConfig {
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl InstanceName for AnthropicInstanceConfig {
    fn get_name(&self) -> &str {
        &self.name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_config_requires_enabled_provider() {
        let mut cfg = create_test_config();
        // Disable all instances
        for instance in &mut cfg.providers.openai {
            instance.enabled = false;
        }
        for instance in &mut cfg.providers.anthropic {
            instance.enabled = false;
        }
        cfg.providers.gemini.clear();

        let result = validate_config(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("At least one provider instance must be enabled"));
    }

    #[test]
    fn test_validate_config_requires_api_keys() {
        let mut cfg = create_test_config();
        cfg.api_keys.clear();

        let result = validate_config(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("At least one API key must be configured"));
    }

    #[test]
    fn test_validate_unique_instance_names() {
        let mut cfg = create_test_config();
        // Add duplicate instance name
        cfg.providers.openai.push(ProviderInstanceConfig {
            name: "openai-primary".to_string(), // Duplicate name
            enabled: true,
            api_key: "sk-test2".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            timeout_seconds: 300,
            priority: 2,
            failure_timeout_seconds: 60,
            weight: 100,
        });

        let result = validate_config(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicated"));
    }

    fn create_test_config() -> Config {
        let mut routing_rules = HashMap::new();
        routing_rules.insert("gpt-".to_string(), "openai".to_string());

        Config {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                log_level: "info".to_string(),
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
                    enabled: true,
                    cache_ttl_seconds: 3600,
                    refresh_on_startup: true,
                    providers_with_listing: vec!["openai".to_string()],
                },
            },
            providers: ProvidersConfig {
                openai: vec![ProviderInstanceConfig {
                    name: "openai-primary".to_string(),
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-primary".to_string(),
                    enabled: false,
                    api_key: "sk-ant-test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: CacheConfig::default(),
                }],
                gemini: vec![],
            },
            observability: ObservabilityConfig::default(),
        }
    }
}
