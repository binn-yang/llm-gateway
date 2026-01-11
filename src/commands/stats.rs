//! Stats command implementation
//!
//! This module implements the `stats` subcommand which displays a real-time
//! dashboard of metrics from the gateway's Prometheus endpoint.

use anyhow::Result;
use chrono::Utc;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::FutureExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::time::interval;

use llm_gateway::{
    config,
    stats::{
        fetcher::MetricsFetcher,
        parser::{parse_and_aggregate, GroupBy},
        StatsApp,
    },
};

/// Execute the stats command
///
/// # Arguments
/// * `interval_secs` - Refresh interval in seconds
/// * `url` - Optional metrics endpoint URL (auto-detected if None)
/// * `group_by` - Grouping strategy ("api-key", "provider", "model", or "all")
pub async fn execute(
    interval_secs: f64,
    url: Option<String>,
    group_by: String,
) -> Result<()> {
    // 1. Validate arguments
    validate_args(interval_secs, &group_by)?;

    // 2. Build metrics URL
    let metrics_url = build_metrics_url(url)?;

    // 3. Parse group strategy
    let group_strategy = parse_group_by(&group_by)?;

    // 4. Run dashboard
    run_dashboard(metrics_url, interval_secs, group_strategy).await
}

/// Validate command arguments
fn validate_args(interval: f64, group_by: &str) -> Result<()> {
    // Validate interval
    if !(0.1..=60.0).contains(&interval) {
        anyhow::bail!(
            "Invalid interval: {}. Must be between 0.1 and 60 seconds",
            interval
        );
    }

    // Validate group_by
    match group_by {
        "api-key" | "provider" | "model" | "all" => Ok(()),
        _ => anyhow::bail!(
            "Invalid group-by: '{}'. Must be one of: api-key, provider, model, all",
            group_by
        ),
    }
}

/// Build metrics URL from config or override
fn build_metrics_url(url_override: Option<String>) -> Result<String> {
    if let Some(url) = url_override {
        return Ok(url);
    }

    // Auto-detect from config.toml
    let cfg = config::load_config()?;
    let host = cfg.server.host;
    let port = cfg.server.port;
    let endpoint = cfg.metrics.endpoint;

    Ok(format!("http://{}:{}{}", host, port, endpoint))
}

/// Detect observability database path from config or use default

/// Parse group-by strategy string
fn parse_group_by(group_by: &str) -> Result<GroupBy> {
    match group_by {
        "api-key" => Ok(GroupBy::ApiKey),
        "provider" => Ok(GroupBy::Provider),
        "model" => Ok(GroupBy::Model),
        "all" => Ok(GroupBy::All),
        _ => anyhow::bail!("Invalid group-by: {}", group_by),
    }
}

/// Run the stats dashboard
async fn run_dashboard(
    metrics_url: String,
    interval_secs: f64,
    group_by: GroupBy,
) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Clear screen on startup
    terminal.clear()?;

    // Initialize state
    let mut app = StatsApp::new(group_by);
    let metrics_fetcher = MetricsFetcher::new(metrics_url.clone());
    let mut interval_timer = interval(Duration::from_secs_f64(interval_secs));

    // Initial fetch
    fetch_and_update(&mut app, &metrics_fetcher).await;

    // Main loop
    let result = loop {
        // Render UI
        if let Err(e) = terminal.draw(|f| app.render(f)) {
            break Err(e.into());
        }

        // Handle events with timeout
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Handle key and check if we should quit
                if app.handle_key(key) {
                    break Ok(());
                }

                // Check for manual refresh (r key)
                if matches!(key.code, crossterm::event::KeyCode::Char('r') | crossterm::event::KeyCode::Char('R')) {
                    fetch_and_update(&mut app, &metrics_fetcher).await;
                }

                // Clear terminal after any key press to avoid residual content
                // This is necessary when switching groupings
                if matches!(
                    key.code,
                    crossterm::event::KeyCode::Char('1')
                        | crossterm::event::KeyCode::Char('2')
                        | crossterm::event::KeyCode::Char('3')
                        | crossterm::event::KeyCode::Char('4')
                ) {
                    terminal.clear()?;
                }
            }
        }

        // Check if interval elapsed
        if interval_timer.tick().now_or_never().is_some() {
            fetch_and_update(&mut app, &metrics_fetcher).await;
        }
    };

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Fetch metrics and update app state
async fn fetch_and_update(app: &mut StatsApp, metrics_fetcher: &MetricsFetcher) {
    // Fetch Prometheus metrics
    match metrics_fetcher.fetch().await {
        Ok(text) => {
            // Try to parse and aggregate
            match parse_and_aggregate(&text, app.group_by) {
                Ok(metrics) => {
                    app.metrics = metrics;
                    app.last_update = Some(Utc::now());
                    app.error_message = None;
                }
                Err(e) => {
                    app.error_message = Some(format!("Parse error: {}", e));
                }
            }
        }
        Err(e) => {
            app.error_message = Some(format!("Fetch error: {}", e));
        }
    }
}
