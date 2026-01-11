//! Trace query command
//!
//! Query and visualize request traces with span hierarchy.

use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use llm_gateway::config;
use llm_gateway::observability::{ObservabilityDb, TraceTree};
use std::sync::Arc;

/// Query and display request trace
#[derive(Debug, Clone, Parser)]
pub struct TraceArgs {
    /// Request ID to trace
    pub request_id: String,

    /// Output format (tree, json)
    #[arg(short = 'f', long, default_value = "tree")]
    pub format: String,

    /// Show associated logs
    #[arg(short, long)]
    pub logs: bool,

    /// Show detailed span attributes
    #[arg(short, long)]
    pub verbose: bool,
}

/// Execute the trace command
pub async fn execute(args: TraceArgs) -> Result<()> {
    // Load configuration
    let cfg = config::load_config()?;

    if !cfg.observability.enabled {
        eprintln!("{}", "Observability is not enabled in config.toml".red());
        return Ok(());
    }

    // Connect to database
    let db_url = format!("sqlite:{}", cfg.observability.database_path);
    let db = Arc::new(ObservabilityDb::new(&db_url).await?);

    // Query trace
    let trace = db.query_trace(&args.request_id).await?;

    if trace.root_span.is_none() && trace.logs.is_empty() {
        println!(
            "{}",
            format!("No trace found for request ID: {}", args.request_id).yellow()
        );
        return Ok(());
    }

    // Display results
    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&trace)?;
            println!("{}", json);
        }
        "tree" | _ => {
            display_trace_tree(&trace, args.verbose, args.logs)?;
        }
    }

    Ok(())
}

/// Display trace in ASCII tree format
fn display_trace_tree(trace: &TraceTree, verbose: bool, show_logs: bool) -> Result<()> {
    // Header
    println!("{}", "=".repeat(80).dimmed());
    println!("{} {}", "Request ID:".bold(), trace.request_id.cyan());

    if let Some(duration) = trace.total_duration_ms {
        println!(
            "{} {}",
            "Total Duration:".bold(),
            format_duration(duration)
        );
    }

    println!("{} {}", "Spans:".bold(), trace.root_span.is_some());
    println!("{} {}", "Logs:".bold(), trace.logs.len());
    println!("{}", "=".repeat(80).dimmed());
    println!();

    // Display span tree
    if let Some(ref root_span) = trace.root_span {
        println!("{}", "Span Hierarchy:".bold().underline());
        println!();
        display_span_node(root_span, 0, "", verbose);
        println!();
    }

    // Display logs
    if show_logs && !trace.logs.is_empty() {
        println!();
        println!("{}", "Associated Logs:".bold().underline());
        println!();

        for log in &trace.logs {
            let timestamp = chrono::DateTime::from_timestamp_millis(log.timestamp as i64)
                .unwrap_or_default()
                .format("%H:%M:%S%.3f");

            let level_colored = match log.level.as_str() {
                "ERROR" => log.level.red().bold(),
                "WARN" => log.level.yellow().bold(),
                "INFO" => log.level.green(),
                "DEBUG" => log.level.blue(),
                _ => log.level.normal(),
            };

            println!(
                "  {} {} {} {}",
                timestamp.to_string().dimmed(),
                level_colored,
                log.target.cyan(),
                log.message
            );
        }
        println!();
    }

    Ok(())
}

/// Recursively display span node with ASCII tree
fn display_span_node(
    span: &llm_gateway::observability::query::SpanNode,
    depth: usize,
    prefix: &str,
    verbose: bool,
) {
    // Determine tree characters
    let is_last = true; // We'll need to track this properly in a more advanced version
    let tree_char = if depth == 0 {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };

    // Format duration
    let duration_str = if let Some(duration) = span.duration_ms {
        format_duration(duration)
    } else {
        "running".yellow().to_string()
    };

    // Status color
    let status_colored = match span.status.as_str() {
        "ok" => span.status.green(),
        "error" => span.status.red().bold(),
        _ => span.status.yellow(),
    };

    // Print span
    println!(
        "{}{}{} {} {} {}",
        prefix,
        tree_char,
        span.name.bold().blue(),
        format!("[{}]", span.kind).dimmed(),
        duration_str,
        format!("({})", status_colored)
    );

    // Show attributes if verbose
    if verbose {
        if let Some(obj) = span.attributes.as_object() {
            if !obj.is_empty() {
                for (key, value) in obj {
                    let attr_prefix = format!("{}    ", prefix);
                    println!(
                        "{}{}: {}",
                        attr_prefix.dimmed(),
                        key,
                        value.to_string().dimmed()
                    );
                }
            }
        }
    }

    // Recursively display children
    let child_prefix = if depth == 0 {
        String::new()
    } else {
        format!("{}  ", prefix)
    };

    for (i, child) in span.children.iter().enumerate() {
        let is_last_child = i == span.children.len() - 1;
        let child_tree_prefix = if is_last_child {
            format!("{}  ", child_prefix)
        } else {
            format!("{}│ ", child_prefix)
        };
        display_span_node(child, depth + 1, &child_tree_prefix, verbose);
    }
}

/// Format duration in human-friendly way
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
    fn test_trace_args_parsing() {
        let args = TraceArgs::parse_from(&[
            "trace",
            "550e8400-e29b-41d4-a716-446655440000",
            "--logs",
        ]);
        assert_eq!(args.request_id, "550e8400-e29b-41d4-a716-446655440000");
        assert!(args.logs);
    }

    #[test]
    fn test_format_duration() {
        assert!(format_duration(500).contains("500ms"));
        assert!(format_duration(1500).contains("1.50s"));
    }
}
