use eyre::{Context, Result};
use log::info;
use std::fs;
use std::path::PathBuf;

mod api;
mod cli;
mod config;
mod history;
mod prompt;
mod shell;
mod tools;

use api::{OpenAIClient, validate_api_key_from_config};
use cli::{Cli, Commands, check_api_key_configured, check_fzf_status};
use config::Config;
use history::HistoryStore;
use prompt::{PromptContext, load_system_prompt, render_prompt};
use shell::generate_init_script;
use tools::ToolCache;

#[cfg(not(tarpaulin_include))]
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

async fn handle_query(query: &str, config: &Config, multi: bool, count: usize) -> Result<()> {
    info!("Processing query: {} (multi: {}, count: {})", query, multi, count);

    // Load and render system prompt
    let system_prompt_template = if multi { load_multi_result_prompt(count)? } else { load_system_prompt()? };
    let context = PromptContext::default();
    let system_prompt = render_prompt(&system_prompt_template, &context);

    // Create API client and send query
    let client = OpenAIClient::new(config)?;
    let result = if multi {
        client.query_multi(&system_prompt, query, count).await?
    } else {
        client.query(&system_prompt, query).await?
    };

    // Print result to stdout (ZLE widget captures this)
    println!("{}", result);

    info!("Query successful, result: {}", result);
    Ok(())
}

