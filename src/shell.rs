/// ZSH integration script
///
/// This is printed to stdout when `qai shell-init zsh` is called.
/// Users add `eval "$(qai shell-init zsh)"` to their .zshrc
pub const ZSH_INIT_SCRIPT: &str = r#"
# qai - Natural language to shell commands
# Add to your .zshrc: eval "$(qai shell-init zsh)"

# Store original tab binding
_qai_original_tab_widget="${$(bindkey '^I')[2]:-expand-or-complete}"

# QAI expansion function
_qai_expand() {
    # Only trigger if:
    # 1. Line starts with "qai "
    # 2. There is text beyond just "qai "
    if [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]; then
        local query="${BUFFER#qai }"

        # Call qai and capture output
        local result
        result=$(qai query "$query" 2>/dev/null)
        local exit_code=$?

        if [[ $exit_code -eq 0 && -n "$result" ]]; then
            # Replace buffer with result
            BUFFER="$result"
            # Move cursor to end
            CURSOR=${#BUFFER}
        fi
        # On error, leave buffer unchanged (error printed to stderr)
    else
        # Not a qai command, use original tab behavior
        zle "${_qai_original_tab_widget}"
    fi
}

# Register the widget
zle -N _qai_expand

# Bind to Tab
bindkey '^I' _qai_expand
"#;

/// Generate shell init script for the specified shell
pub fn generate_init_script(shell: &str) -> Option<&'static str> {
    match shell.to_lowercase().as_str() {
        "zsh" => Some(ZSH_INIT_SCRIPT),
        // Future: add bash, fish support
        _ => None,
    }
}

/// List supported shells
pub fn supported_shells() -> &'static [&'static str] {
    &["zsh"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zsh_init_script_contains_required_elements() {
        let script = ZSH_INIT_SCRIPT;

        // Should contain the widget function
        assert!(script.contains("_qai_expand()"));

        // Should store original tab binding
        assert!(script.contains("_qai_original_tab_widget"));

        // Should register the widget
        assert!(script.contains("zle -N _qai_expand"));

        // Should bind to Tab
        assert!(script.contains("bindkey '^I' _qai_expand"));

        // Should check for qai prefix
        assert!(script.contains(r#""$BUFFER" == qai\ *"#));

        // Should call qai query
        assert!(script.contains("qai query"));
    }

    #[test]
    fn test_generate_init_script_zsh() {
        let script = generate_init_script("zsh");
        assert!(script.is_some());
        assert_eq!(script.unwrap(), ZSH_INIT_SCRIPT);
    }

    #[test]
    fn test_generate_init_script_zsh_uppercase() {
        let script = generate_init_script("ZSH");
        assert!(script.is_some());
    }

    #[test]
    fn test_generate_init_script_zsh_mixed_case() {
        let script = generate_init_script("Zsh");
        assert!(script.is_some());
    }

    #[test]
    fn test_generate_init_script_unsupported() {
        assert!(generate_init_script("bash").is_none());
        assert!(generate_init_script("fish").is_none());
        assert!(generate_init_script("").is_none());
        assert!(generate_init_script("invalid").is_none());
    }

    #[test]
    fn test_supported_shells() {
        let shells = supported_shells();
        assert!(!shells.is_empty());
        assert!(shells.contains(&"zsh"));
    }
}
