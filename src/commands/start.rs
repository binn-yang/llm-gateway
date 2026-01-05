use anyhow::Result;
use colored::Colorize;
use llm_gateway::{config, init_tracing, server};
use std::path::PathBuf;
use tracing::info;

use crate::{daemon, pid::PidFile};

/// Execute the start command
///
/// This will:
/// 1. Optionally daemonize first (before any file I/O on macOS)
/// 2. Load configuration
/// 3. Create PID file
/// 4. Start the server
pub async fn execute(daemon_mode: bool, pid_file: Option<PathBuf>) -> Result<()> {
    if daemon_mode {
        // macOS does not support daemon mode due to tokio runtime limitations
        #[cfg(target_os = "macos")]
        {
            eprintln!("ERROR: Daemon mode is not supported on macOS.");
            eprintln!();
            eprintln!("Reason: Tokio async runtime does not support fork().");
            eprintln!("The I/O driver file descriptors become invalid after fork.");
            eprintln!();
            eprintln!("Recommended solutions:");
            eprintln!("  1. Use launchd to manage the process (recommended):");
            eprintln!("     See examples/llm-gateway.plist and DAEMON.md");
            eprintln!();
            eprintln!("  2. Run in foreground mode (for development):");
            eprintln!("     ./llm-gateway start");
            eprintln!();
            eprintln!("  3. Use screen/tmux for background execution:");
            eprintln!("     screen -dmS llm-gateway ./llm-gateway start");
            eprintln!();
            return Err(anyhow::anyhow!("Daemon mode not supported on macOS"));
        }

        // IMPORTANT: On macOS, we must daemonize BEFORE any complex library calls
        // to avoid fork() crashes with Objective-C runtime
        // Use plain println without colored to avoid extra library initialization
        println!("Starting gateway in daemon mode...");
        println!("  Logs: ./logs/gateway.{{out,err}}.log");

        // Daemonize immediately, before loading config or creating PID file
        daemon::daemonize(daemon::DaemonConfig::default())?;

        // After daemonization, we're in the child process
        // Parent has exited, and stdout/stderr are redirected to log files

        // Initialize tracing AFTER fork() to avoid macOS fork issues
        init_tracing();
    } else {
        println!("{}", "Starting gateway in foreground mode...".green());
    }

    // Now safe to do file I/O after fork (in daemon mode) or without fork (foreground)

    // Load configuration
    let cfg = config::load_config()?;

    if !daemon_mode {
        info!("Starting LLM Gateway in foreground mode");
    } else {
        info!("Starting LLM Gateway in daemon mode");
    }

    // Create PID file to prevent multiple instances
    let _pid_file = PidFile::create(pid_file)?;

    // Start the server (blocks until shutdown)
    server::start_server(cfg).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration testing of start command requires
    // actual server startup and is better suited for integration tests
    // Unit tests here would be minimal
}
