//! Tool Discovery & Validation for qai
//!
//! Discovers available CLI tools on the system and validates
//! that commands use binaries that exist.

use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

/// Standard Unix tools that are always available
const STANDARD_TOOLS: &[&str] = &[
    "ls", "cat", "grep", "find", "awk", "sed", "sort", "uniq", "head", "tail", "cut", "wc", "du", "df", "ps", "top",
    "chmod", "chown", "cp", "mv", "rm", "mkdir", "rmdir", "curl", "wget", "tar", "gzip", "gunzip", "zip", "unzip",
    "echo", "printf", "test", "true", "false", "cd", "pwd", "env", "export", "source", "sh", "bash", "zsh",
];

/// Words to skip when extracting binary from command
#[allow(dead_code)]
const SKIP_WORDS: &[&str] = &["sudo", "env", "time", "nice", "nohup", "strace", "ltrace", "doas"];

/// Parsed dual-list response from AI
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct DualCommandList {
    /// Commands using modern tools (may be empty)
    pub modern: Vec<String>,
    /// Commands using standard Unix tools (should always have content)
    pub standard: Vec<String>,
}

#[allow(dead_code)]
impl DualCommandList {
    /// Parse AI response into dual lists
    pub fn parse(response: &str) -> Self {
        let mut result = Self::default();
        let mut current_section: Option<&str> = None;

        for line in response.lines() {
            let line = line.trim();

            // Check for section markers (case-insensitive)
            let lower = line.to_lowercase();
            if lower == "modern:" || lower.starts_with("modern:") {
                current_section = Some("modern");
                continue;
            }
            if lower == "standard:" || lower.starts_with("standard:") {
                current_section = Some("standard");
                continue;
            }

            // Skip empty lines, comments, and common noise
            if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
                continue;
            }

            // Add to appropriate section
            match current_section {
                Some("modern") => result.modern.push(line.to_string()),
                Some("standard") => result.standard.push(line.to_string()),
                Some(_) => {
                    // Unknown section - treat as standard
                    result.standard.push(line.to_string());
                }
                None => {
                    // No section marker yet - treat as standard (fallback)
                    result.standard.push(line.to_string());
                }
            }
        }

        result
    }

    /// Get all commands, modern first, then standard
    pub fn all_commands(&self) -> Vec<String> {
        let mut all = self.modern.clone();
        all.extend(self.standard.clone());
        all
    }

    /// Check if the response is empty
    pub fn is_empty(&self) -> bool {
        self.modern.is_empty() && self.standard.is_empty()
    }

    /// Total number of commands
    pub fn len(&self) -> usize {
        self.modern.len() + self.standard.len()
    }
}

/// Cache for tool availability checks
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ToolCache {
    /// Tools confirmed to exist on this system
    pub available: HashSet<String>,

    /// Tools confirmed NOT to exist (avoid re-checking)
    pub unavailable: HashSet<String>,

    /// Cache version for format changes
    #[serde(default)]
    pub version: u32,

    /// Whether cache has been modified
    #[serde(skip)]
    dirty: bool,
}

