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

    /// Configuration management commands
    Config {
        #[command(subcommand)]
        action: ConfigCommands,
    },

    /// Display real-time stats dashboard
    Stats {
        /// Refresh interval in seconds
        #[arg(short, long, default_value = "1.0")]
        interval: f64,

        /// Metrics endpoint URL (auto-detected from config if not provided)
        #[arg(short, long)]
        url: Option<String>,

        /// Group by: api-key, provider, model, all
        #[arg(short, long, default_value = "provider")]
        group_by: String,
    },

    /// Query observability logs
    Logs(crate::commands::logs::LogsArgs),

    /// Display request trace
    Trace(crate::commands::trace::TraceArgs),

    /// Manage observability database
    Observability(crate::commands::observability::ObservabilityArgs),

    /// Show version information
    Version,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommands {
    /// Display current configuration (with secrets masked)
    Show,

    /// Validate configuration file
    Validate,
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
    fn test_cli_parsing_config_show() {
        let args = vec!["gateway", "config", "show"];
        let cli = Cli::try_parse_from(args).unwrap();

        match cli.get_command() {
            Commands::Config { action } => {
                matches!(action, ConfigCommands::Show);
            }
            _ => panic!("Expected Config command"),
        }
    }
}
