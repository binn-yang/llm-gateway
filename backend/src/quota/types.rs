use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaSnapshot {
    pub provider: String,
    pub instance: String,
    pub auth_mode: String,
    pub status: QuotaStatus,
    pub quota_data: serde_json::Value,
    pub error_message: Option<String>,
}

impl QuotaSnapshot {
    pub fn success(
        instance: &str,
        provider: &str,
        auth_mode: &str,
        quota_data: serde_json::Value,
    ) -> Self {
        Self {
            provider: provider.to_string(),
            instance: instance.to_string(),
            auth_mode: auth_mode.to_string(),
            status: QuotaStatus::Success,
            quota_data,
            error_message: None,
        }
    }

    pub fn error(instance: &str, provider: &str, auth_mode: &str, error: String) -> Self {
        Self {
            provider: provider.to_string(),
            instance: instance.to_string(),
            auth_mode: auth_mode.to_string(),
            status: QuotaStatus::Error,
            quota_data: json!({}),
            error_message: Some(error),
        }
    }

    pub fn unavailable(instance: &str, provider: &str, auth_mode: &str) -> Self {
        Self {
            provider: provider.to_string(),
            instance: instance.to_string(),
            auth_mode: auth_mode.to_string(),
            status: QuotaStatus::Unavailable,
            quota_data: json!({}),
            error_message: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum QuotaStatus {
    Success,
    Error,
    Unavailable,
}

impl QuotaStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuotaStatus::Success => "success",
            QuotaStatus::Error => "error",
            QuotaStatus::Unavailable => "unavailable",
        }
    }
}
