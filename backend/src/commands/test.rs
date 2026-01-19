use anyhow::Result;

use llm_gateway::config;
use tracing::info;

/// Execute the test command
///
/// This validates the configuration file without starting the server
pub fn execute() -> Result<()> {
    println!("{}", "Testing configuration...");
    info!("Loading and validating configuration");

    // Load configuration (this will validate it)
    let cfg = config::load_config()?;

    // Print success message
    println!("{}", "✓ Configuration test successful");
    println!();

    // Print summary
    println!("{}", "Configuration Summary:");
    println!("  {}: {}:{}", "Server", cfg.server.host, cfg.server.port);
    println!("  {}: {}", "Log Level", cfg.server.log_level);
    println!("  {}: {}", "Log Format", cfg.server.log_format);
    println!();

    println!("  {}: {}", "API Keys", cfg.api_keys.len());
    for (idx, key_cfg) in cfg.api_keys.iter().enumerate() {
        let status = if key_cfg.enabled {
            "enabled"
        } else {
            "disabled"
        };
        println!(
            "    {}. {} ({})",
            idx + 1,
            key_cfg.name,
            status
        );
    }
    println!();

    println!("  {}: {}", "Routing Rules", cfg.routing.rules.len());
    for (prefix, provider) in cfg.routing.rules.iter() {
        println!(
            "    {} → {}",
            prefix, provider
        );
    }
    if let Some(default) = &cfg.routing.default_provider {
        println!("    {} → {}", "(default)", default);
    }
    println!();

    println!("{}", "Providers:");

    // OpenAI instances
    let enabled_openai = cfg.providers.openai.iter().filter(|p| p.enabled).count();
    if enabled_openai > 0 {
        println!("    OpenAI: {} ({} instances)", "enabled", enabled_openai);
        for inst in cfg.providers.openai.iter().filter(|p| p.enabled) {
            println!("      - {} (priority: {})", inst.name, inst.priority);
        }
    } else {
        println!("    OpenAI: {}", "disabled");
    }

    // Anthropic instances
    let enabled_anthropic = cfg.providers.anthropic.iter().filter(|p| p.enabled).count();
    if enabled_anthropic > 0 {
        println!("    Anthropic: {} ({} instances)", "enabled", enabled_anthropic);
        for inst in cfg.providers.anthropic.iter().filter(|p| p.enabled) {
            println!("      - {} (priority: {})", inst.name, inst.priority);
        }
    } else {
        println!("    Anthropic: {}", "disabled");
    }

    // Gemini instances
    let enabled_gemini = cfg.providers.gemini.iter().filter(|p| p.enabled).count();
    if enabled_gemini > 0 {
        println!("    Gemini: {} ({} instances)", "enabled", enabled_gemini);
        for inst in cfg.providers.gemini.iter().filter(|p| p.enabled) {
            println!("      - {} (priority: {})", inst.name, inst.priority);
        }
    } else {
        println!("    Gemini: {}", "disabled");
    }
    println!();

    println!("  {}: {}", "Metrics", if cfg.metrics.enabled {
        "enabled"
    } else {
        "disabled"
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
