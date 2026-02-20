//! 安全的日志记录工具
//!
//! 提供敏感信息脱敏功能，确保日志中不会泄露 API keys 等敏感信息。

use regex::Regex;
use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, RwLock};

use crate::config::{BodyLoggingConfig, RedactPattern};

/// Global cache for compiled regex patterns, keyed by pattern string.
static REGEX_CACHE: LazyLock<RwLock<HashMap<String, Regex>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 脱敏后的 API key 表示
///
/// 只显示前 8 个字符，其余替换为 `***`，用于安全地记录日志
#[derive(Clone, Debug)]
pub struct SensitiveApiKey<'a> {
    inner: &'a str,
}

impl<'a> SensitiveApiKey<'a> {
    /// 创建脱敏的 API key 表示
    ///
    /// # 示例
    /// ```
    /// use llm_gateway::logging::SensitiveApiKey;
    ///
    /// let key = "sk-ant-api123-abcdef123456";
    /// let sanitized = SensitiveApiKey::new(key);
    /// assert_eq!(format!("{}", sanitized), "sk-ant-a***");
    /// ```
    pub fn new(key: &'a str) -> Self {
        Self { inner: key }
    }
}

impl<'a> fmt::Display for SensitiveApiKey<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let visible_len = 8.min(self.inner.len());
        if self.inner.len() <= visible_len {
            // 如果 key 太短，全部脱敏
            write!(f, "***")
        } else {
            write!(f, "{}***", &self.inner[..visible_len])
        }
    }
}

/// 检查字符串是否可能是敏感的 API key
///
/// 如果字符串看起来像 API key（以 sk-、pk- 等开头），返回 true
pub fn is_sensitive_key(value: &str) -> bool {
    let sensitive_prefixes = [
        "sk-ant-",
        "sk-",
        "pk-",
        "sess-",
        "acct-",
        "Bearer sk-",
        "Bearer pk-",
    ];

    for prefix in &sensitive_prefixes {
        if value.starts_with(prefix) {
            return true;
        }
    }

    false
}