/// Load multi-result system prompt
fn load_multi_result_prompt(count: usize) -> Result<String> {
    // Check for custom multi prompt
    if let Some(config_dir) = dirs::config_dir() {
        let prompt_file = config_dir.join("qai").join("system-prompt-multi.txt");
        if prompt_file.exists() {
            let template =
                fs::read_to_string(&prompt_file).context("Failed to read custom multi-result system prompt")?;
            return Ok(template.replace("{{count}}", &count.to_string()));
        }
    }

    // Default multi-result prompt
    Ok(format!(
        r#"You are a shell command assistant. Convert natural language queries into shell commands.

CRITICAL RULES:
1. Return EXACTLY {} command options, one per line
2. Return ONLY the commands, no explanations, no numbering, no backticks
3. Commands should be variations that accomplish the user's goal
4. Order from most likely/common to least
5. Each command should be complete and executable

Environment:
- Shell: {{{{shell}}}}
- OS: {{{{os}}}}
- Working directory: {{{{cwd}}}}"#,
        count
    ))
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

async fn handle_validate_api(config: &Config) -> Result<()> {
    match validate_api_key_from_config(config).await {
        Ok(()) => {
            println!("✅ API key is valid");
            Ok(())
        }
        Err(e) => Err(eyre::eyre!("{}", e)),
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

/// Build status footer for --help output
pub fn build_status_footer() -> String {
    let mut lines = vec![];

    // fzf status
    let (fzf_available, fzf_version) = check_fzf_status();
    if fzf_available {
        lines.push(format!("TOOLS: ✅ fzf       {}", fzf_version.unwrap_or_default()));
    } else {
        lines.push("TOOLS: ⚠️  fzf       not found (single-result mode only)".to_string());
    }

    // API key status
    if check_api_key_configured() {
        lines.push("API:   ✅ key configured".to_string());
    } else {
        lines.push("API:   ❌ key not configured (set QAI_API_KEY or add to config)".to_string());
    }

    lines.join("\n")
}

/// Join query words into a single string
pub fn join_query(words: &[String]) -> String {
    words.join(" ")
}

/// Handle history command
fn handle_history(limit: usize, patterns: bool, stats: bool, clear: bool) -> Result<()> {
    let mut store = HistoryStore::new().context("Failed to open history store")?;

    if clear {
        store.clear()?;
        println!("History cleared.");
        return Ok(());
    }

    if stats {
        let stats = store.stats()?;
        println!("History Statistics:");
        println!("  Total queries:    {}", stats.total_queries);
        println!("  Unique patterns:  {}", stats.unique_patterns);
        println!("  With preferences: {}", stats.patterns_with_preference);
        return Ok(());
    }

    if patterns {
        let patterns = store.get_patterns_by_usage();
        if patterns.is_empty() {
            println!("No patterns recorded yet.");
            return Ok(());
        }

        println!("Query Patterns (by usage):\n");
        for pattern in patterns.iter().take(limit) {
            println!(
                "  \"{}\" (used {} times)",
                pattern.normalized_query, pattern.query_count
            );
            if let Some(preferred) = &pattern.preferred_command {
                println!("    → preferred: {}", preferred);
            }
            println!();
        }
        return Ok(());
    }

    // Show recent queries
    let records = store.get_recent_queries(limit)?;
    if records.is_empty() {
        println!("No queries recorded yet.");
        return Ok(());
    }

    println!("Recent Queries:\n");
    for record in records {
        let time = record.timestamp.format("%Y-%m-%d %H:%M");
        println!("  [{}] \"{}\"", time, record.query);
        if let Some(cmd) = record.final_command() {
            let status = if record.executed { "✓" } else { " " };
            println!("    {} {}", status, cmd);
        }
        println!();
    }

    Ok(())
}

/// Handle tools command
fn handle_tools(refresh: bool, clear: bool) -> Result<()> {
    let mut cache = ToolCache::load();

    if clear {
        cache.clear();
        cache.save()?;
        println!("Tool cache cleared.");
        return Ok(());
    }

    if refresh {
        cache.clear();
        // Probe some common modern tools
        let tools_to_check = [
            "eza",
            "exa",
            "rg",
            "fd",
            "bat",
            "delta",
            "jq",
            "yq",
            "fzf",
            "zoxide",
            "starship",
            "dust",
            "procs",
            "bottom",
            "btm",
            "sd",
            "hyperfine",
            "tokei",
            "duf",
            "broot",
            "httpie",
            "http",
            "xh",
            "curlie",
            "glow",
            "mdcat",
            "navi",
            "tldr",
            "fuck",
            "thefuck",
            "atuin",
            "mcfly",
            "direnv",
            "mise",
            "asdf",
            "fnm",
            "nvm",
            "pyenv",
            "rbenv",
        ];

        for tool in tools_to_check {
            cache.is_available(tool);
        }
        cache.save()?;
        println!("Tool cache refreshed.");
    }

    // Display cache contents
    let stats = cache.stats();
    println!("Tool Cache Statistics:");
    println!("  Available tools:     {}", stats.available_count);
    println!("  Unavailable tools:   {}", stats.unavailable_count);
    println!("  Modern tools found:  {}", stats.modern_tools_count);

    if stats.available_count > 0 {
        println!("\nAvailable modern tools:");
        let prompt_hint = cache.available_tools_for_prompt();
        if !prompt_hint.is_empty() {
            // Extract just the tool names from the hint
            if let Some(tools_part) = prompt_hint.strip_prefix("User has these modern tools installed: ")
                && let Some(tools_only) = tools_part.strip_suffix("\nPrefer these when appropriate.\n")
            {
                println!("  {}", tools_only);
            }
        } else {
            println!("  (only standard tools detected)");
        }
    }

    println!("\nCache location: {}", ToolCache::cache_path().display());

    Ok(())
}

/// Process a command and return result (for testing)
pub async fn run_command(command: Option<&Commands>, config_path: Option<&PathBuf>) -> Result<()> {
    match command {
        Some(Commands::Query { query, multi, count }) => {
            let config = Config::load(config_path).context("Failed to load configuration")?;
            let query_str = join_query(query);
            handle_query(&query_str, &config, *multi, *count).await
        }
        Some(Commands::ShellInit { shell }) => handle_shell_init(shell),
        Some(Commands::ValidateApi) => {
            let config = Config::load(config_path).context("Failed to load configuration")?;
            handle_validate_api(&config).await
        }
        Some(Commands::History {
            limit,
            patterns,
            stats,
            clear,
        }) => handle_history(*limit, *patterns, *stats, *clear),
        Some(Commands::Tools { refresh, clear }) => handle_tools(*refresh, *clear),
        None => {
            use clap::CommandFactory;
            let after_help = build_status_footer();
            Cli::command().after_help(after_help).print_help()?;
            println!();
            Ok(())
        }
    }
}

#[tokio::main]
#[cfg(not(tarpaulin_include))]
async fn main() -> Result<()> {
    // Setup logging first (errors go to file, not interfere with stdout)
    if let Err(e) = setup_logging() {
        // Don't fail if logging setup fails, just continue
        eprintln!("Warning: Failed to setup logging: {}", e);
    }

    // Parse CLI arguments with status info
    let cli = Cli::parse_with_status();

    // Handle commands
    match &cli.command {
        Some(Commands::Query { query, multi, count }) => {
            // Load configuration
            let config = Config::load(cli.config.as_ref()).context("Failed to load configuration")?;

            // Join query words into single string
            let query_str = query.join(" ");

            // Handle the query
            if let Err(e) = handle_query(&query_str, &config, *multi, *count).await {
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
        Some(Commands::ValidateApi) => {
            let config = Config::load(cli.config.as_ref()).context("Failed to load configuration")?;
            if let Err(e) = handle_validate_api(&config).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::History {
            limit,
            patterns,
            stats,
            clear,
        }) => {
            if let Err(e) = handle_history(*limit, *patterns, *stats, *clear) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Tools { refresh, clear }) => {
            if let Err(e) = handle_tools(*refresh, *clear) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            // No command provided, show help with status
            use clap::CommandFactory;
            let after_help = build_status_footer();
            Cli::command().after_help(after_help).print_help()?;
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

        let result = handle_query("list files", &config, false, 1).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_handle_query_multi_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(create_success_response("ls -la\\nls -lh\\nls")))
            .mount(&mock_server)
            .await;

        let config = Config {
            api_key: Some("test-key".to_string()),
            api_base: mock_server.uri(),
            model: "gpt-4o-mini".to_string(),
            debug: false,
        };

        let result = handle_query("list files", &config, true, 3).await;
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

        let result = handle_query("test query", &config, false, 1).await;
        assert!(result.is_err());
    }

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
            multi: false,
            count: 5,
        };
        let result = run_command(Some(&cmd), Some(&config_file.path().to_path_buf())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_query_multi_with_mock() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(create_success_response("echo a\\necho b\\necho c")),
            )
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
            query: vec!["print".to_string(), "letters".to_string()],
            multi: true,
            count: 3,
        };
        let result = run_command(Some(&cmd), Some(&config_file.path().to_path_buf())).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_validate_api_with_mock() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data": []}"#))
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

        let cmd = Commands::ValidateApi;
        let result = run_command(Some(&cmd), Some(&config_file.path().to_path_buf())).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_build_status_footer_contains_sections() {
        let footer = build_status_footer();
        // Should contain tools and API sections
        assert!(footer.contains("TOOLS:") || footer.contains("fzf"));
        assert!(footer.contains("API:"));
    }

    #[test]
    fn test_load_multi_result_prompt_default() {
        let prompt = load_multi_result_prompt(5).unwrap();
        assert!(prompt.contains("EXACTLY 5 command options"));
        assert!(prompt.contains("one per line"));
        assert!(prompt.contains("{{shell}}"));
        assert!(prompt.contains("{{os}}"));
        assert!(prompt.contains("{{cwd}}"));
    }

    #[test]
    fn test_load_multi_result_prompt_different_count() {
        let prompt = load_multi_result_prompt(3).unwrap();
        assert!(prompt.contains("EXACTLY 3 command options"));
    }

    #[tokio::test]
    async fn test_handle_validate_api_unauthorized() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(401).set_body_string(r#"{"error": "unauthorized"}"#))
            .mount(&mock_server)
            .await;

        let config = Config {
            api_key: Some("invalid-key".to_string()),
            api_base: mock_server.uri(),
            model: "gpt-4o-mini".to_string(),
            debug: false,
        };

        let result = handle_validate_api(&config).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid") || err.contains("invalid"));
    }

    #[tokio::test]
    async fn test_handle_validate_api_forbidden() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(403).set_body_string(r#"{"error": "forbidden"}"#))
            .mount(&mock_server)
            .await;

        let config = Config {
            api_key: Some("limited-key".to_string()),
            api_base: mock_server.uri(),
            model: "gpt-4o-mini".to_string(),
            debug: false,
        };

        let result = handle_validate_api(&config).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Access") || err.contains("denied") || err.contains("permissions"));
    }

    #[tokio::test]
    async fn test_handle_validate_api_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data": []}"#))
            .mount(&mock_server)
            .await;

        let config = Config {
            api_key: Some("valid-key".to_string()),
            api_base: mock_server.uri(),
            model: "gpt-4o-mini".to_string(),
            debug: false,
        };

        let result = handle_validate_api(&config).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_load_multi_result_prompt_single() {
        let prompt = load_multi_result_prompt(1).unwrap();
        assert!(prompt.contains("EXACTLY 1 command options"));
    }

    #[test]
    fn test_load_multi_result_prompt_ten() {
        let prompt = load_multi_result_prompt(10).unwrap();
        assert!(prompt.contains("EXACTLY 10 command options"));
    }

    #[test]
    fn test_build_status_footer_fzf_status() {
        let footer = build_status_footer();
        // Should mention fzf status
        assert!(footer.contains("fzf"));
    }

    #[test]
    fn test_build_status_footer_api_status() {
        let footer = build_status_footer();
        // Should contain API key status
        assert!(footer.contains("API:"));
    }

    #[test]
    fn test_get_log_dir_not_empty() {
        let log_dir = get_log_dir();
        assert!(!log_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_get_log_file_not_empty() {
        let log_file = get_log_file();
        assert!(!log_file.as_os_str().is_empty());
    }

    #[test]
    fn test_join_query_preserves_spaces_in_words() {
        // Words themselves shouldn't have spaces, but test the behavior
        let words = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(join_query(&words), "a b c");
    }

    #[tokio::test]
    async fn test_run_command_history_stats() {
        let cmd = Commands::History {
            limit: 10,
            patterns: false,
            stats: true,
            clear: false,
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_history_default() {
        let cmd = Commands::History {
            limit: 5,
            patterns: false,
            stats: false,
            clear: false,
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_history_patterns() {
        let cmd = Commands::History {
            limit: 10,
            patterns: true,
            stats: false,
            clear: false,
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_tools_default() {
        let cmd = Commands::Tools {
            refresh: false,
            clear: false,
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_tools_refresh() {
        let cmd = Commands::Tools {
            refresh: true,
            clear: false,
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_command_tools_clear() {
        let cmd = Commands::Tools {
            refresh: false,
            clear: true,
        };
        let result = run_command(Some(&cmd), None).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_tools_display() {
        // Just verify the function runs without crashing
        let result = handle_tools(false, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_tools_clear() {
        let result = handle_tools(false, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_tools_refresh() {
        let result = handle_tools(true, false);
        assert!(result.is_ok());
    }
}
