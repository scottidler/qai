//! Key name to terminal escape sequence mapping
//!
//! This module provides a mapping from human-readable key names to zsh bindkey
//! escape sequences. Users can specify keys like "tab", "ctrl-space", or "f1"
//! in their config file without needing to know the escape sequences.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Mapping from friendly key names to zsh bindkey sequences
static KEY_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Tab and Enter
    m.insert("tab", "^I");
    m.insert("enter", "^M");
    m.insert("return", "^M");

    // Escape
    m.insert("escape", "^[");
    m.insert("esc", "^[");

    // Backspace
    m.insert("backspace", "^?");

    // Ctrl + letter (a-z)
    m.insert("ctrl-a", "^A");
    m.insert("ctrl-b", "^B");
    m.insert("ctrl-c", "^C");
    m.insert("ctrl-d", "^D");
    m.insert("ctrl-e", "^E");
    m.insert("ctrl-f", "^F");
    m.insert("ctrl-g", "^G");
    m.insert("ctrl-h", "^H");
    m.insert("ctrl-i", "^I"); // Same as Tab
    m.insert("ctrl-j", "^J");
    m.insert("ctrl-k", "^K");
    m.insert("ctrl-l", "^L");
    m.insert("ctrl-m", "^M"); // Same as Enter
    m.insert("ctrl-n", "^N");
    m.insert("ctrl-o", "^O");
    m.insert("ctrl-p", "^P");
    m.insert("ctrl-q", "^Q");
    m.insert("ctrl-r", "^R");
    m.insert("ctrl-s", "^S");
    m.insert("ctrl-t", "^T");
    m.insert("ctrl-u", "^U");
    m.insert("ctrl-v", "^V");
    m.insert("ctrl-w", "^W");
    m.insert("ctrl-x", "^X");
    m.insert("ctrl-y", "^Y");
    m.insert("ctrl-z", "^Z");

    // Ctrl + special
    m.insert("ctrl-space", "^@");
    m.insert("ctrl-backslash", "^\\");
    m.insert("ctrl-]", "^]");
    m.insert("ctrl-^", "^^");
    m.insert("ctrl-_", "^_");

    // Function keys (xterm sequences)
    m.insert("f1", "^[OP");
    m.insert("f2", "^[OQ");
    m.insert("f3", "^[OR");
    m.insert("f4", "^[OS");
    m.insert("f5", "^[[15~");
    m.insert("f6", "^[[17~");
    m.insert("f7", "^[[18~");
    m.insert("f8", "^[[19~");
    m.insert("f9", "^[[20~");
    m.insert("f10", "^[[21~");
    m.insert("f11", "^[[23~");
    m.insert("f12", "^[[24~");

    // Arrow keys
    m.insert("up", "^[[A");
    m.insert("down", "^[[B");
    m.insert("right", "^[[C");
    m.insert("left", "^[[D");

    // Navigation
    m.insert("home", "^[[H");
    m.insert("end", "^[[F");
    m.insert("insert", "^[[2~");
    m.insert("delete", "^[[3~");
    m.insert("page-up", "^[[5~");
    m.insert("pageup", "^[[5~");
    m.insert("page-down", "^[[6~");
    m.insert("pagedown", "^[[6~");

    m
});

/// Convert a friendly key name to a zsh bindkey sequence
///
/// # Arguments
/// * `name` - Key name like "tab", "ctrl-space", "f1"
///
/// # Returns
/// * `Ok(sequence)` - The zsh bindkey sequence (e.g., "^I")
/// * `Err(message)` - Error with list of valid keys
///
/// # Examples
/// ```
/// use qai::keys::key_name_to_sequence;
///
/// assert_eq!(key_name_to_sequence("tab").unwrap(), "^I");
/// assert_eq!(key_name_to_sequence("ctrl-space").unwrap(), "^@");
/// assert_eq!(key_name_to_sequence("Tab").unwrap(), "^I"); // case insensitive
/// ```
pub fn key_name_to_sequence(name: &str) -> Result<&'static str, String> {
    let normalized = name.to_lowercase().replace(' ', "-");

    KEY_MAP.get(normalized.as_str()).copied().ok_or_else(|| {
        let mut valid_keys: Vec<_> = KEY_MAP.keys().copied().collect();
        valid_keys.sort();
        format!("Unknown key '{}'. Valid keys: {}", name, valid_keys.join(", "))
    })
}

