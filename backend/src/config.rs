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
    /// OAuth providers configuration for upstream authentication
    #[serde(default)]
    pub oauth_providers: Vec<OAuthProviderConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub log_level: String,
    pub log_format: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_level: "info".to_string(),
            log_format: "json".to_string(),
        }
    }
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

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            rules: HashMap::new(),
            default_provider: Some("openai".to_string()),
            discovery: DiscoveryConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiscoveryConfig {
    pub enabled: bool,
    pub cache_ttl_seconds: u64,
    pub refresh_on_startup: bool,
    pub providers_with_listing: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cache_ttl_seconds: 3600,
            refresh_on_startup: true,
            providers_with_listing: vec!["openai".to_string()],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub openai: Vec<ProviderInstanceConfig>,
    #[serde(default)]
    pub anthropic: Vec<AnthropicInstanceConfig>,
    #[serde(default)]
    pub gemini: Vec<ProviderInstanceConfig>,
    #[serde(default)]
    pub azure_openai: Vec<AzureOpenAIInstanceConfig>,
    #[serde(default)]
    pub bedrock: Vec<BedrockInstanceConfig>,
    #[serde(default)]
    pub custom: Vec<CustomProviderInstanceConfig>,
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            openai: vec![],
            anthropic: vec![],
            gemini: vec![],
            azure_openai: vec![],
            bedrock: vec![],
            custom: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderInstanceConfig {
    pub name: String,
    pub enabled: bool,

    /// Authentication mode: bearer (API key) or oauth
    #[serde(default = "default_auth_mode")]
    pub auth_mode: AuthMode,

    /// API key for bearer mode authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// OAuth provider name for oauth mode authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,

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

    /// Authentication mode: bearer (API key) or oauth
    #[serde(default = "default_auth_mode")]
    pub auth_mode: AuthMode,

    /// API key for bearer mode authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    /// OAuth provider name for oauth mode authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,

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

/// Azure OpenAI instance configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AzureOpenAIInstanceConfig {
    pub name: String,
    pub enabled: bool,

    #[serde(default = "default_auth_mode")]
    pub auth_mode: AuthMode,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,

    /// Azure resource name (e.g. "my-openai-resource")
    pub resource_name: String,

    /// Azure API version (e.g. "2024-02-01")
    pub api_version: String,

    /// Default deployment name (used if model not found in model_deployments)
    #[serde(default)]
    pub deployment_name: Option<String>,

    /// Model name to deployment name mapping
    #[serde(default)]
    pub model_deployments: HashMap<String, String>,

    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    #[serde(default = "default_priority")]
    pub priority: u32,

    #[serde(default = "default_failure_timeout")]
    pub failure_timeout_seconds: u64,

    #[serde(default = "default_weight")]
    pub weight: u32,
}

/// AWS Bedrock instance configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BedrockInstanceConfig {
    pub name: String,
    pub enabled: bool,

    /// AWS region (e.g. "us-east-1")
    pub region: String,

    /// AWS access key ID
    pub access_key_id: String,

    /// AWS secret access key
    pub secret_access_key: String,

    /// AWS session token (for temporary credentials)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_token: Option<String>,

    /// Model name to Bedrock model ID mapping
    /// e.g. "claude-3-5-sonnet" -> "anthropic.claude-3-5-sonnet-20241022-v2:0"
    #[serde(default)]
    pub model_id_mapping: HashMap<String, String>,

    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    #[serde(default = "default_priority")]
    pub priority: u32,

    #[serde(default = "default_failure_timeout")]
    pub failure_timeout_seconds: u64,

    #[serde(default = "default_weight")]
    pub weight: u32,
}

/// Custom OpenAI-compatible provider instance configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomProviderInstanceConfig {
    pub name: String,
    pub enabled: bool,

    /// Unique provider identifier (e.g. "deepseek", "ollama")
    /// Each provider_id gets its own registry entry as "custom:{provider_id}"
    pub provider_id: String,

    #[serde(default = "default_auth_mode")]
    pub auth_mode: AuthMode,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_provider: Option<String>,

    pub base_url: String,

    /// Extra headers to include in every request
    #[serde(default)]
    pub custom_headers: HashMap<String, String>,

    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,

    #[serde(default = "default_priority")]
    pub priority: u32,

    #[serde(default = "default_failure_timeout")]
    pub failure_timeout_seconds: u64,

    #[serde(default = "default_weight")]
    pub weight: u32,
}

