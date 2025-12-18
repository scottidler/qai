use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// OpenAI API key (can also be set via QAI_API_KEY env var)
    pub api_key: Option<String>,
    /// Model to use (default: gpt-4o-mini)
    pub model: String,
    /// API base URL (default: https://api.openai.com/v1)
    pub api_base: String,
    /// Enable debug mode
    pub debug: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            model: "gpt-4o-mini".to_string(),
            api_base: "https://api.openai.com/v1".to_string(),
            debug: false,
        }
    }
}

impl Config {
    /// Get API key from environment variable or config file
    pub fn get_api_key(&self) -> Option<String> {
        // Environment variable takes precedence
        if let Ok(key) = std::env::var("QAI_API_KEY")
            && !key.is_empty()
        {
            return Some(key);
        }
        // Fall back to config file
        self.api_key.clone()
    }

    /// Get API key from config only (for testing without touching env vars)
    #[cfg(test)]
    pub fn get_api_key_from_config_only(&self) -> Option<String> {
        self.api_key.clone()
    }

    /// Check if environment variable would provide the key (for testing)
    #[cfg(test)]
    pub fn would_env_provide_key(env_value: Option<&str>) -> bool {
        match env_value {
            Some(key) if !key.is_empty() => true,
            _ => false,
        }
    }

    /// Load configuration with fallback chain
    pub fn load(config_path: Option<&PathBuf>) -> Result<Self> {
        // If explicit config path provided, try to load it
        if let Some(path) = config_path {
            return Self::load_from_file(path).context(format!("Failed to load config from {}", path.display()));
        }

        // Try primary location: ~/.config/qai/qai.yml
        if let Some(config_dir) = dirs::config_dir() {
            let project_name = env!("CARGO_PKG_NAME");
            let primary_config = config_dir.join(project_name).join(format!("{}.yml", project_name));
            if primary_config.exists() {
                match Self::load_from_file(&primary_config) {
                    Ok(config) => return Ok(config),
                    Err(e) => {
                        log::warn!("Failed to load config from {}: {}", primary_config.display(), e);
                    }
                }
            }
        }

        // Try fallback location: ./qai.yml
        let project_name = env!("CARGO_PKG_NAME");
        let fallback_config = PathBuf::from(format!("{}.yml", project_name));
        if fallback_config.exists() {
            match Self::load_from_file(&fallback_config) {
                Ok(config) => return Ok(config),
                Err(e) => {
                    log::warn!("Failed to load config from {}: {}", fallback_config.display(), e);
                }
            }
        }

        // No config file found, use defaults
        log::info!("No config file found, using defaults");
        Ok(Self::default())
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path).context("Failed to read config file")?;

        let config: Self = serde_yaml::from_str(&content).context("Failed to parse config file")?;

        log::info!("Loaded config from: {}", path.as_ref().display());
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.api_base, "https://api.openai.com/v1");
        assert!(!config.debug);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_load_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
api_key: test-key-123
model: gpt-4o
api_base: https://custom.api.com/v1
debug: true
"#
        )
        .unwrap();

        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert_eq!(config.api_key, Some("test-key-123".to_string()));
        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.api_base, "https://custom.api.com/v1");
        assert!(config.debug);
    }

    #[test]
    fn test_load_partial_config_uses_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
model: custom-model
"#
        )
        .unwrap();

        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert_eq!(config.model, "custom-model");
        // Should use defaults for unspecified fields
        assert_eq!(config.api_base, "https://api.openai.com/v1");
        assert!(!config.debug);
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_load_empty_config_uses_defaults() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "").unwrap();

        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.api_base, "https://api.openai.com/v1");
    }

    #[test]
    fn test_load_nonexistent_file_fails() {
        let path = PathBuf::from("/nonexistent/path/config.yml");
        let result = Config::load(Some(&path));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_invalid_yaml_fails() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "invalid: yaml: content: [").unwrap();

        let result = Config::load(Some(&file.path().to_path_buf()));
        assert!(result.is_err());
    }

    #[test]
    fn test_get_api_key_from_config_only() {
        let config = Config {
            api_key: Some("config-key".to_string()),
            ..Default::default()
        };

        // Test config-only path (doesn't touch env vars)
        assert_eq!(config.get_api_key_from_config_only(), Some("config-key".to_string()));
    }

    #[test]
    fn test_get_api_key_from_config_only_none() {
        let config = Config::default();
        assert!(config.get_api_key_from_config_only().is_none());
    }

    #[test]
    fn test_would_env_provide_key_with_value() {
        assert!(Config::would_env_provide_key(Some("my-key")));
    }

    #[test]
    fn test_would_env_provide_key_empty() {
        assert!(!Config::would_env_provide_key(Some("")));
    }

    #[test]
    fn test_would_env_provide_key_none() {
        assert!(!Config::would_env_provide_key(None));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            api_key: Some("test-key".to_string()),
            model: "test-model".to_string(),
            api_base: "https://test.api.com".to_string(),
            debug: true,
        };

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("api_key: test-key"));
        assert!(yaml.contains("model: test-model"));
        assert!(yaml.contains("api_base: https://test.api.com"));
        assert!(yaml.contains("debug: true"));
    }

    #[test]
    fn test_load_with_no_config_path_returns_defaults() {
        // When no config path is provided and no config files exist in standard locations,
        // should return defaults. This test may find an actual config file if one exists,
        // so we just verify it doesn't crash
        let result = Config::load(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_default_model() {
        let config = Config::default();
        assert_eq!(config.model, "gpt-4o-mini");
    }

    #[test]
    fn test_config_default_api_base() {
        let config = Config::default();
        assert_eq!(config.api_base, "https://api.openai.com/v1");
    }

    #[test]
    fn test_config_default_debug_false() {
        let config = Config::default();
        assert!(!config.debug);
    }

    #[test]
    fn test_config_can_override_all_fields() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
api_key: my-key
model: gpt-4
api_base: https://my.api.com
debug: true
"#
        )
        .unwrap();

        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert_eq!(config.api_key, Some("my-key".to_string()));
        assert_eq!(config.model, "gpt-4");
        assert_eq!(config.api_base, "https://my.api.com");
        assert!(config.debug);
    }

    #[test]
    fn test_config_debug_field_parsing() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "debug: true").unwrap();
        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert!(config.debug);

        let mut file2 = NamedTempFile::new().unwrap();
        writeln!(file2, "debug: false").unwrap();
        let config2 = Config::load(Some(&file2.path().to_path_buf())).unwrap();
        assert!(!config2.debug);
    }

    #[test]
    fn test_config_api_key_can_be_null() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "api_key: null").unwrap();
        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_config_preserves_whitespace_in_values() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "model: \"  gpt-4  \"").unwrap();
        let config = Config::load(Some(&file.path().to_path_buf())).unwrap();
        assert_eq!(config.model, "  gpt-4  ");
    }
}
