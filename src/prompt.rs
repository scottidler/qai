use eyre::{Context, Result};
use std::fs;

/// Default system prompt embedded at compile time
const DEFAULT_SYSTEM_PROMPT: &str = include_str!("../prompts/system.pmt");

/// Context variables for prompt template substitution
pub struct PromptContext {
    pub shell: String,
    pub os: String,
    pub cwd: String,
}

impl Default for PromptContext {
    fn default() -> Self {
        Self {
            shell: std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string()),
            os: std::env::consts::OS.to_string(),
            cwd: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| ".".to_string()),
        }
    }
}

/// Load prompt from a specific file path
pub fn load_prompt_from_file(path: &std::path::Path) -> Result<String> {
    log::info!("Loading prompt from: {}", path.display());
    let content = fs::read_to_string(path).context(format!("Failed to read prompt file: {}", path.display()))?;
    Ok(content)
}

/// Load the system prompt with the following priority:
/// 1. User override: ~/.config/qai/prompts/system.pmt
/// 2. Embedded default
pub fn load_system_prompt() -> Result<String> {
    // Check for user override
    if let Some(config_dir) = dirs::config_dir() {
        let user_prompt = config_dir
            .join(env!("CARGO_PKG_NAME"))
            .join("prompts")
            .join("system.pmt");

        if user_prompt.exists() {
            return load_prompt_from_file(&user_prompt);
        }
    }

    // Use embedded default
    log::debug!("Using embedded default prompt");
    Ok(DEFAULT_SYSTEM_PROMPT.to_string())
}

/// Substitute template variables in the prompt
pub fn render_prompt(template: &str, context: &PromptContext) -> String {
    template
        .replace("{{shell}}", &context.shell)
        .replace("{{os}}", &context.os)
        .replace("{{cwd}}", &context.cwd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_prompt_exists() {
        // Verify the prompt has content
        assert!(DEFAULT_SYSTEM_PROMPT.len() > 100);
    }

    #[test]
    fn test_default_prompt_contains_placeholders() {
        assert!(DEFAULT_SYSTEM_PROMPT.contains("{{shell}}"));
        assert!(DEFAULT_SYSTEM_PROMPT.contains("{{os}}"));
        assert!(DEFAULT_SYSTEM_PROMPT.contains("{{cwd}}"));
    }

    #[test]
    fn test_default_prompt_contains_instructions() {
        // Should contain key instruction elements
        let prompt = DEFAULT_SYSTEM_PROMPT.to_lowercase();
        assert!(prompt.contains("shell") || prompt.contains("command"));
    }

    #[test]
    fn test_render_prompt() {
        let template = "Shell: {{shell}}, OS: {{os}}, CWD: {{cwd}}";
        let context = PromptContext {
            shell: "/bin/zsh".to_string(),
            os: "linux".to_string(),
            cwd: "/home/user".to_string(),
        };

        let result = render_prompt(template, &context);
        assert_eq!(result, "Shell: /bin/zsh, OS: linux, CWD: /home/user");
    }

    #[test]
    fn test_render_prompt_replaces_all_occurrences() {
        let template = "{{shell}} and {{shell}} again";
        let context = PromptContext {
            shell: "zsh".to_string(),
            os: "linux".to_string(),
            cwd: "/tmp".to_string(),
        };

        let result = render_prompt(template, &context);
        assert_eq!(result, "zsh and zsh again");
    }

    #[test]
    fn test_render_prompt_no_placeholders() {
        let template = "Plain text without placeholders";
        let context = PromptContext {
            shell: "zsh".to_string(),
            os: "linux".to_string(),
            cwd: "/tmp".to_string(),
        };

        let result = render_prompt(template, &context);
        assert_eq!(result, "Plain text without placeholders");
    }

    #[test]
    fn test_render_prompt_empty_template() {
        let template = "";
        let context = PromptContext {
            shell: "zsh".to_string(),
            os: "linux".to_string(),
            cwd: "/tmp".to_string(),
        };

        let result = render_prompt(template, &context);
        assert_eq!(result, "");
    }

    #[test]
    fn test_render_prompt_special_characters_in_values() {
        let template = "Dir: {{cwd}}";
        let context = PromptContext {
            shell: "zsh".to_string(),
            os: "linux".to_string(),
            cwd: "/home/user/my project (1)/test".to_string(),
        };

        let result = render_prompt(template, &context);
        assert_eq!(result, "Dir: /home/user/my project (1)/test");
    }

    #[test]
    fn test_prompt_context_default() {
        let context = PromptContext::default();

        // Should have non-empty values (actual values depend on system)
        assert!(!context.shell.is_empty());
        assert!(!context.os.is_empty());
        assert!(!context.cwd.is_empty());
    }

    #[test]
    fn test_prompt_context_os_is_valid() {
        let context = PromptContext::default();

        // OS should be one of the known values
        let known_os = ["linux", "macos", "windows", "ios", "android", "freebsd"];
        assert!(
            known_os.iter().any(|&os| context.os.contains(os)),
            "Unknown OS: {}",
            context.os
        );
    }

    #[test]
    fn test_load_system_prompt_returns_content() {
        let result = load_system_prompt();
        assert!(result.is_ok());

        let prompt = result.unwrap();
        assert!(!prompt.is_empty());
    }

    #[test]
    fn test_load_system_prompt_has_placeholders() {
        let prompt = load_system_prompt().unwrap();

        // Should have placeholders for rendering
        assert!(prompt.contains("{{shell}}") || prompt.contains("{{os}}") || prompt.contains("{{cwd}}"));
    }

    #[test]
    fn test_default_system_prompt_is_valid_template() {
        // The default prompt should be renderable
        let context = PromptContext::default();
        let rendered = render_prompt(DEFAULT_SYSTEM_PROMPT, &context);
        // After rendering, placeholders should be gone
        assert!(!rendered.contains("{{shell}}"));
        assert!(!rendered.contains("{{os}}"));
        assert!(!rendered.contains("{{cwd}}"));
    }

    #[test]
    fn test_prompt_context_shell_has_path() {
        let context = PromptContext::default();
        // Shell should typically have a path-like value or be "bash"
        assert!(context.shell.contains('/') || context.shell == "bash");
    }

    #[test]
    fn test_render_prompt_preserves_non_placeholder_braces() {
        let template = "Test {regular} braces and {{shell}}";
        let context = PromptContext {
            shell: "zsh".to_string(),
            os: "linux".to_string(),
            cwd: "/tmp".to_string(),
        };
        let result = render_prompt(template, &context);
        assert_eq!(result, "Test {regular} braces and zsh");
    }

    #[test]
    fn test_load_prompt_from_file_success() {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let content = "Custom prompt: {{shell}}";
        writeln!(file, "{}", content).unwrap();

        let result = load_prompt_from_file(file.path());
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Custom prompt"));
    }

    #[test]
    fn test_load_prompt_from_file_not_found() {
        let path = std::path::Path::new("/nonexistent/path/prompt.pmt");
        let result = load_prompt_from_file(path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Failed to read prompt file"));
    }

    #[test]
    fn test_load_prompt_from_file_empty() {
        use std::io::Write;
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(file, "").unwrap();

        let result = load_prompt_from_file(file.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }
}