impl ToolCache {
    const CACHE_VERSION: u32 = 1;

    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            available: HashSet::new(),
            unavailable: HashSet::new(),
            version: Self::CACHE_VERSION,
            dirty: false,
        }
    }

    /// Load cache from disk
    pub fn load() -> Self {
        let cache_path = Self::cache_path();
        if let Ok(content) = fs::read_to_string(&cache_path)
            && let Ok(mut cache) = serde_json::from_str::<Self>(&content)
            && cache.version == Self::CACHE_VERSION
        {
            cache.dirty = false;
            return cache;
        }
        Self::new()
    }

    /// Load cache from a specific path (for testing)
    #[allow(dead_code)]
    pub fn load_from(path: &PathBuf) -> Self {
        if let Ok(content) = fs::read_to_string(path)
            && let Ok(mut cache) = serde_json::from_str::<Self>(&content)
            && cache.version == Self::CACHE_VERSION
        {
            cache.dirty = false;
            return cache;
        }
        Self::new()
    }

    /// Save cache to disk (if dirty)
    pub fn save(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        self.save_to(&Self::cache_path())
    }

    /// Save cache to a specific path (if dirty)
    pub fn save_to(&mut self, path: &PathBuf) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create cache directory")?;
        }
        let content = serde_json::to_string_pretty(self).context("Failed to serialize tool cache")?;
        fs::write(path, content).context("Failed to write tool cache")?;
        self.dirty = false;
        Ok(())
    }

    /// Get the default cache path
    pub fn cache_path() -> PathBuf {
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("qai")
            .join("tools.json")
    }

    /// Check if a binary is available, using cache
    pub fn is_available(&mut self, binary: &str) -> bool {
        // Fast path: already in cache
        if self.available.contains(binary) {
            return true;
        }
        if self.unavailable.contains(binary) {
            return false;
        }

        // Slow path: check PATH using which
        let exists = which::which(binary).is_ok();

        // Update cache
        if exists {
            self.available.insert(binary.to_string());
        } else {
            self.unavailable.insert(binary.to_string());
        }
        self.dirty = true;

        exists
    }

    /// Extract the primary binary from a command string
    /// Handles: sudo, env VAR=x, time, nice, flags like -n, numeric args, etc.
    #[allow(dead_code)]
    pub fn extract_binary(cmd: &str) -> Option<&str> {
        cmd.split_whitespace().find(|word| {
            !word.contains('=')
                && !word.starts_with('-')
                && !word.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                && !SKIP_WORDS.contains(word)
        })
    }

    /// Filter commands to only those with available binaries
    /// Returns (available_commands, unavailable_commands)
    #[allow(dead_code)]
    pub fn filter_commands(&mut self, commands: &[String]) -> (Vec<String>, Vec<String>) {
        let mut available = Vec::new();
        let mut unavailable = Vec::new();

        for cmd in commands {
            let is_available = Self::extract_binary(cmd)
                .map(|bin| self.is_available(bin))
                .unwrap_or(true); // If can't parse, assume available

            if is_available {
                available.push(cmd.clone());
            } else {
                unavailable.push(cmd.clone());
            }
        }

        (available, unavailable)
    }

    /// Process dual-list response: validate and merge
    #[allow(dead_code)]
    pub fn process_response(&mut self, response: &DualCommandList) -> Vec<String> {
        // Filter modern commands to only available ones
        let (available_modern, _) = self.filter_commands(&response.modern);

        // Standard commands should always be available, but validate anyway
        let (available_standard, _) = self.filter_commands(&response.standard);

        // Combine: available modern first, then standard
        let mut result = available_modern;
        result.extend(available_standard);

        // Ensure we have at least something
        if result.is_empty() {
            // Fallback to unfiltered standard (they should work)
            return response.standard.clone();
        }

        result
    }

    /// Get list of available modern tools for prompt enhancement
    pub fn available_tools_for_prompt(&self) -> String {
        if self.available.is_empty() {
            return String::new();
        }

        // Filter to "interesting" modern tools (not standard Unix)
        let standard_set: HashSet<&str> = STANDARD_TOOLS.iter().copied().collect();

        let mut modern_tools: Vec<&String> = self
            .available
            .iter()
            .filter(|t| !standard_set.contains(t.as_str()))
            .collect();

        if modern_tools.is_empty() {
            return String::new();
        }

        // Sort for consistent output
        modern_tools.sort();

        format!(
            "User has these modern tools installed: {}\nPrefer these when appropriate.\n",
            modern_tools.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.available.clear();
        self.unavailable.clear();
        self.dirty = true;
    }

    /// Get statistics about the cache
    pub fn stats(&self) -> ToolStats {
        let standard_set: HashSet<&str> = STANDARD_TOOLS.iter().copied().collect();
        let modern_count = self
            .available
            .iter()
            .filter(|t| !standard_set.contains(t.as_str()))
            .count();

        ToolStats {
            available_count: self.available.len(),
            unavailable_count: self.unavailable.len(),
            modern_tools_count: modern_count,
        }
    }

    /// Check if the cache is dirty (has unsaved changes)
    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the cache as dirty
    #[allow(dead_code)]
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}

/// Statistics about the tool cache
#[derive(Debug)]
pub struct ToolStats {
    pub available_count: usize,
    pub unavailable_count: usize,
    pub modern_tools_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // DualCommandList tests

    #[test]
    fn test_dual_command_list_parse_both_sections() {
        let response = r#"
MODERN:
fd -e rs
rg --files

STANDARD:
find . -name "*.rs"
ls -la
"#;
        let parsed = DualCommandList::parse(response);
        assert_eq!(parsed.modern.len(), 2);
        assert_eq!(parsed.standard.len(), 2);
        assert_eq!(parsed.modern[0], "fd -e rs");
        assert_eq!(parsed.standard[0], "find . -name \"*.rs\"");
    }

