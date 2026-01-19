//! 安全的日志记录工具
//!
//! 提供敏感信息脱敏功能，确保日志中不会泄露 API keys 等敏感信息。

use std::fmt;

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
    /// assert_eq!(format!("{}", sanitized), "sk-ant-***");
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
/// assert_eq!(sanitize_log_value("sk-ant-api123-key"), "sk-ant-***");
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
