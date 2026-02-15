use anyhow::Result;
use chrono::Utc;
use comfy_table::{presets::UTF8_FULL, Cell, Color, ContentArrangement, Table};
use llm_gateway::config;
use sqlx::{FromRow, SqlitePool};
use tracing::info;

/// Token usage statistics row from database
#[derive(Debug, FromRow)]
struct TokenUsageRow {
    provider: String,
    instance: String,
    model: String,
    requests: i64,
    total_tokens: i64,
    input_tokens: i64,
    output_tokens: i64,
    cache_creation_tokens: i64,
    cache_read_tokens: i64,
    input_cost: f64,
    output_cost: f64,
    cache_write_cost: f64,
    cache_read_cost: f64,
    total_cost: f64,
}

/// API key cost statistics row from database
#[derive(Debug, FromRow)]
struct ApiKeyCostRow {
    api_key_name: String,
    requests: i64,
    total_cost: f64,
}

/// Execute the stats command
///
/// Displays system statistics and token usage information
pub async fn execute(hours: u32, detailed: bool) -> Result<()> {
    println!("LLM Gateway Statistics");
    println!("======================\n");

    info!("Loading configuration");
    let cfg = config::load_config()?;

    // Display system summary
    display_system_summary(&cfg, hours).await?;

    // Display provider health status
    display_provider_health(&cfg).await?;

    // Display token usage statistics
    display_token_usage(&cfg, hours, detailed).await?;

    // Display quota status
    display_quota_status(&cfg).await?;

    Ok(())
}

/// Display system summary section
async fn display_system_summary(cfg: &config::Config, hours: u32) -> Result<()> {
    println!("System Summary:");

    // Count API keys
    let total_api_keys = cfg.api_keys.len();
    let enabled_api_keys = cfg.api_keys.iter().filter(|k| k.enabled).count();

    // Count providers
    let total_providers = cfg.providers.openai.len()
        + cfg.providers.anthropic.len()
        + cfg.providers.gemini.len();

    let enabled_providers = cfg.providers.openai.iter().filter(|p| p.enabled).count()
        + cfg.providers.anthropic.iter().filter(|p| p.enabled).count()
        + cfg.providers.gemini.iter().filter(|p| p.enabled).count();

    println!(
        "  API Keys:          {} configured ({} enabled)",
        total_api_keys, enabled_api_keys
    );
    println!(
        "  Providers:         {} total ({} enabled)",
        total_providers, enabled_providers
    );

    // Try to connect to database for runtime statistics
    if cfg.observability.enabled {
        match connect_to_database(cfg).await {
            Ok(pool) => {
                display_database_stats(&pool, hours).await?;
            }
            Err(e) => {
                println!("  Database:          Not available ({})", e);
            }
        }
    } else {
        println!("  Observability:     Disabled");
    }

    println!();
    Ok(())
}

/// Provider health status row from database
#[derive(Debug, FromRow)]
#[allow(dead_code)]
struct ProviderHealthRow {
    provider: String,
    instance: String,
    event_type: String,
    consecutive_failures: i64,
    next_retry_secs: Option<i64>,
    timestamp: String,
}

