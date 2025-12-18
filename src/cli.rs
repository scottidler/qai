use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use std::path::PathBuf;

use crate::{build_status_footer, get_log_file};

/// Build the after_help dynamically with status info
fn get_after_help() -> String {
    let mut lines = vec![
        format!("Logs are written to: {}", get_log_file().display()),
        String::new(),
    ];

    lines.push(build_status_footer());

    lines.join("\n")
}

#[derive(Parser)]
#[command(
    name = "qai",
    about = "Natural language to shell commands via LLM",
    version = env!("GIT_DESCRIBE"),
    after_help = "Run 'qai --help' for status information"
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

impl Cli {
    /// Get CLI with dynamic after_help populated
    #[cfg(not(tarpaulin_include))]
    pub fn parse_with_status() -> Self {
        let after_help = get_after_help();
        let matches = Self::command().after_help(after_help).get_matches();

        Self::from_arg_matches(&matches).expect("Failed to parse CLI arguments")
    }
}

#[derive(Subcommand, Clone)]
pub enum Commands {
    /// Send a query to the LLM and get shell command(s)
    #[command(name = "query")]
    Query {
        /// Return multiple command options (one per line)
        #[arg(short, long, help = "Return multiple command options")]
        multi: bool,

        /// Number of results to return when using --multi
        #[arg(short = 'n', long, default_value = "5", help = "Number of results (with --multi)")]
        count: usize,

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

    /// Validate API key by calling OpenAI (no token usage)
    #[command(name = "validate-api")]
    ValidateApi,
}

/// Check if fzf is available and get its version
pub fn check_fzf_status() -> (bool, Option<String>) {
    use std::process::Command;

    match Command::new("fzf").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string();
            (true, Some(version))
        }
        _ => (false, None),
    }
}

/// Check if API key is configured (doesn't validate it)
pub fn check_api_key_configured() -> bool {
    // Check environment variable
    if let Ok(key) = std::env::var("QAI_API_KEY")
        && !key.is_empty()
    {
        return true;
    }

    // Check config file
    if let Some(config_dir) = dirs::config_dir() {
        let config_path = config_dir.join("qai").join("qai.yml");
        if config_path.exists()
            && let Ok(content) = std::fs::read_to_string(&config_path)
        {
            // Simple check for api_key in config
            if content.contains("api_key:") && !content.contains("api_key: null") {
                return true;
            }
        }
    }

    false
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
            Some(Commands::Query { query, multi, count }) => {
                assert_eq!(query, vec!["test"]);
                assert!(!multi);
                assert_eq!(count, 5);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_query_multiple_words() {
        let cli = Cli::try_parse_from(["qai", "query", "list", "all", "files"]).unwrap();
        match cli.command {
            Some(Commands::Query { query, .. }) => {
                assert_eq!(query, vec!["list", "all", "files"]);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_query_with_multi_flag() {
        let cli = Cli::try_parse_from(["qai", "query", "--multi", "find", "files"]).unwrap();
        match cli.command {
            Some(Commands::Query { query, multi, count }) => {
                assert_eq!(query, vec!["find", "files"]);
                assert!(multi);
                assert_eq!(count, 5);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_cli_query_with_multi_and_count() {
        let cli = Cli::try_parse_from(["qai", "query", "--multi", "-n", "10", "find", "files"]).unwrap();
        match cli.command {
            Some(Commands::Query { query, multi, count }) => {
                assert_eq!(query, vec!["find", "files"]);
                assert!(multi);
                assert_eq!(count, 10);
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
    fn test_cli_validate_api() {
        let cli = Cli::try_parse_from(["qai", "validate-api"]).unwrap();
        match cli.command {
            Some(Commands::ValidateApi) => {}
            _ => panic!("Expected ValidateApi command"),
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
        let mut cmd = <Cli as CommandFactory>::command();
        let _ = cmd.render_help();
    }

    #[test]
    fn test_cli_version_available() {
        let cmd = <Cli as CommandFactory>::command();
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
            Some(Commands::Query { query, .. }) => {
                assert_eq!(query, vec!["find", "*.txt", "-name"]);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_check_fzf_status_returns_tuple() {
        // Just verify the function returns without crashing
        let (available, version) = check_fzf_status();
        // If fzf is available, version should be Some
        if available {
            assert!(version.is_some());
        } else {
            assert!(version.is_none());
        }
    }

    #[test]
    fn test_check_api_key_configured_no_env() {
        // This test just verifies the function doesn't crash
        // Actual result depends on environment
        let _ = check_api_key_configured();
    }

    #[test]
    fn test_get_after_help_contains_log_path() {
        let help = get_after_help();
        assert!(help.contains("Logs are written to:"));
    }

    #[test]
    fn test_query_short_multi_flag() {
        let cli = Cli::try_parse_from(["qai", "query", "-m", "test"]).unwrap();
        match cli.command {
            Some(Commands::Query { multi, .. }) => {
                assert!(multi);
            }
            _ => panic!("Expected Query command"),
        }
    }

    #[test]
    fn test_query_count_without_multi() {
        // Count can be specified even without multi (it just won't be used)
        let cli = Cli::try_parse_from(["qai", "query", "-n", "3", "test"]).unwrap();
        match cli.command {
            Some(Commands::Query { count, multi, .. }) => {
                assert_eq!(count, 3);
                assert!(!multi);
            }
            _ => panic!("Expected Query command"),
        }
    }
}
