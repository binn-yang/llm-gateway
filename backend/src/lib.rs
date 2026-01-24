pub mod auth;
pub mod config;
pub mod conversion_warnings;
pub mod converters;
pub mod error;
pub mod handlers;
pub mod image_utils;
pub mod load_balancer;
pub mod logging;
pub mod models;
pub mod observability;
pub mod providers;
pub mod retry;
pub mod router;
pub mod server;
pub mod signals;
pub mod static_files;
pub mod streaming;

use tracing_subscriber::{fmt, prelude::*, EnvFilter, Layer};
use tracing_appender::rolling;
use std::fs;

/// Initialize tracing/logging
pub fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info"));

    // 创建logs目录
    fs::create_dir_all("logs").expect("Failed to create logs directory");

    // 配置文件输出（JSONL格式，按天轮转）
    let file_appender = rolling::daily("logs", "requests");
    let (non_blocking_file, _guard) = tracing_appender::non_blocking(file_appender);

    // 使用 Box::leak 保持 guard 存活，确保日志写入不会被中断
    // 这对于应用生命周期的资源是可以接受的
    Box::leak(Box::new(_guard));

    // 配置控制台输出（保持现有功能）
    let console_layer = fmt::layer()
        .with_target(true)
        .with_filter(filter.clone());

    // 配置文件输出（JSON格式）
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking_file)
        .with_filter(filter);

    // 组合layers
    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init();
}
