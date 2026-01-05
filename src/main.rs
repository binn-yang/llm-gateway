use anyhow::Result;
use clap::Parser;

mod cli;
mod commands;
mod daemon;
mod pid;

use llm_gateway::init_tracing;

#[tokio::main]
async fn main() -> Result<()> {
    // On macOS, disable fork safety check BEFORE any library initialization
    // This must be done before parsing CLI arguments or any other library calls
    #[cfg(target_os = "macos")]
    unsafe {
        std::env::set_var("OBJC_DISABLE_INITIALIZE_FORK_SAFETY", "YES");
    }

    // Parse CLI arguments
    let args = cli::Cli::parse();

    // Initialize tracing/logging early (except for daemon mode)
    // In daemon mode, tracing will be initialized after fork() to avoid macOS fork issues
    let is_daemon_mode = matches!(args.get_command(), cli::Commands::Start { daemon: true, .. });

    if !is_daemon_mode {
        init_tracing();
    }

    // Dispatch to appropriate command handler
    match args.get_command() {
        cli::Commands::Start { daemon, pid_file } => {
            commands::start::execute(daemon, pid_file).await?;
        }
        cli::Commands::Stop {
            pid_file,
            force,
            timeout,
        } => {
            commands::stop::execute(pid_file, force, timeout).await?;
        }
        cli::Commands::Reload { pid_file } => {
            commands::reload::execute(pid_file).await?;
        }
        cli::Commands::Test => {
            commands::test::execute()?;
        }
        cli::Commands::Config { action } => match action {
            cli::ConfigCommands::Show => commands::config::show()?,
            cli::ConfigCommands::Validate => commands::config::validate()?,
        },
        cli::Commands::Version => {
            println!("LLM Gateway v{}", env!("CARGO_PKG_VERSION"));
            println!("Rust {}", env!("CARGO_PKG_RUST_VERSION"));
        }
    }

    Ok(())
}

