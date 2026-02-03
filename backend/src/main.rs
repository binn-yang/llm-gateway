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

    // Initialize tracing/logging early (except for daemon mode and start command)
    // - In daemon mode, tracing will be initialized after fork()
    // - In start command, tracing will be initialized in server.rs with optional observability layer
    let needs_early_tracing = !matches!(
        args.get_command(),
        cli::Commands::Start { .. }
    );

    if needs_early_tracing {
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
        cli::Commands::Stats { hours, detailed } => {
            commands::stats::execute(hours, detailed).await?;
        }
        cli::Commands::OAuth { action } => {
            match action {
                cli::OAuthCommands::Login { provider, port, no_browser } => {
                    commands::oauth::login(provider.clone(), port, no_browser).await?;
                }
                cli::OAuthCommands::Status { provider, verbose } => {
                    commands::oauth::status(provider.clone(), verbose).await?;
                }
                cli::OAuthCommands::Refresh { provider } => {
                    commands::oauth::refresh(provider.clone()).await?;
                }
                cli::OAuthCommands::Logout { provider } => {
                    commands::oauth::logout(provider.clone()).await?;
                }
            }
        }
        cli::Commands::Version => {
            println!("LLM Gateway v{}", env!("CARGO_PKG_VERSION"));
            println!("Rust {}", env!("CARGO_PKG_RUST_VERSION"));
        }
    }

    Ok(())
}