/// Display provider health status
async fn display_provider_health(cfg: &config::Config) -> Result<()> {
    if !cfg.observability.enabled {
        println!("Provider Health Status: Not available (observability disabled)\n");
        return Ok(());
    }

    let pool = match connect_to_database(cfg).await {
        Ok(pool) => pool,
        Err(e) => {
            println!("Provider Health Status: Not available ({})\n", e);
            return Ok(());
        }
    };

    // Get all configured instances
    let mut all_instances = Vec::new();
    for inst in &cfg.providers.openai {
        all_instances.push(("openai".to_string(), inst.name.clone()));
    }
    for inst in &cfg.providers.anthropic {
        all_instances.push(("anthropic".to_string(), inst.name.clone()));
    }
    for inst in &cfg.providers.gemini {
        all_instances.push(("gemini".to_string(), inst.name.clone()));
    }

    if all_instances.is_empty() {
        println!("Provider Health Status: No providers configured\n");
        return Ok(());
    }

    // Query latest event for each instance
    let mut health_statuses = Vec::new();

    for (provider, instance) in &all_instances {
        let latest_event = sqlx::query_as::<_, ProviderHealthRow>(
            "SELECT provider, instance, event_type, consecutive_failures, next_retry_secs, timestamp
             FROM failover_events
             WHERE provider = ? AND instance = ?
             ORDER BY timestamp DESC
             LIMIT 1"
        )
        .bind(provider)
        .bind(instance)
        .fetch_optional(&pool)
        .await?;

        match latest_event {
            Some(event) => {
                health_statuses.push((provider.clone(), instance.clone(), Some(event)));
            }
            None => {
                // No events = healthy
                health_statuses.push((provider.clone(), instance.clone(), None));
            }
        }
    }

    // Display health status
    println!("Provider Health Status:");

    let mut healthy_count = 0;
    let mut recovering_count = 0;
    let mut unhealthy_count = 0;

    for (provider, instance, event_opt) in &health_statuses {
        let (status_symbol, status_text, details) = match event_opt {
            None => {
                healthy_count += 1;
                ("✅", "Healthy", "(0 failures)".to_string())
            }
            Some(event) => {
                match event.event_type.as_str() {
                    "circuit_closed" => {
                        healthy_count += 1;
                        ("✅", "Healthy", "(0 failures)".to_string())
                    }
                    "circuit_half_open" | "recovery" => {
                        recovering_count += 1;
                        let retry_info = if let Some(retry_secs) = event.next_retry_secs {
                            format!("(testing recovery, retry in {}s)", retry_secs)
                        } else {
                            "(testing recovery)".to_string()
                        };
                        ("🟡", "Recovering", retry_info)
                    }
                    "circuit_open" | "failure" => {
                        unhealthy_count += 1;
                        let failures = event.consecutive_failures;
                        let retry_info = if let Some(retry_secs) = event.next_retry_secs {
                            format!("({} failures, retry in {}s)", failures, retry_secs)
                        } else {
                            format!("({} failures)", failures)
                        };
                        ("🔴", "Unhealthy", retry_info)
                    }
                    _ => {
                        healthy_count += 1;
                        ("✅", "Healthy", "(0 failures)".to_string())
                    }
                }
            }
        };

        println!("  {:<30} {} {:<12} {}",
            format!("{}-{}", provider, instance),
            status_symbol,
            status_text,
            details
        );
    }

    let total = health_statuses.len();
    println!("\nOverall: {}/{} healthy, {} recovering, {} down\n",
        healthy_count, total, recovering_count, unhealthy_count);

    Ok(())
}

/// Connect to the observability database
async fn connect_to_database(cfg: &config::Config) -> Result<SqlitePool> {
    let db_path = &cfg.observability.database_path;
    let pool = SqlitePool::connect(&format!("sqlite:{}", db_path)).await?;
    Ok(pool)
}

/// Display database-derived statistics
async fn display_database_stats(pool: &SqlitePool, hours: u32) -> Result<()> {
    // Calculate cutoff timestamp
    let cutoff_timestamp = Utc::now().timestamp_millis() - (hours as i64 * 3600 * 1000);

    // Query healthy providers (based on recent successful requests)
    let healthy_providers = sqlx::query_as::<_, (String, String)>(
        "SELECT DISTINCT provider, instance
         FROM requests
         WHERE timestamp >= ?
           AND status = 'success'
         GROUP BY provider, instance
         HAVING COUNT(*) > 0"
    )
    .bind(cutoff_timestamp)
    .fetch_all(pool)
    .await?;

    println!("  Healthy Providers: {} (last {} hours)", healthy_providers.len(), hours);

    // Query system uptime (from earliest request)
    let uptime_result = sqlx::query_as::<_, (Option<i64>,)>(
        "SELECT MIN(timestamp) as earliest FROM requests"
    )
    .fetch_one(pool)
    .await?;

    if let Some(earliest_timestamp) = uptime_result.0 {
        if earliest_timestamp > 0 {
            let uptime_ms = Utc::now().timestamp_millis() - earliest_timestamp;
            let uptime_str = format_duration(uptime_ms);
            println!("  System Uptime:     {}", uptime_str);
        }
    }

    // Query last hour statistics
    let last_hour_cutoff = Utc::now().timestamp_millis() - 3600 * 1000;

    let last_hour_api_keys = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(DISTINCT api_key_name) FROM requests WHERE timestamp >= ?"
    )
    .bind(last_hour_cutoff)
    .fetch_one(pool)
    .await?;

    let last_hour_providers = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(DISTINCT provider) FROM requests WHERE timestamp >= ?"
    )
    .bind(last_hour_cutoff)
    .fetch_one(pool)
    .await?;

    println!(
        "  Last Hour:         {} unique API keys, {} active providers",
        last_hour_api_keys.0, last_hour_providers.0
    );

    Ok(())
}

