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
use crate::registry::ProviderRegistry;

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
    registry: Arc<ArcSwap<ProviderRegistry>>,
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
                    if let Err(e) = reload_config(config.clone(), &registry).await {
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
    _registry: Arc<ArcSwap<ProviderRegistry>>,
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

/// Reload configuration atomically with load balancer rebuild
///
/// This loads a new configuration, validates it, rebuilds load balancers,
/// and atomically swaps both config and load balancers.
/// If any step fails, the old configuration remains in place.
async fn reload_config(
    config: Arc<ArcSwap<Config>>,
    registry: &Arc<ArcSwap<ProviderRegistry>>,
) -> Result<()> {
    info!("Loading new configuration...");

    // Phase 1: Load and validate new config
    let new_config = crate::config::load_config()?;

    info!(
        "New configuration loaded. Server: {}:{}, Routing Rules: {}, API Keys: {}",
        new_config.server.host,
        new_config.server.port,
        new_config.routing.rules.len(),
        new_config.api_keys.len()
    );

    // Phase 2: Build new provider registry from new config
    info!("Building new provider registry...");
    let new_registry = crate::server::create_provider_registry(&new_config, None);

    // Phase 3: Validate that each provider has at least one healthy instance
    for (provider_name, registered) in new_registry.iter() {
        let healthy_count = registered.load_balancer.healthy_instance_count().await;
        if healthy_count == 0 {
            bail!(
                "Rejecting reload: Provider {} has no healthy instances (all instances are disabled or unhealthy)",
                provider_name
            );
        }
        info!(
            "Provider {} has {} healthy instance(s)",
            provider_name, healthy_count
        );
    }

    // Phase 3.5: Migrate sessions from old registry to new ones
    info!("Migrating sticky sessions from old load balancers...");
    let old_registry = registry.load();
    let mut total_migrated = 0;
    let mut total_dropped = 0;

    for (provider_name, new_reg) in new_registry.iter() {
        if let Some(old_reg) = old_registry.get(provider_name) {
            let stats = new_reg
                .load_balancer
                .migrate_sessions_from(&old_reg.load_balancer)
                .await;

            total_migrated += stats.migrated;
            total_dropped += stats.total_dropped();

            info!(
                provider = %provider_name,
                total = stats.total_sessions,
                migrated = stats.migrated,
                dropped_expired = stats.dropped_expired,
                dropped_not_found = stats.dropped_instance_not_found,
                dropped_disabled = stats.dropped_instance_disabled,
                dropped_unhealthy = stats.dropped_instance_unhealthy,
                "Session migration completed for provider"
            );
        } else {
            info!(
                provider = %provider_name,
                "No old load balancer found, skipping session migration"
            );
        }
    }

    info!(
        total_migrated = total_migrated,
        total_dropped = total_dropped,
        "Session migration completed for all providers"
    );

    // Phase 4: Atomic swap - both config and registry are updated together
    config.store(Arc::new(new_config));
    registry.store(Arc::new(new_registry));

    info!("Configuration and provider registry swapped atomically");
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
        AnthropicInstanceConfig, ApiKeyConfig, DiscoveryConfig, ProviderInstanceConfig,
        ProvidersConfig, RoutingConfig, ServerConfig,
    };
    use std::collections::HashMap;

    fn create_test_config() -> Config {
        let mut routing_rules = HashMap::new();
        routing_rules.insert("gpt-".to_string(), "openai".to_string());

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
                    name: "openai-test".to_string(),
                    enabled: true,
                    api_key: Some("sk-test".to_string()),
                    base_url: "https://api.openai.com/v1".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                anthropic: vec![AnthropicInstanceConfig {
                    name: "anthropic-test".to_string(),
                    enabled: false,
                    api_key: Some("sk-ant-test".to_string()),
                    base_url: "https://api.anthropic.com/v1".to_string(),
                    timeout_seconds: 300,
                    api_version: "2023-06-01".to_string(),
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    cache: crate::config::CacheConfig::default(),
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                gemini: vec![ProviderInstanceConfig {
                    name: "gemini-test".to_string(),
                    enabled: false,
                    api_key: Some("test".to_string()),
                    base_url: "https://generativelanguage.googleapis.com/v1beta".to_string(),
                    timeout_seconds: 300,
                    priority: 1,
                    failure_timeout_seconds: 60,
                    weight: 100,
                    auth_mode: crate::config::AuthMode::Bearer,
                    oauth_provider: None,
                }],
                azure_openai: vec![],
                bedrock: vec![],
                custom: vec![],
            },
            observability: crate::config::ObservabilityConfig::default(),
            oauth_providers: vec![],
        }
    }

    #[tokio::test]
    async fn test_setup_signal_handlers() {
        let config = Arc::new(ArcSwap::from_pointee(create_test_config()));
        let registry = Arc::new(ArcSwap::from_pointee(ProviderRegistry::new()));
        let (shutdown_tx, _handle) = setup_signal_handlers(config, registry);

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
