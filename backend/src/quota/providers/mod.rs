pub mod anthropic;

use async_trait::async_trait;
use crate::config::{ProviderInstanceConfig, AnthropicInstanceConfig};
use super::types::QuotaSnapshot;

#[async_trait]
pub trait QuotaProvider: Send + Sync {
    /// 查询配额并返回 JSON 格式数据
    async fn query_quota(
        &self,
        instance: &ProviderInstanceConfig,
        provider_name: &str,
    ) -> Result<QuotaSnapshot, Box<dyn std::error::Error + Send + Sync>>;

    /// 查询配额并返回 JSON 格式数据
    async fn query_quota_anthropic(
        &self,
        _instance: &AnthropicInstanceConfig,
        _provider_name: &str,
    ) -> Result<QuotaSnapshot, Box<dyn std::error::Error + Send + Sync>> {
        // 默认实现：不支持 Anthropic 特定配置
        Err("Anthropic quota not supported".into())
    }

    /// 检查是否支持配额查询
    fn supports_quota(&self, instance: &ProviderInstanceConfig) -> bool;

    /// 检查是否支持配额查询
    fn supports_quota_anthropic(&self, _instance: &AnthropicInstanceConfig) -> bool {
        false
    }

    /// 获取 provider 名称
    fn provider_name(&self) -> &str;
}
