use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "qai",
    about = "Natural language to shell commands via LLM",
    version = env!("GIT_DESCRIBE"),
    after_help = "Logs are written to: ~/.local/share/qai/logs/qai.log\n\n\
                  Setup: eval \"$(qai shell-init zsh)\" in your .zshrc\n\
                  Usage: Type 'qai <your request>' then press Tab"
)]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, help = "Path to config file")]
    pub config: Option<PathBuf>,

    /// Enable verbose output
    #[arg(short, long, help = "Enable verbose output")]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Send a query to the LLM and get a shell command
    #[command(name = "query")]
    Query {
        /// The natural language query
        #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
        query: Vec<String>,
    },

    /// Print shell initialization script
    #[command(name = "shell-init")]
    ShellInit {
        /// Shell to generate init script for (zsh)
        #[arg(default_value = "zsh")]
        shell: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn test_cli_no_args() {
        let cli = Cli::try_parse_from(["qai"]);
        assert!(cli.is_ok());
        let cli = cli.unwrap();
        assert!(cli.command.is_none());
        assert!(cli.config.is_none());
        assert!(!cli.verbose);
    }

    #[test]
    fn test_cli_query_single_word() {
        let cli = Cli::try_parse_from(["qai", "query", "test"]).unwrap();
        match cli.command {
            Some(Commands::Query { query }) => {
                assert_eq!(query, vec!["test"]);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_query_multiple_words() {
        let cli = Cli::try_parse_from(["qai", "query", "list", "all", "files"]).unwrap();
        match cli.command {
            Some(Commands::Query { query }) => {
                assert_eq!(query, vec!["list", "all", "files"]);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_query_requires_args() {
        let result = Cli::try_parse_from(["qai", "query"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_shell_init_default() {
        let cli = Cli::try_parse_from(["qai", "shell-init"]).unwrap();
        match cli.command {
            Some(Commands::ShellInit { shell }) => {
                assert_eq!(shell, "zsh");
            }
            _ => panic!("Expected ShellInit command"),
        }
    }

    #[test]
    fn test_cli_shell_init_explicit() {
        let cli = Cli::try_parse_from(["qai", "shell-init", "bash"]).unwrap();
        match cli.command {
            Some(Commands::ShellInit { shell }) => {
                assert_eq!(shell, "bash");
            }
            _ => panic!("Expected ShellInit command"),
        }
    }

    #[test]
    fn test_cli_config_option() {
        let cli = Cli::try_parse_from(["qai", "-c", "/path/to/config.yml", "query", "test"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.yml")));
    }

    #[test]
    fn test_cli_config_long_option() {
        let cli = Cli::try_parse_from(["qai", "--config", "/path/to/config.yml", "query", "test"]).unwrap();
        assert_eq!(cli.config, Some(PathBuf::from("/path/to/config.yml")));
    }

    #[test]
    fn test_cli_verbose_flag() {
        let cli = Cli::try_parse_from(["qai", "-v", "query", "test"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_cli_verbose_long_flag() {
        let cli = Cli::try_parse_from(["qai", "--verbose", "query", "test"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn test_cli_help_available() {
        // Just verify help doesn't panic
        let mut cmd = Cli::command();
        let _ = cmd.render_help();
    }

    #[test]
    fn test_cli_version_available() {
        let cmd = Cli::command();
        assert!(cmd.get_version().is_some());
    }

    #[test]
    fn test_cli_invalid_command() {
        let result = Cli::try_parse_from(["qai", "invalid-command"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_query_with_special_characters() {
        let cli = Cli::try_parse_from(["qai", "query", "find", "*.txt", "-name"]).unwrap();
        match cli.command {
            Some(Commands::Query { query }) => {
                assert_eq!(query, vec!["find", "*.txt", "-name"]);
            }
            _ => panic!("Expected Query command"),
        }
    }
}