    #[test]
    fn test_dual_command_list_parse_empty_modern() {
        let response = r#"
MODERN:

STANDARD:
ls -la
find .
"#;
        let parsed = DualCommandList::parse(response);
        assert!(parsed.modern.is_empty());
        assert_eq!(parsed.standard.len(), 2);
    }

    #[test]
    fn test_dual_command_list_parse_case_insensitive() {
        let response = r#"
modern:
fd -e rs

standard:
find .
"#;
        let parsed = DualCommandList::parse(response);
        assert_eq!(parsed.modern.len(), 1);
        assert_eq!(parsed.standard.len(), 1);
    }

    #[test]
    fn test_dual_command_list_parse_no_sections() {
        let response = r#"
ls -la
find . -name "*.rs"
"#;
        let parsed = DualCommandList::parse(response);
        // Without section markers, all go to standard
        assert!(parsed.modern.is_empty());
        assert_eq!(parsed.standard.len(), 2);
    }

    #[test]
    fn test_dual_command_list_parse_skips_empty_and_comments() {
        let response = r#"
MODERN:
# This is a comment
fd -e rs

# Another comment

STANDARD:
find .
"#;
        let parsed = DualCommandList::parse(response);
        assert_eq!(parsed.modern.len(), 1);
        assert_eq!(parsed.standard.len(), 1);
    }

    #[test]
    fn test_dual_command_list_parse_skips_code_fences() {
        let response = r#"
MODERN:
```
fd -e rs
```

STANDARD:
find .
"#;
        let parsed = DualCommandList::parse(response);
        assert_eq!(parsed.modern.len(), 1);
        assert_eq!(parsed.standard.len(), 1);
    }