/// Get all valid key names (for documentation/help)
#[allow(dead_code)]
pub fn valid_key_names() -> Vec<&'static str> {
    let mut keys: Vec<_> = KEY_MAP.keys().copied().collect();
    keys.sort();
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_key() {
        assert_eq!(key_name_to_sequence("tab").unwrap(), "^I");
    }

    #[test]
    fn test_enter_key() {
        assert_eq!(key_name_to_sequence("enter").unwrap(), "^M");
        assert_eq!(key_name_to_sequence("return").unwrap(), "^M");
    }

    #[test]
    fn test_ctrl_space() {
        assert_eq!(key_name_to_sequence("ctrl-space").unwrap(), "^@");
    }

    #[test]
    fn test_ctrl_letters() {
        assert_eq!(key_name_to_sequence("ctrl-a").unwrap(), "^A");
        assert_eq!(key_name_to_sequence("ctrl-g").unwrap(), "^G");
        assert_eq!(key_name_to_sequence("ctrl-z").unwrap(), "^Z");
    }

    #[test]
    fn test_function_keys() {
        assert_eq!(key_name_to_sequence("f1").unwrap(), "^[OP");
        assert_eq!(key_name_to_sequence("f5").unwrap(), "^[[15~");
        assert_eq!(key_name_to_sequence("f12").unwrap(), "^[[24~");
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(key_name_to_sequence("up").unwrap(), "^[[A");
        assert_eq!(key_name_to_sequence("down").unwrap(), "^[[B");
        assert_eq!(key_name_to_sequence("left").unwrap(), "^[[D");
        assert_eq!(key_name_to_sequence("right").unwrap(), "^[[C");
    }

    #[test]
    fn test_navigation_keys() {
        assert_eq!(key_name_to_sequence("home").unwrap(), "^[[H");
        assert_eq!(key_name_to_sequence("end").unwrap(), "^[[F");
        assert_eq!(key_name_to_sequence("delete").unwrap(), "^[[3~");
        assert_eq!(key_name_to_sequence("page-up").unwrap(), "^[[5~");
        assert_eq!(key_name_to_sequence("pageup").unwrap(), "^[[5~");
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(key_name_to_sequence("Tab").unwrap(), "^I");
        assert_eq!(key_name_to_sequence("TAB").unwrap(), "^I");
        assert_eq!(key_name_to_sequence("CTRL-SPACE").unwrap(), "^@");
        assert_eq!(key_name_to_sequence("Ctrl-A").unwrap(), "^A");
    }

    #[test]
    fn test_space_to_dash_normalization() {
        assert_eq!(key_name_to_sequence("ctrl space").unwrap(), "^@");
        assert_eq!(key_name_to_sequence("page up").unwrap(), "^[[5~");
    }

    #[test]
    fn test_invalid_key() {
        let result = key_name_to_sequence("invalid-key");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown key 'invalid-key'"));
        assert!(err.contains("Valid keys:"));
    }

    #[test]
    fn test_valid_key_names_not_empty() {
        let names = valid_key_names();
        assert!(!names.is_empty());
        assert!(names.contains(&"tab"));
        assert!(names.contains(&"ctrl-space"));
        assert!(names.contains(&"f1"));
    }

    #[test]
    fn test_valid_key_names_sorted() {
        let names = valid_key_names();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn test_escape_key() {
        assert_eq!(key_name_to_sequence("escape").unwrap(), "^[");
        assert_eq!(key_name_to_sequence("esc").unwrap(), "^[");
    }

    #[test]
    fn test_backspace() {
        assert_eq!(key_name_to_sequence("backspace").unwrap(), "^?");
    }

    #[test]
    fn test_ctrl_special_chars() {
        assert_eq!(key_name_to_sequence("ctrl-backslash").unwrap(), "^\\");
        assert_eq!(key_name_to_sequence("ctrl-]").unwrap(), "^]");
        assert_eq!(key_name_to_sequence("ctrl-^").unwrap(), "^^");
        assert_eq!(key_name_to_sequence("ctrl-_").unwrap(), "^_");
    }
}
