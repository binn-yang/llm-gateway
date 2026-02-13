use std::sync::LazyLock;
use regex::Regex;

/// 匹配 user_{hex}_account__session_{UUID} 格式
static SESSION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"session_([a-f0-9-]{36})$").unwrap()
});

/// 从 metadata.user_id 中提取 session UUID
///
/// Claude Code 客户端发送的 user_id 格式:
/// `user_{64-char-hex}_account__session_{UUID}`
pub fn extract_session_id(user_id: &str) -> Option<&str> {
    SESSION_RE.captures(user_id).map(|c| c.get(1).unwrap().as_str())
}

/// 组合粘性会话键
///
/// 有 session_id 时用 `{api_key_name}:{session_id}` 实现每会话独立路由，
/// 否则回退到仅 `api_key_name`（向后兼容）。
pub fn compose_session_key(api_key_name: &str, session_id: Option<&str>) -> String {
    match session_id {
        Some(sid) => format!("{}:{}", api_key_name, sid),
        None => api_key_name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_session_id_normal() {
        let user_id = "user_01abc234def567890abcdef1234567890abcdef1234567890abcdef12345678_account__session_17cf0fd3-d51b-4b59-977d-b899dafb3022";
        assert_eq!(
            extract_session_id(user_id),
            Some("17cf0fd3-d51b-4b59-977d-b899dafb3022")
        );
    }

    #[test]
    fn test_extract_session_id_short_prefix() {
        let user_id = "user_abc123_account__session_aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
        assert_eq!(
            extract_session_id(user_id),
            Some("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee")
        );
    }

    #[test]
    fn test_extract_session_id_no_session() {
        assert_eq!(extract_session_id("user_abc123_account"), None);
    }

    #[test]
    fn test_extract_session_id_empty() {
        assert_eq!(extract_session_id(""), None);
    }

    #[test]
    fn test_extract_session_id_invalid_uuid() {
        // UUID 太短，不匹配 36 字符
        let user_id = "user_abc_account__session_not-a-uuid";
        assert_eq!(extract_session_id(user_id), None);
    }

    #[test]
    fn test_compose_session_key_with_session() {
        let key = compose_session_key("my-app", Some("17cf0fd3-d51b-4b59-977d-b899dafb3022"));
        assert_eq!(key, "my-app:17cf0fd3-d51b-4b59-977d-b899dafb3022");
    }

    #[test]
    fn test_compose_session_key_without_session() {
        let key = compose_session_key("my-app", None);
        assert_eq!(key, "my-app");
    }
}