    #[test]
    fn test_dual_command_list_all_commands() {
        let mut list = DualCommandList::default();
        list.modern.push("fd -e rs".to_string());
        list.standard.push("find .".to_string());
        list.standard.push("ls".to_string());

        let all = list.all_commands();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0], "fd -e rs"); // modern first
        assert_eq!(all[1], "find .");
        assert_eq!(all[2], "ls");
    }

    #[test]
    fn test_dual_command_list_is_empty() {
        let empty = DualCommandList::default();
        assert!(empty.is_empty());

        let mut not_empty = DualCommandList::default();
        not_empty.standard.push("ls".to_string());
        assert!(!not_empty.is_empty());
    }

    #[test]
    fn test_dual_command_list_len() {
        let mut list = DualCommandList::default();
        assert_eq!(list.len(), 0);

        list.modern.push("fd".to_string());
        list.standard.push("find".to_string());
        list.standard.push("ls".to_string());
        assert_eq!(list.len(), 3);
    }

    // ToolCache tests

    #[test]
    fn test_tool_cache_new() {
        let cache = ToolCache::new();
        assert!(cache.available.is_empty());
        assert!(cache.unavailable.is_empty());
        assert_eq!(cache.version, ToolCache::CACHE_VERSION);
        assert!(!cache.is_dirty());
    }

    #[test]
    fn test_tool_cache_extract_binary_simple() {
        assert_eq!(ToolCache::extract_binary("ls -la"), Some("ls"));
        assert_eq!(ToolCache::extract_binary("find . -name '*.rs'"), Some("find"));
        assert_eq!(ToolCache::extract_binary("cat file.txt"), Some("cat"));
    }

    #[test]
    fn test_tool_cache_extract_binary_with_sudo() {
        assert_eq!(ToolCache::extract_binary("sudo ls -la"), Some("ls"));
        assert_eq!(ToolCache::extract_binary("sudo apt install pkg"), Some("apt"));
    }

    #[test]
    fn test_tool_cache_extract_binary_with_env() {
        assert_eq!(ToolCache::extract_binary("env VAR=value cmd arg"), Some("cmd"));
        assert_eq!(ToolCache::extract_binary("FOO=bar baz"), Some("baz"));
    }

    #[test]
    fn test_tool_cache_extract_binary_with_time() {
        assert_eq!(ToolCache::extract_binary("time ls -la"), Some("ls"));
        assert_eq!(ToolCache::extract_binary("nice -n 10 make"), Some("make"));
    }

    #[test]
    fn test_tool_cache_extract_binary_empty() {
        assert_eq!(ToolCache::extract_binary(""), None);
        assert_eq!(ToolCache::extract_binary("   "), None);
    }

    #[test]
    fn test_tool_cache_extract_binary_only_env_vars() {
        // Edge case: only env vars, no actual command
        assert_eq!(ToolCache::extract_binary("FOO=bar BAZ=qux"), None);
    }

    #[test]
    fn test_tool_cache_is_available_caches_result() {
        let mut cache = ToolCache::new();

        // Check for 'ls' which should exist
        let first_check = cache.is_available("ls");
        assert!(first_check);
        assert!(cache.available.contains("ls"));
        assert!(cache.is_dirty());

        // Second check should hit cache
        let second_check = cache.is_available("ls");
        assert!(second_check);
    }

    #[test]
    fn test_tool_cache_is_available_nonexistent() {
        let mut cache = ToolCache::new();

        // Check for something that definitely doesn't exist
        let exists = cache.is_available("this_binary_definitely_does_not_exist_12345");
        assert!(!exists);
        assert!(
            cache
                .unavailable
                .contains("this_binary_definitely_does_not_exist_12345")
        );
    }

    #[test]
    fn test_tool_cache_filter_commands() {
        let mut cache = ToolCache::new();

        let commands = vec![
            "ls -la".to_string(),
            "nonexistent_cmd_xyz123 arg".to_string(),
            "find . -name '*.rs'".to_string(),
        ];

        let (available, unavailable) = cache.filter_commands(&commands);

        assert!(available.contains(&"ls -la".to_string()));
        assert!(available.contains(&"find . -name '*.rs'".to_string()));
        assert!(unavailable.contains(&"nonexistent_cmd_xyz123 arg".to_string()));
    }

    #[test]
    fn test_tool_cache_process_response() {
        let mut cache = ToolCache::new();

        let mut response = DualCommandList::default();
        response.modern.push("nonexistent_modern_tool_xyz".to_string());
        response.standard.push("ls -la".to_string());
        response.standard.push("find .".to_string());

        let result = cache.process_response(&response);

        // Should only contain available commands
        assert!(result.contains(&"ls -la".to_string()));
        assert!(result.contains(&"find .".to_string()));
        assert!(!result.contains(&"nonexistent_modern_tool_xyz".to_string()));
    }

    #[test]
    fn test_tool_cache_process_response_fallback() {
        let mut cache = ToolCache::new();

        // Pre-populate cache to mark standard tools as unavailable (edge case)
        cache.unavailable.insert("ls".to_string());
        cache.unavailable.insert("find".to_string());

        let mut response = DualCommandList::default();
        response.standard.push("ls -la".to_string());
        response.standard.push("find .".to_string());

        let result = cache.process_response(&response);

        // Should fallback to unfiltered standard when all filtered out
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_tool_cache_available_tools_for_prompt_empty() {
        let cache = ToolCache::new();
        let prompt = cache.available_tools_for_prompt();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_tool_cache_available_tools_for_prompt_only_standard() {
        let mut cache = ToolCache::new();
        cache.available.insert("ls".to_string());
        cache.available.insert("cat".to_string());
        cache.available.insert("grep".to_string());

        // Standard tools should not appear in prompt
        let prompt = cache.available_tools_for_prompt();
        assert!(prompt.is_empty());
    }

    #[test]
    fn test_tool_cache_available_tools_for_prompt_with_modern() {
        let mut cache = ToolCache::new();
        cache.available.insert("ls".to_string()); // standard
        cache.available.insert("eza".to_string()); // modern
        cache.available.insert("rg".to_string()); // modern
        cache.available.insert("fd".to_string()); // modern

        let prompt = cache.available_tools_for_prompt();
        assert!(prompt.contains("eza"));
        assert!(prompt.contains("rg"));
        assert!(prompt.contains("fd"));
        // Check that "ls" doesn't appear as a tool name (the word "tools" contains "ls" substring)
        assert!(!prompt.contains(" ls,") && !prompt.contains(" ls\n") && !prompt.contains(": ls,"));
    }

    #[test]
    fn test_tool_cache_clear() {
        let mut cache = ToolCache::new();
        cache.available.insert("ls".to_string());
        cache.unavailable.insert("xyz".to_string());
        cache.dirty = false;

        cache.clear();

        assert!(cache.available.is_empty());
        assert!(cache.unavailable.is_empty());
        assert!(cache.is_dirty());
    }

    #[test]
    fn test_tool_cache_stats() {
        let mut cache = ToolCache::new();
        cache.available.insert("ls".to_string()); // standard
        cache.available.insert("cat".to_string()); // standard
        cache.available.insert("eza".to_string()); // modern
        cache.available.insert("rg".to_string()); // modern
        cache.unavailable.insert("nonexistent".to_string());

        let stats = cache.stats();
        assert_eq!(stats.available_count, 4);
        assert_eq!(stats.unavailable_count, 1);
        assert_eq!(stats.modern_tools_count, 2);
    }

    #[test]
    fn test_tool_cache_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("tools.json");

        // Create and save cache
        {
            let mut cache = ToolCache::new();
            cache.available.insert("eza".to_string());
            cache.available.insert("rg".to_string());
            cache.unavailable.insert("nonexistent".to_string());
            cache.dirty = true;
            cache.save_to(&cache_path).unwrap();
        }

        // Load cache
        {
            let cache = ToolCache::load_from(&cache_path);
            assert!(cache.available.contains("eza"));
            assert!(cache.available.contains("rg"));
            assert!(cache.unavailable.contains("nonexistent"));
            assert!(!cache.is_dirty());
        }
    }

    #[test]
    fn test_tool_cache_load_nonexistent() {
        let cache = ToolCache::load_from(&PathBuf::from("/nonexistent/path/cache.json"));
        assert!(cache.available.is_empty());
        assert_eq!(cache.version, ToolCache::CACHE_VERSION);
    }

    #[test]
    fn test_tool_cache_load_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("tools.json");
        fs::write(&cache_path, "not valid json").unwrap();

        let cache = ToolCache::load_from(&cache_path);
        assert!(cache.available.is_empty());
    }

    #[test]
    fn test_tool_cache_load_wrong_version() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("tools.json");
        fs::write(&cache_path, r#"{"available":["ls"],"unavailable":[],"version":999}"#).unwrap();

        let cache = ToolCache::load_from(&cache_path);
        // Should create new cache due to version mismatch
        assert!(cache.available.is_empty());
        assert_eq!(cache.version, ToolCache::CACHE_VERSION);
    }

    #[test]
    fn test_tool_cache_save_not_dirty() {
        let temp_dir = TempDir::new().unwrap();
        let cache_path = temp_dir.path().join("tools.json");

        let mut cache = ToolCache::new();
        cache.dirty = false;

        // Should not write anything since not dirty
        cache.save_to(&cache_path).unwrap();
        assert!(!cache_path.exists());
    }

    #[test]
    fn test_tool_cache_mark_dirty() {
        let mut cache = ToolCache::new();
        assert!(!cache.is_dirty());

        cache.mark_dirty();
        assert!(cache.is_dirty());
    }

    #[test]
    fn test_tool_cache_cache_path() {
        let path = ToolCache::cache_path();
        assert!(path.ends_with("qai/tools.json"));
    }

    #[test]
    fn test_dual_command_list_parse_inline_section_marker() {
        // Some models might output "MODERN: fd -e rs" on same line
        let response = "MODERN: ignore\nfd -e rs\nSTANDARD:\nls";
        let parsed = DualCommandList::parse(response);
        assert_eq!(parsed.modern.len(), 1);
        assert_eq!(parsed.standard.len(), 1);
    }

    #[test]
    fn test_filter_commands_empty() {
        let mut cache = ToolCache::new();
        let (available, unavailable) = cache.filter_commands(&[]);
        assert!(available.is_empty());
        assert!(unavailable.is_empty());
    }

    #[test]
    fn test_process_response_empty() {
        let mut cache = ToolCache::new();
        let response = DualCommandList::default();
        let result = cache.process_response(&response);
        assert!(result.is_empty());
    }

    #[test]
    fn test_available_tools_sorted() {
        let mut cache = ToolCache::new();
        cache.available.insert("zoxide".to_string());
        cache.available.insert("eza".to_string());
        cache.available.insert("bat".to_string());

        let prompt = cache.available_tools_for_prompt();
        // Should be sorted alphabetically
        let bat_pos = prompt.find("bat").unwrap();
        let eza_pos = prompt.find("eza").unwrap();
        let zoxide_pos = prompt.find("zoxide").unwrap();
        assert!(bat_pos < eza_pos);
        assert!(eza_pos < zoxide_pos);
    }
}
