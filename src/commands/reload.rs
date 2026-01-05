use anyhow::Result;
use colored::Colorize;
use std::path::PathBuf;
use tracing::info;

use crate::pid::PidFile;

#[cfg(unix)]
use llm_gateway::signals::send_signal_to_pid;
#[cfg(unix)]
use tokio::signal::unix::SignalKind;

/// Execute the reload command
///
/// This sends SIGHUP to the running process, which triggers
/// a configuration reload without restarting the server
pub async fn execute(pid_file: Option<PathBuf>) -> Result<()> {
    #[cfg(not(unix))]
    {
        bail!("Reload command is not supported on this platform");
    }

    #[cfg(unix)]
    {
        // Read PID from file
        let pid = PidFile::read(pid_file)?;

        println!(
            "{} {}",
            "Reloading configuration".yellow(),
            format!("(PID: {})", pid).cyan()
        );
        info!("Sending SIGHUP to PID {} for config reload", pid);

        // Send SIGHUP for configuration reload
        send_signal_to_pid(pid, SignalKind::hangup())?;

        println!("{}", "  Reload signal sent successfully".green());
        println!(
            "  {}",
            "Note: Check server logs to verify reload succeeded".dimmed()
        );
        info!("SIGHUP sent to PID {}", pid);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // Note: Full testing of reload requires a running server
    // and is better suited for integration tests
}
