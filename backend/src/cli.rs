use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "gateway", version, about = "LLM Gateway")]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "config.toml", global = true)]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the gateway server (default)
    Start {
        /// Run in daemon mode (background process)
        #[arg(short, long)]
        daemon: bool,

        /// Path to PID file
        #[arg(short, long)]
        pid_file: Option<PathBuf>,
    },

    /// Stop a running gateway instance
    Stop {
        /// Path to PID file
        #[arg(short, long)]
        pid_file: Option<PathBuf>,

        /// Force kill if graceful shutdown times out
        #[arg(short, long)]
        force: bool,

        /// Timeout in seconds for graceful shutdown
        #[arg(short, long, default_value = "30")]
        timeout: u64,
    },

    /// Reload configuration without restarting (sends SIGHUP)
    Reload {
        /// Path to PID file
        #[arg(short, long)]
        pid_file: Option<PathBuf>,
    },

    /// Test configuration file validity
    Test,

    /// Display system statistics and token usage
    Stats {
        /// Number of hours to analyze (default: 24)
        #[arg(short = 'n', long, default_value = "24")]
        hours: u32,

        /// Show detailed breakdown by provider, API key, and model
        #[arg(short, long)]
        detailed: bool,
    },

    /// Show version information
    Version,

    /// OAuth authentication management
    #[command(name = "oauth")]
    OAuth {
        #[command(subcommand)]
        action: OAuthCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum OAuthCommands {
    /// Login to an OAuth provider
    Login {
        /// OAuth provider name
        provider: String,

        /// Local callback server port
        #[arg(short, long, default_value = "54545")]
        port: u16,
    },

    /// Show OAuth token status
    Status {
        /// OAuth provider name (optional, shows all if omitted)
        provider: Option<String>,

        /// Show verbose information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Manually refresh OAuth token
    Refresh {
        /// OAuth provider name
        provider: String,
    },

    /// Logout from OAuth provider (delete token)
    Logout {
        /// OAuth provider name
        provider: String,
    },
}

impl Cli {
    /// Get the command to execute, defaulting to Start if none provided
    pub fn get_command(&self) -> Commands {
        self.command.clone().unwrap_or(Commands::Start {
            daemon: false,
            pid_file: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_command_is_start() {
        let cli = Cli {
            config: PathBuf::from("config.toml"),
            command: None,
        };

        match cli.get_command() {
            Commands::Start { daemon, pid_file } => {
                assert!(!daemon);
                assert!(pid_file.is_none());
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_cli_parsing_start_with_daemon() {
        let args = vec!["gateway", "start", "--daemon"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.get_command() {
            Commands::Start { daemon, .. } => {
                assert!(daemon);
            }
            _ => panic!("Expected Start command"),
        }
    }

    #[test]
    fn test_cli_parsing_stop() {
        let args = vec!["gateway", "stop", "--timeout", "60"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.get_command() {
            Commands::Stop { timeout, .. } => {
                assert_eq!(timeout, 60);
            }
            _ => panic!("Expected Stop command"),
        }
    }

    #[test]
    fn test_cli_parsing_oauth_login() {
        let args = vec!["gateway", "oauth", "login", "test-provider"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.get_command() {
            Commands::OAuth { action } => {
                matches!(action, OAuthCommands::Login { .. });
            }
            _ => panic!("Expected OAuth command"),
        }
    }
}
