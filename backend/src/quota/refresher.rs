use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use crate::config::Config;
use crate::quota::providers::{QuotaProvider, anthropic::AnthropicOAuthQuotaProvider};
use super::db::QuotaDatabase;
use crate::oauth::token_store::TokenStore;

pub struct QuotaRefresher {
    db: QuotaDatabase,
    providers: Vec<Box<dyn QuotaProvider>>,
    refresh_interval: Duration,
}

impl QuotaRefresher {
    pub fn new(db: QuotaDatabase, config: &Config, token_store: Arc<TokenStore>) -> Self {
        // 注册支持配额查询的 provider
        // 未来可添加: Gemini, OpenAI 等
        let providers: Vec<Box<dyn QuotaProvider>> = vec![
            Box::new(AnthropicOAuthQuotaProvider::new(token_store)),
        ];

        let interval_seconds = config
            .observability
            .quota_refresh
            .interval_seconds;

        Self {
            db,
            providers,
            refresh_interval: Duration::from_secs(interval_seconds),
        }
    }

    /// 启动后台任务
    pub fn spawn(self, config: Arc<Config>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut timer = interval(self.refresh_interval);

            // 立即执行第一次刷新
            if let Err(e) = self.refresh_all_quotas(&config).await {
                tracing::error!("配额刷新失败: {}", e);
            }

            loop {
                timer.tick().await;

                if let Err(e) = self.refresh_all_quotas(&config).await {
                    tracing::error!("配额刷新失败: {}", e);
                }
            }
        })
    }

    /// 刷新所有实例的配额
    async fn refresh_all_quotas(&self, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!("开始刷新配额数据");

        // 遍历所有 provider 实现
        for provider_impl in &self.providers {
            let provider_name = provider_impl.provider_name();

            // 根据 provider 类型遍历实例
            match provider_name {
                "anthropic" => {
                    for instance_config in &config.providers.anthropic {
                        if !instance_config.enabled {
                            continue;
                        }

                        if !provider_impl.supports_quota_anthropic(instance_config) {
                            continue;
                        }

                        // 超时保护
                        let result = tokio::time::timeout(
                            Duration::from_secs(config.observability.quota_refresh.timeout_seconds),
                            provider_impl.query_quota_anthropic(instance_config, provider_name),
                        )
                        .await;

                        let snapshot = match result {
                            Ok(Ok(s)) => s,
                            Ok(Err(e)) => {
                                tracing::error!(
                                    "查询配额失败 [{}/{}]: {}",
                                    provider_name,
                                    instance_config.name,
                                    e
                                );
                                continue;
                            }
                            Err(_) => {
                                tracing::warn!(
                                    "查询配额超时 [{}/{}]",
                                    provider_name,
                                    instance_config.name
                                );
                                continue;
                            }
                        };

                        if let Err(e) = self.db.save_snapshot(&snapshot).await {
                            tracing::error!(
                                "保存配额快照失败 [{}/{}]: {}",
                                provider_name,
                                instance_config.name,
                                e
                            );
                        }
                    }
                }
                "openai" => {
                    for instance_config in &config.providers.openai {
                        if !instance_config.enabled {
                            continue;
                        }

                        if !provider_impl.supports_quota(instance_config) {
                            continue;
                        }

                        // 超时保护
                        let result = tokio::time::timeout(
                            Duration::from_secs(config.observability.quota_refresh.timeout_seconds),
                            provider_impl.query_quota(instance_config, provider_name),
                        )
                        .await;

                        let snapshot = match result {
                            Ok(Ok(s)) => s,
                            Ok(Err(e)) => {
                                tracing::error!(
                                    "查询配额失败 [{}/{}]: {}",
                                    provider_name,
                                    instance_config.name,
                                    e
                                );
                                continue;
                            }
                            Err(_) => {
                                tracing::warn!(
                                    "查询配额超时 [{}/{}]",
                                    provider_name,
                                    instance_config.name
                                );
                                continue;
                            }
                        };

                        if let Err(e) = self.db.save_snapshot(&snapshot).await {
                            tracing::error!(
                                "保存配额快照失败 [{}/{}]: {}",
                                provider_name,
                                instance_config.name,
                                e
                            );
                        }
                    }
                }
                "gemini" => {
                    for instance_config in &config.providers.gemini {
                        if !instance_config.enabled {
                            continue;
                        }

                        if !provider_impl.supports_quota(instance_config) {
                            continue;
                        }

                        // 超时保护
                        let result = tokio::time::timeout(
                            Duration::from_secs(config.observability.quota_refresh.timeout_seconds),
                            provider_impl.query_quota(instance_config, provider_name),
                        )
                        .await;

                        let snapshot = match result {
                            Ok(Ok(s)) => s,
                            Ok(Err(e)) => {
                                tracing::error!(
                                    "查询配额失败 [{}/{}]: {}",
                                    provider_name,
                                    instance_config.name,
                                    e
                                );
                                continue;
                            }
                            Err(_) => {
                                tracing::warn!(
                                    "查询配额超时 [{}/{}]",
                                    provider_name,
                                    instance_config.name
                                );
                                continue;
                            }
                        };

                        if let Err(e) = self.db.save_snapshot(&snapshot).await {
                            tracing::error!(
                                "保存配额快照失败 [{}/{}]: {}",
                                provider_name,
                                instance_config.name,
                                e
                            );
                        }
                    }
                }
                _ => continue,
            }
        }

        tracing::debug!("配额刷新完成");
        Ok(())
    }
}