/// Display token usage statistics table
async fn display_token_usage(cfg: &config::Config, hours: u32, detailed: bool) -> Result<()> {
    if !cfg.observability.enabled {
        println!("Token Usage: Not available (observability disabled)");
        return Ok(());
    }

    let pool = match connect_to_database(cfg).await {
        Ok(pool) => pool,
        Err(e) => {
            println!("Token Usage: Not available ({})", e);
            return Ok(());
        }
    };

    // Calculate cutoff timestamp
    let cutoff_timestamp = Utc::now().timestamp_millis() - (hours as i64 * 3600 * 1000);

    // Query token usage statistics
    let stats = sqlx::query_as::<_, TokenUsageRow>(
        "SELECT
            provider,
            instance,
            model,
            COUNT(*) as requests,
            COALESCE(SUM(total_tokens), 0) as total_tokens,
            COALESCE(SUM(input_tokens), 0) as input_tokens,
            COALESCE(SUM(output_tokens), 0) as output_tokens,
            COALESCE(SUM(cache_creation_input_tokens), 0) as cache_creation_tokens,
            COALESCE(SUM(cache_read_input_tokens), 0) as cache_read_tokens,
            COALESCE(SUM(input_cost), 0.0) as input_cost,
            COALESCE(SUM(output_cost), 0.0) as output_cost,
            COALESCE(SUM(cache_write_cost), 0.0) as cache_write_cost,
            COALESCE(SUM(cache_read_cost), 0.0) as cache_read_cost,
            COALESCE(SUM(total_cost), 0.0) as total_cost
         FROM requests
         WHERE timestamp >= ?
         GROUP BY provider, instance, model
         ORDER BY total_cost DESC"
    )
    .bind(cutoff_timestamp)
    .fetch_all(&pool)
    .await?;

    if stats.is_empty() {
        println!("Token Usage (Last {} Hours): No data available", hours);
        return Ok(());
    }

    // Calculate total tokens and cost for percentage
    let total_all_tokens: i64 = stats.iter().map(|s| s.total_tokens).sum();
    let total_all_cost: f64 = stats.iter().map(|s| s.total_cost).sum();

    // Create table
    println!("Token Usage (Last {} Hours):", hours);
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Add header
    table.set_header(vec![
        Cell::new("PROVIDER").fg(Color::Cyan),
        Cell::new("INSTANCE").fg(Color::Cyan),
        Cell::new("MODEL").fg(Color::Cyan),
        Cell::new("REQUESTS").fg(Color::Cyan),
        Cell::new("INPUT").fg(Color::Cyan),
        Cell::new("OUTPUT").fg(Color::Cyan),
        Cell::new("CACHE CREATE").fg(Color::Cyan),
        Cell::new("CACHE READ").fg(Color::Cyan),
        Cell::new("TOTAL TOKENS").fg(Color::Cyan),
        Cell::new("TOKEN PERCENTAGE").fg(Color::Cyan),
        Cell::new("IN COST").fg(Color::Cyan),
        Cell::new("OUT COST").fg(Color::Cyan),
        Cell::new("CACHE W").fg(Color::Cyan),
        Cell::new("CACHE R").fg(Color::Cyan),
        Cell::new("TOTAL COST").fg(Color::Cyan),
        Cell::new("COST PERCENTAGE").fg(Color::Cyan),
    ]);

    // Add rows
    for stat in &stats {
        let token_percentage = if total_all_tokens > 0 {
            (stat.total_tokens as f64 / total_all_tokens as f64) * 100.0
        } else {
            0.0
        };

        let cost_percentage = if total_all_cost > 0.0 {
            (stat.total_cost / total_all_cost) * 100.0
        } else {
            0.0
        };

        table.add_row(vec![
            Cell::new(&stat.provider),
            Cell::new(&stat.instance),
            Cell::new(truncate_model_name(&stat.model)),
            Cell::new(format_number(stat.requests)),
            Cell::new(format_number(stat.input_tokens)),
            Cell::new(format_number(stat.output_tokens)),
            Cell::new(format_number(stat.cache_creation_tokens)),
            Cell::new(format_number(stat.cache_read_tokens)),
            Cell::new(format_number(stat.total_tokens)),
            Cell::new(format!("{:.1}%", token_percentage)),
            Cell::new(format!("${:.6}", stat.input_cost)),
            Cell::new(format!("${:.6}", stat.output_cost)),
            Cell::new(format!("${:.6}", stat.cache_write_cost)),
            Cell::new(format!("${:.6}", stat.cache_read_cost)),
            Cell::new(format!("${:.6}", stat.total_cost)),
            Cell::new(format!("{:.1}%", cost_percentage)),
        ]);
    }

    println!("{}", table);

    // Print summary
    let total_requests: i64 = stats.iter().map(|s| s.requests).sum();
    let total_cost: f64 = stats.iter().map(|s| s.total_cost).sum();
    println!(
        "\nTotal: {} requests, {} tokens, ${:.6}",
        format_number(total_requests),
        format_number(total_all_tokens),
        total_cost
    );

    // Display detailed API key statistics if requested
    if detailed {
        display_api_key_costs(&pool, hours).await?;
    }

    Ok(())
}

