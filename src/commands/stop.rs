use anyhow::{bail, Result};
use colored::Colorize;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::info;

use crate::pid::PidFile;

#[cfg(unix)]
use llm_gateway::signals::send_signal_to_pid;
#[cfg(unix)]
use nix::libc;
#[cfg(unix)]
use tokio::signal::unix::SignalKind;

/// Execute the stop command
///
/// This will:
/// 1. Read PID from PID file
/// 2. Send SIGTERM to the process
/// 3. Wait for graceful shutdown (with timeout)
/// 4. Optionally force kill with SIGKILL
pub async fn execute(pid_file: Option<PathBuf>, force: bool, timeout: u64) -> Result<()> {
    #[cfg(not(unix))]
    {
        bail!("Stop command is not supported on this platform");
    }

    #[cfg(unix)]
    {
        // Read PID from file
        let pid = PidFile::read(pid_file)?;

        println!("{} {}", "Stopping gateway".yellow(), format!("(PID: {})", pid).cyan());
        info!("Sending SIGTERM to PID {}", pid);

        // Send SIGTERM for graceful shutdown
        send_signal_to_pid(pid, SignalKind::terminate())?;

        println!("  Sent SIGTERM, waiting for graceful shutdown...");

        // Wait for process to exit
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(timeout);

        while start.elapsed() < timeout_duration {
            if !is_process_running(pid) {
                println!("{}", "  Gateway stopped successfully".green());
                info!("Gateway stopped successfully");
                return Ok(());
            }
            sleep(Duration::from_millis(500)).await;

            // Print progress dots
            if start.elapsed().as_secs() % 5 == 0 {
                print!(".");
                use std::io::Write;
                std::io::stdout().flush()?;
            }
        }

        println!();

        // Timeout reached
        if force {
            println!("{}", "  Timeout reached, force killing...".red());
            info!("Force killing PID {}", pid);
            send_signal_to_pid(pid, SignalKind::from_raw(libc::SIGKILL))?;
            sleep(Duration::from_secs(1)).await;

            if !is_process_running(pid) {
                println!("{}", "  Gateway force stopped".yellow());
                return Ok(());
            } else {
                bail!("Failed to kill process even with SIGKILL");
            }
        } else {
            bail!(
                "Timeout after {} seconds. Use --force to kill immediately.",
                timeout
            );
        }
    }
}

/// Check if a process is running (Unix-specific)
#[cfg(unix)]
fn is_process_running(pid: u32) -> bool {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    match kill(Pid::from_raw(pid as i32), Signal::SIGCONT) {
        Ok(_) => true,
        Err(nix::errno::Errno::ESRCH) => false, // No such process
        Err(nix::errno::Errno::EPERM) => true,  // Process exists but no permission
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn test_is_process_running() {
        // Current process should be running
        let current_pid = std::process::id();
        assert!(is_process_running(current_pid));

        // PID 1 should exist on Unix (init/systemd)
        assert!(is_process_running(1));

        // Very high PID unlikely to exist
        assert!(!is_process_running(999999));
    }
}
