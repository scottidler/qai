use clap::Parser;
use eyre::{Context, Result};
use log::info;
use std::fs;
use std::path::PathBuf;

mod api;
mod cli;
mod config;
mod prompt;
mod shell;

use api::OpenAIClient;
use cli::{Cli, Commands};
use config::Config;
use prompt::{PromptContext, load_system_prompt, render_prompt};
use shell::generate_init_script;

fn setup_logging() -> Result<()> {
    let log_dir = get_log_dir();
    fs::create_dir_all(&log_dir).context("Failed to create log directory")?;

    let log_file = get_log_file();
    let target = Box::new(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
            .context("Failed to open log file")?,
    );

    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(target))
        .filter_level(log::LevelFilter::Info)
        .init();

    info!("Logging initialized, writing to: {}", log_file.display());
    Ok(())
}

async fn handle_query(query: &str, config: &Config) -> Result<()> {
    info!("Processing query: {}", query);

    // Load and render system prompt
    let system_prompt_template = load_system_prompt()?;
    let context = PromptContext::default();
    let system_prompt = render_prompt(&system_prompt_template, &context);

    // Create API client and send query
    let client = OpenAIClient::new(config)?;
    let result = client.query(&system_prompt, query).await?;

    // Print result to stdout (ZLE widget captures this)
    println!("{}", result);

    info!("Query successful, result: {}", result);
    Ok(())
}

fn handle_shell_init(shell: &str) -> Result<()> {
    match generate_init_script(shell) {
        Some(script) => {
            print!("{}", script);
            Ok(())
        }
        None => {
            let supported = shell::supported_shells().join(", ");
            Err(eyre::eyre!(
                "Unsupported shell: '{}'. Supported shells: {}",
                shell,
                supported
            ))
        }
    }
}

/// Get the log directory path
pub fn get_log_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("qai")
        .join("logs")
}

/// Get the log file path
pub fn get_log_file() -> PathBuf {
    get_log_dir().join("qai.log")
}

/// Join query words into a single string
pub fn join_query(words: &[String]) -> String {
    words.join(" ")
}

/// Process a command and return result (for testing)
pub async fn run_command(command: Option<&Commands>, config_path: Option<&PathBuf>) -> Result<()> {
    match command {
        Some(Commands::Query { query }) => {
            let config = Config::load(config_path).context("Failed to load configuration")?;
            let query_str = join_query(query);
            handle_query(&query_str, &config).await
        }
        Some(Commands::ShellInit { shell }) => handle_shell_init(shell),
        None => {
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup logging first (errors go to file, not interfere with stdout)
    if let Err(e) = setup_logging() {
        // Don't fail if logging setup fails, just continue
        eprintln!("Warning: Failed to setup logging: {}", e);
    }

    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle commands
    match &cli.command {
        Some(Commands::Query { query }) => {
            // Load configuration
            let config = Config::load(cli.config.as_ref()).context("Failed to load configuration")?;

            // Join query words into single string
            let query_str = query.join(" ");

            // Handle the query
            if let Err(e) = handle_query(&query_str, &config).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::ShellInit { shell }) => {
            if let Err(e) = handle_shell_init(shell) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            // No command provided, show help
            use clap::CommandFactory;
            Cli::command().print_help()?;
            println!();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_success_response(content: &str) -> String {
        format!(r#"{{"choices": [{{"message": {{"content": "{}"}}}}]}}"#, content)
    }

    #[test]
    fn test_get_log_dir() {
        let log_dir = get_log_dir();
        assert!(log_dir.ends_with("qai/logs"));
    }

    #[test]
    fn test_get_log_file() {
        let log_file = get_log_file();
        assert!(log_file.ends_with("qai.log"));
    }

    #[test]
    fn test_handle_shell_init_zsh() {
        let result = handle_shell_init("zsh");
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_shell_init_unsupported() {
        let result = handle_shell_init("fish");
        assert!(result.is_err());
        let error = result.unwrap_err().to_string();
        assert!(error.contains("Unsupported shell"));
        assert!(error.contains("fish"));
    }

    #[test]
    fn test_handle_shell_init_error_lists_supported() {
        let result = handle_shell_init("invalid");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("zsh"));
    }

    #[tokio::test]
    async fn test_handle_query_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(create_success_response("ls -la")))
            .mount(&mock_server)
            .await;

        let config = Config {
            api_key: Some("test-key".to_string()),
            api_base: mock_server.uri(),
            model: "gpt-4o-mini".to_string(),
            debug: false,
        };

        let result = handle_query("list files", &config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_query_api_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let config = Config {
            api_key: Some("test-key".to_string()),
            api_base: mock_server.uri(),
            model: "gpt-4o-mini".to_string(),
            debug: false,
        };

        let result = handle_query("test query", &config).await;
        assert!(result.is_err());
    }

    // Note: Cannot test "no API key" scenario without touching env vars
    // The OpenAIClient::new() reads QAI_API_KEY from environment
    // Tests for API key validation are in api.rs using new_with_base()

    #[test]
    fn test_join_query_single_word() {
        let words = vec!["test".to_string()];
        assert_eq!(join_query(&words), "test");
    }

    #[test]
    fn test_join_query_multiple_words() {
        let words = vec!["list".to_string(), "all".to_string(), "files".to_string()];
        assert_eq!(join_query(&words), "list all files");
    }

    #[test]
    fn test_join_query_empty() {
        let words: Vec<String> = vec![];
        assert_eq!(join_query(&words), "");
    }

    #[test]
    fn test_join_query_with_special_chars() {
        let words = vec!["find".to_string(), "*.txt".to_string()];
        assert_eq!(join_query(&words), "find *.txt");
    }

    #[test]
    fn test_get_log_dir_structure() {
        let log_dir = get_log_dir();
        let path_str = log_dir.to_string_lossy();
        assert!(path_str.contains("qai"));
        assert!(path_str.contains("logs"));
    }

    #[test]
    fn test_get_log_file_extension() {
        let log_file = get_log_file();
        assert!(log_file.extension().map(|e| e == "log").unwrap_or(false));
    }

    #[tokio::test]
    async fn test_run_command_shell_init_zsh() {
        let cmd = Commands::ShellInit {
            shell: "zsh".to_string(),
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_shell_init_unsupported() {
        let cmd = Commands::ShellInit {
            shell: "unsupported".to_string(),
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_command_none_shows_help() {
        // Running with no command should show help and succeed
        let result = run_command(None, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_query_with_mock() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(create_success_response("echo hello")))
            .mount(&mock_server)
            .await;

        // Create a temp config file
        let mut config_file = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        writeln!(
            config_file,
            "api_key: test-key\napi_base: {}\nmodel: gpt-4o-mini",
            mock_server.uri()
        )
        .unwrap();

        let cmd = Commands::Query {
            query: vec!["print".to_string(), "hello".to_string()],
        };
        let result = run_command(Some(&cmd), Some(&config_file.path().to_path_buf())).await;
        assert!(result.is_ok());
    }

    // Note: Cannot test "no API key" scenario in run_command without touching env vars
    // Config::get_api_key() reads QAI_API_KEY from environment first
}