/// Display cost breakdown by API key (detailed mode)
async fn display_api_key_costs(pool: &SqlitePool, hours: u32) -> Result<()> {
    println!("\nCost by API Key (Last {} Hours):", hours);

    // Calculate cutoff timestamp
    let cutoff_timestamp = Utc::now().timestamp_millis() - (hours as i64 * 3600 * 1000);

    // Query API key statistics
    let api_key_stats = sqlx::query_as::<_, ApiKeyCostRow>(
        "SELECT
            api_key_name,
            COUNT(*) as requests,
            COALESCE(SUM(total_cost), 0.0) as total_cost
         FROM requests
         WHERE timestamp >= ?
         GROUP BY api_key_name
         ORDER BY total_cost DESC"
    )
    .bind(cutoff_timestamp)
    .fetch_all(pool)
    .await?;

    if api_key_stats.is_empty() {
        println!("  No data available");
        return Ok(());
    }

    // Create table
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("API KEY").fg(Color::Cyan),
        Cell::new("REQUESTS").fg(Color::Cyan),
        Cell::new("TOTAL COST").fg(Color::Cyan),
        Cell::new("TOP MODEL").fg(Color::Cyan),
        Cell::new("TOP MODEL COST").fg(Color::Cyan),
    ]);

    // For each API key, find the top model by cost
    for api_key_stat in &api_key_stats {
        let top_model_result = sqlx::query_as::<_, (String, f64)>(
            "SELECT
                model as top_model,
                COALESCE(SUM(total_cost), 0.0) as top_model_cost
             FROM requests
             WHERE timestamp >= ? AND api_key_name = ?
             GROUP BY model
             ORDER BY top_model_cost DESC
             LIMIT 1"
        )
        .bind(cutoff_timestamp)
        .bind(&api_key_stat.api_key_name)
        .fetch_optional(pool)
        .await?;

        let (top_model, top_model_cost) = top_model_result.unwrap_or(("N/A".to_string(), 0.0));

        table.add_row(vec![
            Cell::new(&api_key_stat.api_key_name),
            Cell::new(format_number(api_key_stat.requests)),
            Cell::new(format!("${:.6}", api_key_stat.total_cost)),
            Cell::new(truncate_model_name(&top_model)),
            Cell::new(format!("${:.6}", top_model_cost)),
        ]);
    }

    println!("{}", table);

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Format duration in milliseconds to human-readable string
fn format_duration(ms: i64) -> String {
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;

    if days > 0 {
        let remaining_hours = hours % 24;
        let remaining_minutes = minutes % 60;
        format!("{}d {}h {}m", days, remaining_hours, remaining_minutes)
    } else if hours > 0 {
        let remaining_minutes = minutes % 60;
        format!("{}h {}m", hours, remaining_minutes)
    } else if minutes > 0 {
        let remaining_seconds = seconds % 60;
        format!("{}m {}s", minutes, remaining_seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format number with commas or K/M suffix
fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Truncate model name if too long
fn truncate_model_name(name: &str) -> String {
    const MAX_LEN: usize = 20;
    if name.len() > MAX_LEN {
        format!("{}...", &name[..MAX_LEN - 3])
    } else {
        name.to_string()
    }
}

/// Display quota status for all provider instances
async fn display_quota_status(cfg: &config::Config) -> Result<()> {
    if !cfg.observability.enabled {
        println!("\nQuota Status: Not available (observability disabled)");
        return Ok(());
    }

    let pool = match connect_to_database(cfg).await {
        Ok(pool) => pool,
        Err(e) => {
            println!("\nQuota Status: Not available ({})", e);
            return Ok(());
        }
    };

    // Use the quota database module
    let quota_db = llm_gateway::quota::db::QuotaDatabase::new(pool);

    let snapshots = match quota_db.get_latest_snapshots().await {
        Ok(s) => s,
        Err(e) => {
            println!("\nQuota Status: Not available ({})", e);
            return Ok(());
        }
    };

    if snapshots.is_empty() {
        println!("\nQuota Status: No data available (waiting for first refresh)");
        return Ok(());
    }

    println!("\nQuota Status:");

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("PROVIDER").fg(Color::Cyan),
        Cell::new("INSTANCE").fg(Color::Cyan),
        Cell::new("AUTH MODE").fg(Color::Cyan),
        Cell::new("STATUS").fg(Color::Cyan),
        Cell::new("QUOTA INFO").fg(Color::Cyan),
        Cell::new("LAST UPDATE").fg(Color::Cyan),
    ]);

    for snapshot in &snapshots {
        let status_cell = match snapshot.status.as_str() {
            "success" => Cell::new("✓ OK").fg(Color::Green),
            "error" => Cell::new("✗ ERROR").fg(Color::Red),
            "unavailable" => Cell::new("- N/A").fg(Color::DarkGrey),
            _ => Cell::new(&snapshot.status),
        };

        let quota_info = format_quota_info(&snapshot.quota_data, &snapshot.status)?;
        let last_update = format_time_ago(snapshot.timestamp);

        table.add_row(vec![
            Cell::new(&snapshot.provider),
            Cell::new(&snapshot.instance),
            Cell::new(&snapshot.auth_mode),
            status_cell,
            Cell::new(quota_info),
            Cell::new(last_update),
        ]);
    }

    println!("{}", table);
    Ok(())
}

/// Format quota info based on provider type
fn format_quota_info(quota_data: &str, status: &str) -> Result<String> {
    if status != "success" {
        return Ok("-".to_string());
    }

    let data: serde_json::Value = serde_json::from_str(quota_data)?;

    match data["type"].as_str() {
        Some("anthropic_oauth") => {
            let five_h = data["windows"]["five_hour"]["utilization"]
                .as_f64()
                .unwrap_or(0.0);
            let seven_d = data["windows"]["seven_day"]["utilization"]
                .as_f64()
                .unwrap_or(0.0);
            let seven_d_sonnet = data["windows"]["seven_day_sonnet"]["utilization"]
                .as_f64()
                .unwrap_or(0.0);

            Ok(format!(
                "5h: {:.1}% | 7d: {:.1}% | 7d(s): {:.1}%",
                five_h * 100.0,
                seven_d * 100.0,
                seven_d_sonnet * 100.0
            ))
        }
        Some("gemini_antigravity") => {
            let percentage = data["overall"]["percentage"]
                .as_f64()
                .unwrap_or(0.0);
            Ok(format!("Used: {:.1}%", percentage))
        }
        _ => Ok("Unknown format".to_string()),
    }
}

/// Format timestamp as time ago
fn format_time_ago(timestamp_ms: i64) -> String {
    let now = Utc::now().timestamp_millis();
    let diff_ms = now - timestamp_ms;
    let diff_secs = diff_ms / 1000;

    if diff_secs < 60 {
        format!("{}s ago", diff_secs)
    } else if diff_secs < 3600 {
        format!("{}m ago", diff_secs / 60)
    } else if diff_secs < 86400 {
        format!("{}h ago", diff_secs / 3600)
    } else {
        format!("{}d ago", diff_secs / 86400)
    }
}
