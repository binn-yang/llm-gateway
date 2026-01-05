use anyhow::{bail, Result};
use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

#[cfg(unix)]
use nix::libc;
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

use crate::config::Config;

/// Shutdown signal types
#[derive(Debug, Clone, Copy)]
pub enum ShutdownSignal {
    /// Graceful shutdown (drain connections, clean up)
    Graceful,
}

/// Setup signal handlers for the server
///
/// Returns a broadcast sender for shutdown signals and a join handle for the signal task
///
/// Handles:
/// - SIGTERM/SIGINT: Graceful shutdown
/// - SIGHUP: Configuration reload
#[cfg(unix)]
pub fn setup_signal_handlers(
    config: Arc<ArcSwap<Config>>,
) -> (
    broadcast::Sender<ShutdownSignal>,
    tokio::task::JoinHandle<()>,
) {
    let (shutdown_tx, _) = broadcast::channel(16);
    let tx_clone = shutdown_tx.clone();

    let handle = tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).expect("Failed to setup SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("Failed to setup SIGINT handler");
        let mut sighup = signal(SignalKind::hangup()).expect("Failed to setup SIGHUP handler");

        loop {
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("SIGTERM received, initiating graceful shutdown");
                    let _ = tx_clone.send(ShutdownSignal::Graceful);
                    break;
                }
                _ = sigint.recv() => {
                    info!("SIGINT received, initiating graceful shutdown");
                    let _ = tx_clone.send(ShutdownSignal::Graceful);
                    break;
                }
                _ = sighup.recv() => {
                    info!("SIGHUP received, reloading configuration");
                    if let Err(e) = reload_config(config.clone()).await {
                        error!("Failed to reload configuration: {}", e);
                    } else {
                        info!("Configuration reloaded successfully");
                    }
                }
            }
        }
    });

    (shutdown_tx, handle)
}

/// Windows placeholder - signals not fully supported
#[cfg(not(unix))]
pub fn setup_signal_handlers(
    _config: Arc<ArcSwap<Config>>,
) -> (
    broadcast::Sender<ShutdownSignal>,
    tokio::task::JoinHandle<()>,
) {
    let (shutdown_tx, _) = broadcast::channel(16);
    let tx_clone = shutdown_tx.clone();

    let handle = tokio::spawn(async move {
        // On Windows, only Ctrl+C is supported
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Ctrl+C received, initiating shutdown");
                let _ = tx_clone.send(ShutdownSignal::Graceful);
            }
            Err(e) => {
                error!("Failed to listen for Ctrl+C: {}", e);
            }
        }
    });

    (shutdown_tx, handle)
}

/// Reload configuration atomically
///
/// This loads a new configuration and atomically swaps it with the current one
/// If loading fails, the old configuration remains in place
async fn reload_config(config: Arc<ArcSwap<Config>>) -> Result<()> {
    info!("Loading new configuration...");

    // Load new config
    let new_config = crate::config::load_config()?;

    info!(
        "New configuration loaded. Server: {}:{}, Models: {}, API Keys: {}",
        new_config.server.host,
        new_config.server.port,
        new_config.models.len(),
        new_config.api_keys.len()
    );

    // Atomic swap - all readers will see the new config immediately
    config.store(Arc::new(new_config));

    info!("Configuration swap completed");
    Ok(())
}

/// Send a signal to a process by PID (for stop/reload commands)
#[cfg(unix)]
pub fn send_signal_to_pid(pid: u32, signal_kind: SignalKind) -> Result<()> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    let nix_signal = match signal_kind.as_raw_value() {
        libc::SIGTERM => Signal::SIGTERM,
        libc::SIGHUP => Signal::SIGHUP,
        libc::SIGINT => Signal::SIGINT,
        libc::SIGKILL => Signal::SIGKILL,
        libc::SIGCONT => Signal::SIGCONT,
        _ => bail!("Unsupported signal: {:?}", signal_kind),
    };

    info!("Sending signal {:?} to PID {}", nix_signal, pid);

    kill(Pid::from_raw(pid as i32), nix_signal)
        .map_err(|e| anyhow::anyhow!("Failed to send signal to PID {}: {}", pid, e))?;

    Ok(())
}

/// Windows placeholder
#[cfg(not(unix))]
pub fn send_signal_to_pid(_pid: u32, _signal_kind: ()) -> Result<()> {
    bail!("Signal sending not supported on this platform");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AnthropicConfig, ApiKeyConfig, MetricsConfig, ModelConfig, ProviderConfig,
        ProvidersConfig, ServerConfig,
    };
    use std::collections::HashMap;

    fn create_test_config() -> Config {
        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
                log_level: "info".to_string(),
                log_format: "json".to_string(),
            },
            api_keys: vec![ApiKeyConfig {
                key: "test-key".to_string(),
                name: "test".to_string(),
                enabled: true,
            }],
            models: HashMap::new(),
            providers: ProvidersConfig {
                openai: ProviderConfig {
                    enabled: true,
                    api_key: "sk-test".to_string(),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                },
                anthropic: AnthropicConfig {
                    enabled: false,
                    api_key: "sk-ant-test".to_string(),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                },
                gemini: ProviderConfig {
                    enabled: false,
                    api_key: "test".to_string(),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                },
            },
            metrics: MetricsConfig {
                enabled: true,
                endpoint: "/metrics".to_string(),
                include_api_key_hash: true,
            },
        }
    }

    #[tokio::test]
    async fn test_setup_signal_handlers() {
        let config = Arc::new(ArcSwap::from_pointee(create_test_config()));
        let (shutdown_tx, _handle) = setup_signal_handlers(config);

        // Should be able to subscribe to shutdown signals
        let mut rx = shutdown_tx.subscribe();

        // Send a test signal
        shutdown_tx.send(ShutdownSignal::Graceful).unwrap();

        // Should receive the signal
        let received = rx.recv().await.unwrap();
        matches!(received, ShutdownSignal::Graceful);
    }

    #[cfg(unix)]
    #[test]
    fn test_send_signal_to_current_process() {
        use tokio::signal::unix::SignalKind;

        let pid = std::process::id();

        // Sending SIGCONT to ourselves should work (it's harmless)
        // We can't test SIGTERM as it would kill the test process
        let result = send_signal_to_pid(pid, SignalKind::from_raw(libc::SIGCONT));
        assert!(result.is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_send_signal_to_nonexistent_process() {
        use tokio::signal::unix::SignalKind;

        // PID 999999 very unlikely to exist
        let result = send_signal_to_pid(999999, SignalKind::terminate());
        assert!(result.is_err());
    }

    #[test]
    fn test_shutdown_signal_clone() {
        let signal = ShutdownSignal::Graceful;
        let cloned = signal;
        matches!(cloned, ShutdownSignal::Graceful);
    }
}