fn default_timeout() -> u64 {
    300
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

/// Authentication mode for provider instances
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AuthMode {
    /// Use API key for authentication (default)
    Bearer,
    /// Use OAuth token for authentication
    OAuth,
}

fn default_auth_mode() -> AuthMode {
    AuthMode::Bearer
}

/// OAuth provider configuration for upstream authentication
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OAuthProviderConfig {
    /// Unique name for this OAuth provider
    pub name: String,
    /// OAuth client ID
    pub client_id: String,
    /// OAuth client secret (optional for PKCE flow)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Authorization endpoint URL
    pub auth_url: String,
    /// Token endpoint URL
    pub token_url: String,
    /// Redirect URI for OAuth callback
    pub redirect_uri: String,
    /// OAuth scopes
    pub scopes: Vec<String>,
    /// Custom headers for token exchange requests (optional)
    #[serde(default)]
    pub custom_headers: std::collections::HashMap<String, String>,
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

    /// Body logging configuration (request/response bodies)
    #[serde(default)]
    pub body_logging: BodyLoggingConfig,

    /// Quota refresh configuration
    #[serde(default)]
    pub quota_refresh: QuotaRefreshConfig,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            database_path: default_database_path(),
            performance: ObservabilityPerformanceConfig::default(),
            retention: ObservabilityRetentionConfig::default(),
            metrics_snapshot: MetricsSnapshotConfig::default(),
            body_logging: BodyLoggingConfig::default(),
            quota_refresh: QuotaRefreshConfig::default(),
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

/// Body logging configuration for request/response bodies
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BodyLoggingConfig {
    /// Enable body logging (default: true)
    #[serde(default = "default_body_logging_enabled")]
    pub enabled: bool,

    /// Maximum body size to log in bytes (default: 102400 = 100KB)
    #[serde(default = "default_max_body_size")]
    pub max_body_size: usize,

    /// Log level for body content (default: "info")
    #[serde(default = "default_body_log_level")]
    pub log_level: String,

    /// Redaction patterns for sensitive data
    #[serde(default)]
    pub redact_patterns: Vec<RedactPattern>,

    /// Simple mode: only log user messages and assistant text responses
    /// Excludes: system prompts, tool definitions, images, metadata
    /// (default: false)
    #[serde(default = "default_simple_mode")]
    pub simple_mode: bool,
}

impl Default for BodyLoggingConfig {
    fn default() -> Self {
        Self {
            enabled: default_body_logging_enabled(),
            max_body_size: default_max_body_size(),
            log_level: default_body_log_level(),
            redact_patterns: default_redact_patterns(),
            simple_mode: default_simple_mode(),
        }
    }
}

/// Pattern for redacting sensitive data in logs
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RedactPattern {
    /// Regex pattern to match
    pub pattern: String,
    /// Replacement string
    pub replacement: String,
}

/// Quota refresh configuration for provider instances
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QuotaRefreshConfig {
    /// Enable quota refresh (default: true)
    #[serde(default = "default_quota_enabled")]
    pub enabled: bool,

    /// Refresh interval in seconds (default: 600 = 10 minutes)
    #[serde(default = "default_quota_interval")]
    pub interval_seconds: u64,

    /// Timeout for individual instance queries in seconds (default: 30)
    #[serde(default = "default_quota_timeout")]
    pub timeout_seconds: u64,

    /// Retention days for quota snapshots (default: 7)
    #[serde(default = "default_quota_retention")]
    pub retention_days: i64,
}

impl Default for QuotaRefreshConfig {
    fn default() -> Self {
        Self {
            enabled: default_quota_enabled(),
            interval_seconds: default_quota_interval(),
            timeout_seconds: default_quota_timeout(),
            retention_days: default_quota_retention(),
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

fn default_body_logging_enabled() -> bool {
    true
}

fn default_max_body_size() -> usize {
    102400 // 100KB
}

fn default_body_log_level() -> String {
    "info".to_string()
}

fn default_redact_patterns() -> Vec<RedactPattern> {
    vec![
        RedactPattern {
            pattern: r"sk-[a-zA-Z0-9]{48}".to_string(),
            replacement: "sk-***REDACTED***".to_string(),
        },
        RedactPattern {
            pattern: r"sk-ant-[a-zA-Z0-9-]{95}".to_string(),
            replacement: "sk-ant-***REDACTED***".to_string(),
        },
        RedactPattern {
            pattern: r"Bearer [a-zA-Z0-9._-]+".to_string(),
            replacement: "Bearer ***REDACTED***".to_string(),
        },
    ]
}

fn default_simple_mode() -> bool {
    false
}

fn default_quota_enabled() -> bool {
    true
}

fn default_quota_interval() -> u64 {
    600 // 10 minutes
}

fn default_quota_timeout() -> u64 {
    30
}

fn default_quota_retention() -> i64 {
    7
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
    let has_enabled_azure = cfg.providers.azure_openai.iter().any(|p| p.enabled);
    let has_enabled_bedrock = cfg.providers.bedrock.iter().any(|p| p.enabled);
    let has_enabled_custom = cfg.providers.custom.iter().any(|p| p.enabled);

    if !has_enabled_openai && !has_enabled_anthropic && !has_enabled_gemini
        && !has_enabled_azure && !has_enabled_bedrock && !has_enabled_custom
    {
        anyhow::bail!("At least one provider instance must be enabled");
    }

    // Validate instance names are unique within each provider type
    validate_unique_instance_names(&cfg.providers.openai, "OpenAI")?;
    validate_unique_instance_names(&cfg.providers.anthropic, "Anthropic")?;
    validate_unique_instance_names(&cfg.providers.gemini, "Gemini")?;
    validate_unique_instance_names(&cfg.providers.azure_openai, "Azure OpenAI")?;
    validate_unique_instance_names(&cfg.providers.bedrock, "Bedrock")?;
    validate_unique_instance_names(&cfg.providers.custom, "Custom")?;

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
    for instance in &cfg.providers.azure_openai {
        validate_azure_resource_name(&instance.resource_name)
            .map_err(|e| anyhow::anyhow!("Azure OpenAI instance '{}': {}", instance.name, e))?;
        // Validate auth mode for Azure
        match instance.auth_mode {
            AuthMode::Bearer => {
                if instance.api_key.is_none() {
                    anyhow::bail!(
                        "Azure OpenAI instance '{}': api_key is required for bearer auth mode",
                        instance.name
                    );
                }
            }
            AuthMode::OAuth => {
                if instance.oauth_provider.is_none() {
                    anyhow::bail!(
                        "Azure OpenAI instance '{}': oauth_provider is required for oauth auth mode",
                        instance.name
                    );
                }
            }
        }
        if instance.priority < 1 {
            anyhow::bail!("Azure OpenAI instance '{}': priority must be >= 1", instance.name);
        }
    }
    for instance in &cfg.providers.bedrock {
        validate_aws_region(&instance.region)
            .map_err(|e| anyhow::anyhow!("Bedrock instance '{}': {}", instance.name, e))?;
        if instance.priority < 1 {
            anyhow::bail!("Bedrock instance '{}': priority must be >= 1", instance.name);
        }
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

    // Build set of enabled provider names for routing validation
    let mut enabled_providers = std::collections::HashSet::new();
    if has_enabled_openai { enabled_providers.insert("openai"); }
    if has_enabled_anthropic { enabled_providers.insert("anthropic"); }
    if has_enabled_gemini { enabled_providers.insert("gemini"); }
    if has_enabled_azure { enabled_providers.insert("azure_openai"); }
    if has_enabled_bedrock { enabled_providers.insert("bedrock"); }
    // Custom providers use "custom:{id}" keys
    for custom in &cfg.providers.custom {
        if custom.enabled {
            // Custom providers are valid routing targets
            enabled_providers.insert("custom");
        }
    }

    // Validate routing rules reference enabled providers
    for (prefix, provider_name) in &cfg.routing.rules {
        let base_provider = if provider_name.starts_with("custom:") {
            "custom"
        } else {
            provider_name.as_str()
        };
        if !enabled_providers.contains(base_provider) {
            anyhow::bail!(
                "Routing rule '{}' uses provider '{}', but no instances are enabled",
                prefix, provider_name
            );
        }
    }

    // Validate default provider if set
    if let Some(default_provider) = &cfg.routing.default_provider {
        let base_provider = if default_provider.starts_with("custom:") {
            "custom"
        } else {
            default_provider.as_str()
        };
        if !enabled_providers.contains(base_provider) {
            anyhow::bail!("Default provider '{}' has no enabled instances", default_provider);
        }
    }

    // Validate OAuth provider configurations
    validate_oauth_providers(&cfg)?;

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

    // Validate auth_mode configuration
    match instance.auth_mode {
        AuthMode::Bearer => {
            if instance.api_key.is_none() {
                anyhow::bail!(
                    "{} instance '{}': api_key is required for bearer auth mode",
                    provider_name, instance.name
                );
            }
        }
        AuthMode::OAuth => {
            if instance.oauth_provider.is_none() {
                anyhow::bail!(
                    "{} instance '{}': oauth_provider is required for oauth auth mode",
                    provider_name, instance.name
                );
            }
        }
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

    // Validate auth_mode configuration
    match instance.auth_mode {
        AuthMode::Bearer => {
            if instance.api_key.is_none() {
                anyhow::bail!(
                    "Anthropic instance '{}': api_key is required for bearer auth mode",
                    instance.name
                );
            }
        }
        AuthMode::OAuth => {
            if instance.oauth_provider.is_none() {
                anyhow::bail!(
                    "Anthropic instance '{}': oauth_provider is required for oauth auth mode",
                    instance.name
                );
            }
        }
    }

    Ok(())
}

/// Azure resource name: 仅允许字母数字和连字符，1-63 字符
fn validate_azure_resource_name(name: &str) -> anyhow::Result<()> {
    if name.is_empty() || name.len() > 63 {
        anyhow::bail!("Azure resource_name must be 1-63 characters, got {}", name.len());
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        anyhow::bail!("Azure resource_name contains invalid characters (only alphanumeric and '-' allowed): {}", name);
    }
    Ok(())
}

/// AWS region: 格式如 us-east-1, eu-west-2, ap-southeast-1
fn validate_aws_region(region: &str) -> anyhow::Result<()> {
    if region.is_empty() || region.len() > 25 {
        anyhow::bail!("AWS region length invalid: {}", region);
    }
    if !region.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        anyhow::bail!("AWS region contains invalid characters: {}", region);
    }
    Ok(())
}

fn validate_oauth_providers(cfg: &Config) -> anyhow::Result<()> {
    // Validate OAuth provider names are unique
    let mut oauth_names = std::collections::HashSet::new();
    for oauth_provider in &cfg.oauth_providers {
        if oauth_provider.name.is_empty() {
            anyhow::bail!("OAuth provider name cannot be empty");
        }
        if !oauth_names.insert(&oauth_provider.name) {
            anyhow::bail!("OAuth provider name '{}' is duplicated", oauth_provider.name);
        }
    }

    // Validate that oauth_provider references in instances exist
    let validate_oauth_ref = |oauth_provider: &Option<String>, instance_name: &str, provider_type: &str| {
        if let Some(oauth_name) = oauth_provider {
            if !oauth_names.contains(oauth_name) {
                anyhow::bail!(
                    "{} instance '{}' references non-existent OAuth provider '{}'",
                    provider_type, instance_name, oauth_name
                );
            }
        }
        Ok(())
    };

    for instance in &cfg.providers.openai {
        if instance.auth_mode == AuthMode::OAuth {
            validate_oauth_ref(&instance.oauth_provider, &instance.name, "OpenAI")?;
        }
    }
    for instance in &cfg.providers.anthropic {
        if instance.auth_mode == AuthMode::OAuth {
            validate_oauth_ref(&instance.oauth_provider, &instance.name, "Anthropic")?;
        }
    }
    for instance in &cfg.providers.gemini {
        if instance.auth_mode == AuthMode::OAuth {
            validate_oauth_ref(&instance.oauth_provider, &instance.name, "Gemini")?;
        }
    }
    for instance in &cfg.providers.azure_openai {
        if instance.auth_mode == AuthMode::OAuth {
            validate_oauth_ref(&instance.oauth_provider, &instance.name, "Azure OpenAI")?;
        }
    }
    for instance in &cfg.providers.custom {
        if instance.auth_mode == AuthMode::OAuth {
            validate_oauth_ref(&instance.oauth_provider, &instance.name, "Custom")?;
        }
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

impl InstanceName for AzureOpenAIInstanceConfig {
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl InstanceName for BedrockInstanceConfig {
    fn get_name(&self) -> &str {
        &self.name
    }
}

impl InstanceName for CustomProviderInstanceConfig {
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
            auth_mode: AuthMode::Bearer,
            api_key: Some("sk-test2".to_string()),
            oauth_provider: None,
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
                    auth_mode: AuthMode::Bearer,
                    api_key: Some("sk-test".to_string()),
                    oauth_provider: None,
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-primary".to_string(),
                    enabled: false,
                    auth_mode: AuthMode::Bearer,
                    api_key: Some("sk-ant-test".to_string()),
                    oauth_provider: None,
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: CacheConfig::default(),
                }],
                gemini: vec![],
                azure_openai: vec![],
                bedrock: vec![],
                custom: vec![],
            },
            observability: ObservabilityConfig::default(),
            oauth_providers: vec![],
        }
    }
}