/// 对字符串进行脱敏处理（如果包含敏感信息）
///
/// # 示例
/// ```
/// use llm_gateway::logging::{sanitize_log_value, SensitiveApiKey};
///
/// // 敏感值会被脱敏
/// assert_eq!(sanitize_log_value("sk-ant-api123-key"), "sk-ant-a***");
///
/// // 普通值不变
/// assert_eq!(sanitize_log_value("my-app-name"), "my-app-name");
/// ```
pub fn sanitize_log_value(value: &str) -> String {
    if is_sensitive_key(value) {
        SensitiveApiKey::new(value).to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensitive_api_key_display() {
        let key = "sk-ant-api123-abcdef123456";
        let sanitized = SensitiveApiKey::new(key);
        // 显示前 8 个字符 + ***
        assert_eq!(format!("{}", sanitized), "sk-ant-a***");
    }

    #[test]
    fn test_sensitive_api_key_short() {
        let key = "sk-abc";
        let sanitized = SensitiveApiKey::new(key);
        assert_eq!(format!("{}", sanitized), "***");
    }

    #[test]
    fn test_is_sensitive_key() {
        // 检测各种 API key 格式
        assert!(is_sensitive_key("sk-ant-api123"));
        assert!(is_sensitive_key("sk-openai123"));
        assert!(is_sensitive_key("pk-test123"));
        assert!(is_sensitive_key("sess-abc123"));
        assert!(is_sensitive_key("Bearer sk-ant-api123"));

        // 普通字符串不是敏感的
        assert!(!is_sensitive_key("my-app-name"));
        assert!(!is_sensitive_key("test-key"));
        assert!(!is_sensitive_key("provider-name"));
    }

    #[test]
    fn test_sanitize_log_value() {
        // 敏感值会被脱敏
        assert_eq!(
            sanitize_log_value("sk-ant-api123-abcdef"),
            "sk-ant-a***"
        );
        assert_eq!(
            sanitize_log_value("sk-openai123"),
            "sk-opena***"
        );

        // 普通值不变
        assert_eq!(sanitize_log_value("my-app"), "my-app");
        assert_eq!(sanitize_log_value("test-provider"), "test-provider");
    }
}

/// Look up or compile a regex from the global cache.
fn get_or_compile_regex(pattern: &str) -> Option<Regex> {
    // Fast path: read lock
    if let Ok(cache) = REGEX_CACHE.read() {
        if let Some(regex) = cache.get(pattern) {
            return Some(regex.clone());
        }
    }
    // Slow path: compile and insert
    match Regex::new(pattern) {
        Ok(regex) => {
            if let Ok(mut cache) = REGEX_CACHE.write() {
                cache.insert(pattern.to_string(), regex.clone());
            }
            Some(regex)
        }
        Err(_) => None,
    }
}

/// Redact sensitive data in body content using configured patterns
///
/// # Arguments
/// * `body` - The body content to redact
/// * `patterns` - List of redaction patterns to apply
///
/// # Returns
/// Redacted body content with sensitive data replaced
pub fn redact_sensitive_data(body: &str, patterns: &[RedactPattern]) -> String {
    let mut redacted = body.to_string();

    for pattern in patterns {
        if let Some(regex) = get_or_compile_regex(&pattern.pattern) {
            redacted = regex.replace_all(&redacted, &pattern.replacement).to_string();
        }
    }

    redacted
}

/// Truncate body content if it exceeds max size (UTF-8 safe)
///
/// # Arguments
/// * `body` - The body content to truncate
/// * `max_size` - Maximum size in bytes
///
/// # Returns
/// Tuple of (truncated_body, was_truncated)
pub fn truncate_body(body: String, max_size: usize) -> (String, bool) {
    if body.len() > max_size {
        let at = body.floor_char_boundary(max_size);
        (body[..at].to_string(), true)
    } else {
        (body, false)
    }
}

/// Log a body string (already serialized) through the unified pipeline.
///
/// Pipeline: enabled check → redact → truncate → tracing::info!
pub fn log_body(
    config: &BodyLoggingConfig,
    span: &tracing::Span,
    event_type: &str,
    body_str: &str,
    streaming: bool,
    chunks_count: usize,
) {
    if !config.enabled {
        return;
    }

    let redacted = redact_sensitive_data(body_str, &config.redact_patterns);
    let (body_content, truncated) = truncate_body(redacted, config.max_body_size);

    tracing::info!(
        parent: span,
        event_type = event_type,
        body = %body_content,
        body_size = body_content.len(),
        truncated = truncated,
        streaming = streaming,
        chunks_count = chunks_count,
        "Body logged"
    );
}

/// Serialize a value to JSON and log it through the unified pipeline.
///
/// Pipeline: enabled check → serialize → redact → truncate → tracing::info!
pub fn log_body_json<T: serde::Serialize>(
    config: &BodyLoggingConfig,
    span: &tracing::Span,
    event_type: &str,
    value: &T,
    streaming: bool,
    chunks_count: usize,
) {
    if !config.enabled {
        return;
    }

    let body_str = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    log_body(config, span, event_type, &body_str, streaming, chunks_count);
}

#[cfg(test)]
mod body_logging_tests {
    use super::*;

    #[test]
    fn test_redact_sensitive_data() {
        let patterns = vec![
            RedactPattern {
                pattern: r"sk-[a-zA-Z0-9]{10}".to_string(),
                replacement: "sk-***REDACTED***".to_string(),
            },
            RedactPattern {
                pattern: r"Bearer [a-zA-Z0-9]+".to_string(),
                replacement: "Bearer ***REDACTED***".to_string(),
            },
        ];

        let body = r#"{"api_key": "sk-abcdefghij", "auth": "Bearer token123"}"#;
        let redacted = redact_sensitive_data(body, &patterns);

        assert!(redacted.contains("sk-***REDACTED***"));
        assert!(redacted.contains("Bearer ***REDACTED***"));
        assert!(!redacted.contains("sk-abcdefghij"));
        assert!(!redacted.contains("token123"));
    }

    #[test]
    fn test_redact_uses_cache() {
        let patterns = vec![RedactPattern {
            pattern: r"secret-\d+".to_string(),
            replacement: "***".to_string(),
        }];

        // First call compiles and caches
        let result1 = redact_sensitive_data("secret-123", &patterns);
        assert_eq!(result1, "***");

        // Second call hits cache
        let result2 = redact_sensitive_data("secret-456", &patterns);
        assert_eq!(result2, "***");

        // Verify it's in the cache
        let cache = REGEX_CACHE.read().unwrap();
        assert!(cache.contains_key(r"secret-\d+"));
    }

    #[test]
    fn test_truncate_body() {
        let body = "a".repeat(1000);

        // No truncation
        let (result, truncated) = truncate_body(body.clone(), 2000);
        assert_eq!(result.len(), 1000);
        assert!(!truncated);

        // Truncation
        let (result, truncated) = truncate_body(body, 500);
        assert_eq!(result.len(), 500);
        assert!(truncated);
    }

    #[test]
    fn test_truncate_body_utf8_safe() {
        // "你好世界" = 12 bytes (3 bytes per char)
        let body = "你好世界".to_string();
        assert_eq!(body.len(), 12);

        // Truncate at 7 bytes: should snap back to 6 (2 full chars "你好")
        let (result, truncated) = truncate_body(body.clone(), 7);
        assert_eq!(result, "你好");
        assert!(truncated);

        // Truncate at 4 bytes: should snap back to 3 (1 full char "你")
        let (result, truncated) = truncate_body(body, 4);
        assert_eq!(result, "你");
        assert!(truncated);
    }
}
