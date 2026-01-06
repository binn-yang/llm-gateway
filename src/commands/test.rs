use anyhow::Result;
use colored::Colorize;
use llm_gateway::config;
use tracing::info;

/// Execute the test command
///
/// This validates the configuration file without starting the server
pub fn execute() -> Result<()> {
    println!("{}", "Testing configuration...".yellow());
    info!("Loading and validating configuration");

    // Load configuration (this will validate it)
    let cfg = config::load_config()?;

    // Print success message
    println!("{}", "✓ Configuration test successful".green());
    println!();

    // Print summary
    println!("{}", "Configuration Summary:".bold());
    println!("  {}: {}:{}", "Server".cyan(), cfg.server.host, cfg.server.port);
    println!("  {}: {}", "Log Level".cyan(), cfg.server.log_level);
    println!("  {}: {}", "Log Format".cyan(), cfg.server.log_format);
    println!();

    println!("  {}: {}", "API Keys".cyan(), cfg.api_keys.len());
    for (idx, key_cfg) in cfg.api_keys.iter().enumerate() {
        let status = if key_cfg.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        };
        println!(
            "    {}. {} ({})",
            idx + 1,
            key_cfg.name,
            status
        );
    }
    println!();

    println!("  {}: {}", "Routing Rules".cyan(), cfg.routing.rules.len());
    for (prefix, provider) in cfg.routing.rules.iter() {
        println!(
            "    {} → {}",
            prefix, provider
        );
    }
    if let Some(default) = &cfg.routing.default_provider {
        println!("    {} → {}", "(default)".dimmed(), default);
    }
    println!();

    println!("{}", "Providers:".cyan());
    println!(
        "    OpenAI: {}",
        if cfg.providers.openai.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        }
    );
    println!(
        "    Anthropic: {}",
        if cfg.providers.anthropic.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        }
    );
    println!(
        "    Gemini: {}",
        if cfg.providers.gemini.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        }
    );
    println!();

    println!("  {}: {}", "Metrics".cyan(), if cfg.metrics.enabled {
        "enabled".green()
    } else {
        "disabled".red()
    });
    if cfg.metrics.enabled {
        println!("    Endpoint: {}", cfg.metrics.endpoint);
    }

    info!("Configuration validation completed successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    // Note: Testing this command requires a valid config file
    // and is better suited for integration tests
}
