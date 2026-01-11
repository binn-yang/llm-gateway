//! Logs query command
//!
//! Query and display observability logs from SQLite database.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use llm_gateway::config;
use llm_gateway::observability::{LogFilter, ObservabilityDb};
use std::sync::Arc;

/// Query and display logs
#[derive(Debug, Clone, Parser)]
pub struct LogsArgs {
    /// Filter by log level (ERROR, WARN, INFO, DEBUG, TRACE)
    #[arg(short, long)]
    pub level: Option<String>,

    /// Filter by request ID
    #[arg(short, long)]
    pub request_id: Option<String>,

    /// Filter by span ID
    #[arg(short, long)]
    pub span_id: Option<String>,

    /// Filter by target (module path)
    #[arg(short, long)]
    pub target: Option<String>,

    /// Grep pattern for message content
    #[arg(short, long)]
    pub grep: Option<String>,

    /// Show logs since N seconds ago (e.g., 3600 for last hour)
    #[arg(long)]
    pub since: Option<u64>,

    /// Maximum number of results
    #[arg(long, default_value = "100")]
    pub limit: usize,

    /// Show oldest first (default: newest first)
    #[arg(long)]
    pub oldest_first: bool,

    /// Output format (text, json)
    #[arg(short = 'f', long, default_value = "text")]
    pub format: String,

    /// Follow logs in real-time (tail -f mode)
    #[arg(long)]
    pub follow: bool,
}

/// Execute the logs command
pub async fn execute(args: LogsArgs) -> Result<()> {
    // Load configuration to get database path
    let cfg = config::load_config()?;

    if !cfg.observability.enabled {
        eprintln!("{}", "Observability is not enabled in config.toml".red());
        eprintln!();
        eprintln!("To enable, add the following to your config.toml:");
        eprintln!("[observability]");
        eprintln!("enabled = true");
        return Ok(());
    }

    // Connect to database
    let db_url = format!("sqlite:{}", cfg.observability.database_path);
    let db = Arc::new(ObservabilityDb::new(&db_url).await?);

    // Build filter
    let filter = LogFilter {
        level: args.level.clone(),
        request_id: args.request_id.clone(),
        span_id: args.span_id.clone(),
        target: args.target.clone(),
        grep: args.grep.clone(),
        since: args.since.map(|s| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            now - (s * 1000)
        }),
        until: None,
        limit: Some(args.limit),
        reverse: !args.oldest_first,
    };

    if args.follow {
        // Follow mode (tail -f)
        follow_logs(db, filter).await?;
    } else {
        // One-time query
        let logs = db.query_logs(filter).await?;

        if logs.is_empty() {
            println!("{}", "No logs found matching the criteria".yellow());
            return Ok(());
        }

        // Display results
        match args.format.as_str() {
            "json" => {
                let json = serde_json::to_string_pretty(&logs)?;
                println!("{}", json);
            }
            "text" | _ => {
                display_logs_text(&logs)?;
            }
        }
    }

    Ok(())
}

/// Display logs in human-friendly text format
fn display_logs_text(logs: &[llm_gateway::observability::LogEntry]) -> Result<()> {
    println!("{}", format!("Found {} log entries", logs.len()).bold());
    println!();

    for log in logs {
        // Format timestamp
        let timestamp = chrono::DateTime::from_timestamp_millis(log.timestamp as i64)
            .unwrap_or_default()
            .format("%Y-%m-%d %H:%M:%S%.3f");

        // Colorize level
        let level_colored = match log.level.as_str() {
            "ERROR" => log.level.red().bold(),
            "WARN" => log.level.yellow().bold(),
            "INFO" => log.level.green(),
            "DEBUG" => log.level.blue(),
            "TRACE" => log.level.normal(),
            _ => log.level.normal(),
        };

        // Format request/span IDs (shortened)
        let request_id_display = log
            .request_id
            .as_ref()
            .map(|id| format!(" req={}", &id[..8.min(id.len())]))
            .unwrap_or_default();

        let span_id_display = log
            .span_id
            .as_ref()
            .map(|id| format!(" span={}", &id[..8.min(id.len())]))
            .unwrap_or_default();

        // Print log entry
        println!(
            "{} {} {}{}{} {}",
            timestamp.to_string().dimmed(),
            level_colored,
            log.target.cyan(),
            request_id_display.dimmed(),
            span_id_display.dimmed(),
            log.message
        );

        // Show fields if not empty
        if log.fields != "{}" && !log.fields.is_empty() {
            if let Ok(fields) = serde_json::from_str::<serde_json::Value>(&log.fields) {
                if let Some(obj) = fields.as_object() {
                    if !obj.is_empty() {
                        println!("  {}", format!("fields: {}", log.fields).dimmed());
                    }
                }
            }
        }
    }

    Ok(())
}

/// Follow logs in real-time (tail -f mode)
async fn follow_logs(db: Arc<ObservabilityDb>, mut filter: LogFilter) -> Result<()> {
    use tokio::time::{interval, Duration};

    println!("{}", "Following logs (Ctrl+C to stop)...".bold());
    println!();

    let mut last_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis() as u64;

    let mut poll_interval = interval(Duration::from_millis(500));

    loop {
        poll_interval.tick().await;

        // Query logs since last check
        filter.since = Some(last_timestamp);
        filter.reverse = false; // Show in chronological order for follow mode

        let logs = db.query_logs(filter.clone()).await?;

        if !logs.is_empty() {
            // Update last timestamp
            if let Some(last_log) = logs.last() {
                last_timestamp = last_log.timestamp;
            }

            // Display new logs
            display_logs_text(&logs)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logs_args_parsing() {
        let args = LogsArgs::parse_from(&["logs", "--level", "ERROR", "--limit", "50"]);
        assert_eq!(args.level, Some("ERROR".to_string()));
        assert_eq!(args.limit, 50);
    }
}
