use anyhow::{Context, Result};
use std::path::Path;
use tracing::info;

/// Configuration for daemonization
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DaemonConfig {
    /// Working directory for the daemon process
    pub working_directory: String,

    /// Path to stdout log file
    pub stdout_log: String,

    /// Path to stderr log file
    pub stderr_log: String,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            working_directory: ".".to_string(),
            stdout_log: "./logs/gateway.out.log".to_string(),
            stderr_log: "./logs/gateway.err.log".to_string(),
        }
    }
}

/// Daemonize the current process
///
/// This will:
/// 1. Set macOS fork safety environment variable
/// 2. Create log directories if needed
/// 3. Open log files for stdout/stderr
/// 4. Fork the process and detach from terminal
/// 5. Redirect file descriptors
///
/// After this call, the parent process will exit and the child continues
#[cfg(unix)]
#[allow(dead_code)]
pub fn daemonize(config: DaemonConfig) -> Result<()> {
    use daemonize::Daemonize;

    // On macOS, disable the fork safety check for Objective-C
    // This is necessary because tokio and other libraries initialize
    // Objective-C runtime before fork(), causing crashes
    // This is safe for daemon processes that don't use Objective-C after fork
    #[cfg(target_os = "macos")]
    std::env::set_var("OBJC_DISABLE_INITIALIZE_FORK_SAFETY", "YES");

    info!(
        "Daemonizing with working_dir: {}, stdout: {}, stderr: {}",
        config.working_directory, config.stdout_log, config.stderr_log
    );

    // Create log directory if needed
    if let Some(parent) = Path::new(&config.stdout_log).parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create log directory: {:?}", parent))?;
    }

    // Open stdout log file
    let stdout = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.stdout_log)
        .with_context(|| format!("Failed to open stdout log file: {}", config.stdout_log))?;

    // Open stderr log file
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&config.stderr_log)
        .with_context(|| format!("Failed to open stderr log file: {}", config.stderr_log))?;

    // Perform daemonization
    let daemon = Daemonize::new()
        .working_directory(&config.working_directory)
        .stdout(stdout)
        .stderr(stderr);

    daemon
        .start()
        .context("Failed to daemonize process")?;

    // At this point, we're in the child process
    // The parent has exited

    // Reinitialize tracing since we're in a new process
    // File descriptors have been redirected to log files
    info!("Daemon process started successfully");
    info!("Logs: stdout={}, stderr={}", config.stdout_log, config.stderr_log);

    Ok(())
}

/// Windows placeholder - daemonization not supported
#[cfg(not(unix))]
pub fn daemonize(_config: DaemonConfig) -> Result<()> {
    anyhow::bail!("Daemon mode is not supported on Windows. Run as a Windows Service instead.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default() {
        let config = DaemonConfig::default();
        assert_eq!(config.working_directory, ".");
        assert_eq!(config.stdout_log, "./logs/gateway.out.log");
        assert_eq!(config.stderr_log, "./logs/gateway.err.log");
    }

    #[test]
    fn test_daemon_config_custom() {
        let config = DaemonConfig {
            working_directory: "/var/lib/gateway".to_string(),
            stdout_log: "/var/log/gateway/out.log".to_string(),
            stderr_log: "/var/log/gateway/err.log".to_string(),
        };

        assert_eq!(config.working_directory, "/var/lib/gateway");
        assert_eq!(config.stdout_log, "/var/log/gateway/out.log");
        assert_eq!(config.stderr_log, "/var/log/gateway/err.log");
    }

    // Note: We can't easily test actual daemonization in unit tests
    // as it involves forking and process detachment
    // Integration tests would be needed for full testing
}
