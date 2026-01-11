//! Observability CLI commands
//!
//! Manage observability database (stats, cleanup, etc.)

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use llm_gateway::config;
use llm_gateway::observability::{run_cleanup_now, ObservabilityDb};
use std::sync::Arc;

/// Observability database management
#[derive(Debug, Clone, Parser)]
pub struct ObservabilityArgs {
    #[command(subcommand)]
    pub action: ObservabilityAction,
}

#[derive(Debug, Clone, Parser)]
pub enum ObservabilityAction {
    /// Show database statistics
    Stats,

    /// Run cleanup now (delete old data)
    Cleanup,

    /// Show slow requests
    SlowRequests {
        /// Minimum duration in milliseconds
        #[arg(long, default_value = "5000")]
        threshold: u64,

        /// Maximum number of results
        #[arg(long, default_value = "10")]
        limit: usize,
    },
}

/// Execute observability command
pub async fn execute(args: ObservabilityArgs) -> Result<()> {
    // Load configuration
    let cfg = config::load_config()?;

    if !cfg.observability.enabled {
        eprintln!("{}", "Observability is not enabled in config.toml".red());
        return Ok(());
    }

    // Connect to database
    let db_url = format!("sqlite:{}", cfg.observability.database_path);
    let db = Arc::new(ObservabilityDb::new(&db_url).await?);

    match args.action {
        ObservabilityAction::Stats => {
            show_stats(&db).await?;
        }
        ObservabilityAction::Cleanup => {
            run_cleanup(&db).await?;
        }
        ObservabilityAction::SlowRequests { threshold, limit } => {
            show_slow_requests(&db, threshold, limit).await?;
        }
    }

    Ok(())
}

/// Show database statistics
async fn show_stats(db: &ObservabilityDb) -> Result<()> {
    let stats = db.get_stats().await?;

    println!("{}", "Observability Database Statistics".bold().underline());
    println!();
    println!("{:<30} {:>15}", "Log Entries:", format_number(stats.log_count));
    println!("{:<30} {:>15}", "Trace Spans:", format_number(stats.span_count));
    println!(
        "{:<30} {:>15}",
        "Metrics Snapshots:",
        format_number(stats.metrics_snapshot_count)
    );
    println!();

    // Query additional stats
    let unique_requests = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(DISTINCT request_id) FROM logs WHERE request_id IS NOT NULL"
    )
    .fetch_one(db.pool())
    .await?;

    println!("{:<30} {:>15}", "Unique Requests:", format_number(unique_requests as u64));
    println!();

    // Show recent activity
    let recent_logs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM logs WHERE timestamp > ?",
    )
    .bind((chrono::Utc::now().timestamp_millis() - 3600_000) as i64)
    .fetch_one(db.pool())
    .await?;

    println!("{}", "Recent Activity (Last Hour)".bold());
    println!("{:<30} {:>15}", "  New Logs:", format_number(recent_logs as u64));
    println!();

    Ok(())
}

/// Run cleanup and show results
async fn run_cleanup(db: &ObservabilityDb) -> Result<()> {
    println!("{}", "Running cleanup...".bold());
    println!();

    let stats = run_cleanup_now(db).await?;

    println!("{}", "Cleanup Results:".green().bold());
    println!("{:<30} {:>15}", "  Logs Deleted:", format_number(stats.logs_deleted));
    println!("{:<30} {:>15}", "  Spans Deleted:", format_number(stats.spans_deleted));
    println!(
        "{:<30} {:>15}",
        "  Metrics Snapshots Deleted:",
        format_number(stats.metrics_snapshots_deleted)
    );
    println!();

    if stats.logs_deleted == 0 && stats.spans_deleted == 0 && stats.metrics_snapshots_deleted == 0 {
        println!("{}", "No old data to delete (all data within retention period)".dimmed());
    } else {
        println!("{}", "Cleanup completed successfully!".green().bold());
    }

    Ok(())
}

/// Show slow requests
async fn show_slow_requests(db: &ObservabilityDb, threshold_ms: u64, limit: usize) -> Result<()> {
    println!(
        "{}",
        format!("Slow Requests (>{} ms)", threshold_ms).bold().underline()
    );
    println!();

    let slow_requests = db.query_slow_requests(threshold_ms, limit).await?;

    if slow_requests.is_empty() {
        println!(
            "{}",
            format!("No requests found with duration > {} ms", threshold_ms).yellow()
        );
        return Ok(());
    }

    for (i, req) in slow_requests.iter().enumerate() {
        println!(
            "{}. {} {}",
            i + 1,
            "Request ID:".bold(),
            req.request_id.cyan()
        );
        println!(
            "   {} {}",
            "Duration:".bold(),
            format_duration(req.total_duration_ms)
        );
        println!("   {} {}", "Span Count:".bold(), req.span_count);

        if let Some(ref slowest) = req.slowest_span {
            println!(
                "   {} {} ({} - {:.1}%)",
                "Slowest Span:".bold(),
                slowest.name.blue(),
                format_duration(slowest.duration_ms),
                slowest.percentage
            );
        }

        println!();
    }

    Ok(())
}

/// Format large numbers with commas
fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let mut count = 0;

    for ch in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
        count += 1;
    }

    result.chars().rev().collect()
}

/// Format duration
fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms).green().to_string()
    } else if ms < 10000 {
        format!("{:.2}s", ms as f64 / 1000.0).cyan().to_string()
    } else {
        format!("{:.2}s", ms as f64 / 1000.0).yellow().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }
}
