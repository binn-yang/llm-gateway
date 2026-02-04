use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use crate::config::{ProviderInstanceConfig, AnthropicInstanceConfig, AuthMode};
use super::{QuotaProvider, QuotaSnapshot};
use crate::oauth::token_store::TokenStore;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct AnthropicUsageResponse {
    usage: UsageData,
}

#[derive(Debug, Deserialize)]
struct UsageData {
    #[serde(rename = "5h")]
    five_hour: UsageWindow,
    #[serde(rename = "7d")]
    seven_day: UsageWindow,
    #[serde(rename = "7d-sonnet")]
    seven_day_sonnet: UsageWindow,
}

#[derive(Debug, Deserialize)]
struct UsageWindow {
    utilization: f64,
    resets_at: String,
}

pub struct AnthropicOAuthQuotaProvider {
    client: Client,
    token_store: Arc<TokenStore>,
}

impl AnthropicOAuthQuotaProvider {
    pub fn new(token_store: Arc<TokenStore>) -> Self {
        Self {
            client: Client::new(),
            token_store,
        }
    }
}

#[async_trait]
impl QuotaProvider for AnthropicOAuthQuotaProvider {
    async fn query_quota(
        &self,
        instance: &ProviderInstanceConfig,
        provider_name: &str,
    ) -> Result<QuotaSnapshot, Box<dyn std::error::Error + Send + Sync>> {
        // 1. 检查是否为 OAuth 模式
        if instance.auth_mode != AuthMode::OAuth {
            return Ok(QuotaSnapshot::unavailable(
                &instance.name,
                provider_name,
                "bearer",
            ));
        }

        // 2. 获取 access token
        let token = match self
            .token_store
            .get_token("anthropic")
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("获取 Anthropic OAuth token 失败: {}", e);
                return Ok(QuotaSnapshot::error(
                    &instance.name,
                    provider_name,
                    "oauth",
                    "OAuth token unavailable".to_string(),
                ));
            }
        };

        // 3. 调用 Anthropic usage API
        let resp = match self
            .client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .header("anthropic-beta", "oauth-2025-04-20")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Anthropic 配额 API 调用失败: {}", e);
                return Ok(QuotaSnapshot::error(
                    &instance.name,
                    provider_name,
                    "oauth",
                    e.to_string(),
                ));
            }
        };

        let status = resp.status();

        // 4. 检查 HTTP 状态
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            tracing::warn!(
                "Anthropic 配额 API 返回错误: {} - {}",
                status,
                error_text
            );
            return Ok(QuotaSnapshot::error(
                &instance.name,
                provider_name,
                "oauth",
                format!("HTTP {}: {}", status, error_text),
            ));
        }

        // 5. 解析响应
        let usage: AnthropicUsageResponse = match resp.json().await {
            Ok(u) => u,
            Err(e) => {
                return Ok(QuotaSnapshot::error(
                    &instance.name,
                    provider_name,
                    "oauth",
                    format!("解析响应失败: {}", e),
                ));
            }
        };

        // 6. 构造 JSON 数据
        let quota_data = serde_json::json!({
            "type": "anthropic_oauth",
            "windows": {
                "five_hour": {
                    "utilization": usage.usage.five_hour.utilization,
                    "resets_at": usage.usage.five_hour.resets_at,
                },
                "seven_day": {
                    "utilization": usage.usage.seven_day.utilization,
                    "resets_at": usage.usage.seven_day.resets_at,
                },
                "seven_day_sonnet": {
                    "utilization": usage.usage.seven_day_sonnet.utilization,
                    "resets_at": usage.usage.seven_day_sonnet.resets_at,
                }
            }
        });

        Ok(QuotaSnapshot::success(
            &instance.name,
            provider_name,
            "oauth",
            quota_data,
        ))
    }

    async fn query_quota_anthropic(
        &self,
        instance: &AnthropicInstanceConfig,
        provider_name: &str,
    ) -> Result<QuotaSnapshot, Box<dyn std::error::Error + Send + Sync>> {
        // 1. 检查是否为 OAuth 模式
        if instance.auth_mode != AuthMode::OAuth {
            return Ok(QuotaSnapshot::unavailable(
                &instance.name,
                provider_name,
                "bearer",
            ));
        }

        // 2. 获取 access token
        let token = match self
            .token_store
            .get_token("anthropic")
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("获取 Anthropic OAuth token 失败: {}", e);
                return Ok(QuotaSnapshot::error(
                    &instance.name,
                    provider_name,
                    "oauth",
                    "OAuth token unavailable".to_string(),
                ));
            }
        };

        // 3. 调用 Anthropic usage API
        let resp = match self
            .client
            .get("https://api.anthropic.com/api/oauth/usage")
            .header("Authorization", format!("Bearer {}", token.access_token))
            .header("anthropic-beta", "oauth-2025-04-20")
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Anthropic 配额 API 调用失败: {}", e);
                return Ok(QuotaSnapshot::error(
                    &instance.name,
                    provider_name,
                    "oauth",
                    e.to_string(),
                ));
            }
        };

        let status = resp.status();

        // 4. 检查 HTTP 状态
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            tracing::warn!(
                "Anthropic 配额 API 返回错误: {} - {}",
                status,
                error_text
            );
            return Ok(QuotaSnapshot::error(
                &instance.name,
                provider_name,
                "oauth",
                format!("HTTP {}: {}", status, error_text),
            ));
        }

        // 5. 解析响应
        let usage: AnthropicUsageResponse = match resp.json().await {
            Ok(u) => u,
            Err(e) => {
                return Ok(QuotaSnapshot::error(
                    &instance.name,
                    provider_name,
                    "oauth",
                    format!("解析响应失败: {}", e),
                ));
            }
        };

        // 6. 构造 JSON 数据
        let quota_data = serde_json::json!({
            "type": "anthropic_oauth",
            "windows": {
                "five_hour": {
                    "utilization": usage.usage.five_hour.utilization,
                    "resets_at": usage.usage.five_hour.resets_at,
                },
                "seven_day": {
                    "utilization": usage.usage.seven_day.utilization,
                    "resets_at": usage.usage.seven_day.resets_at,
                },
                "seven_day_sonnet": {
                    "utilization": usage.usage.seven_day_sonnet.utilization,
                    "resets_at": usage.usage.seven_day_sonnet.resets_at,
                }
            }
        });

        Ok(QuotaSnapshot::success(
            &instance.name,
            provider_name,
            "oauth",
            quota_data,
        ))
    }

    fn supports_quota(&self, instance: &ProviderInstanceConfig) -> bool {
        instance.auth_mode == AuthMode::OAuth
    }

    fn supports_quota_anthropic(&self, instance: &AnthropicInstanceConfig) -> bool {
        instance.auth_mode == AuthMode::OAuth
    }

    fn provider_name(&self) -> &str {
        "anthropic"
    }
}
