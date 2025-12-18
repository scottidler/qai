/// ZSH integration script
///
/// This is printed to stdout when `qai shell-init zsh` is called.
/// Users add `eval "$(qai shell-init zsh)"` to their .zshrc
pub const ZSH_INIT_SCRIPT: &str = r#"
# qai - Natural language to shell commands via AI
# Add to your .zshrc: eval "$(qai shell-init zsh)"

# State variable: are we in AI mode?
_qai_in_ai_mode=0
_qai_saved_prompt=""
_qai_ai_prompt="🤖 ai> "

# Store original tab binding (parse the widget name from bindkey output)
# bindkey '^I' outputs: "^I" widget-name
# We extract the widget name using parameter expansion
_qai_original_tab_widget=""
if (( ${+widgets[expand-or-complete]} )); then
    _qai_original_tab_widget="expand-or-complete"
fi
# Try to get actual current binding
_qai_tab_binding="$(bindkey '^I' 2>/dev/null)"
if [[ "$_qai_tab_binding" == *'" '* ]]; then
    _qai_original_tab_widget="${_qai_tab_binding##*\" }"
fi
# Fallback to expand-or-complete if nothing found
[[ -z "$_qai_original_tab_widget" ]] && _qai_original_tab_widget="expand-or-complete"
unset _qai_tab_binding

# Tab key handler - dispatch based on buffer content and mode
_qai_tab_handler() {
    if [[ "$BUFFER" == "ai" && $_qai_in_ai_mode -eq 0 ]]; then
        _qai_start
    else
        # Normal tab completion
        zle "${_qai_original_tab_widget:-expand-or-complete}"
    fi
}

# Start AI mode session
_qai_start() {
    # Validate API key first (calls OpenAI /v1/models, no token usage)
    local validation_result
    validation_result=$(qai validate-api 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        zle -M "❌ $validation_result"
        BUFFER=""
        return 1
    fi

    # Enter AI mode
    _qai_in_ai_mode=1
    _qai_saved_prompt="$PROMPT"
    PROMPT="$_qai_ai_prompt"
    BUFFER=""
    CURSOR=0
    zle reset-prompt
}

# Exit AI mode session
_qai_exit() {
    if [[ $_qai_in_ai_mode -eq 1 ]]; then
        _qai_in_ai_mode=0
        PROMPT="$_qai_saved_prompt"
        BUFFER=""
        CURSOR=0
        zle reset-prompt
    fi
}

# Submit query in AI mode
_qai_submit() {
    if [[ $_qai_in_ai_mode -eq 1 ]]; then
        local query="$BUFFER"

        if [[ -z "$query" ]]; then
            # Empty query, exit AI mode
            _qai_exit
            return
        fi

        # Show fetching indicator
        zle -M "🔄 Fetching..."

        local result
        local exit_code

        # Check if fzf is available
        if command -v fzf >/dev/null 2>&1; then
            # Get multiple results
            result=$(qai query --multi "$query" 2>/dev/null)
            exit_code=$?

            if [[ $exit_code -eq 0 && -n "$result" ]]; then
                # Use fzf to select
                local selected
                selected=$(echo "$result" | fzf --height=10 --reverse --prompt="Select command: ")

                if [[ -n "$selected" ]]; then
                    _qai_in_ai_mode=0
                    PROMPT="$_qai_saved_prompt"
                    BUFFER="$selected"
                    CURSOR=${#BUFFER}
                    zle reset-prompt
                    zle -M ""
                else
                    # User cancelled fzf
                    zle -M "Cancelled"
                fi
            else
                zle -M "❌ No results"
            fi
        else
            # No fzf, single result mode
            result=$(qai query "$query" 2>/dev/null)
            exit_code=$?

            if [[ $exit_code -eq 0 && -n "$result" ]]; then
                _qai_in_ai_mode=0
                PROMPT="$_qai_saved_prompt"
                BUFFER="$result"
                CURSOR=${#BUFFER}
                zle reset-prompt
                zle -M ""
            else
                zle -M "❌ No results"
            fi
        fi
    else
        # Not in AI mode, normal enter (accept-line)
        zle accept-line
    fi
}

# TRAPINT handles Ctrl+C at signal level (the ONLY reliable way in zsh)
# This fires BEFORE any widget, so we can intercept cleanly
TRAPINT() {
    if [[ $_qai_in_ai_mode -eq 1 ]]; then
        _qai_in_ai_mode=0
        PROMPT="$_qai_saved_prompt"
        BUFFER=""
        print ""  # newline
        zle && zle reset-prompt
        return 128  # indicate interrupt was handled, don't propagate
    fi
    # Not in AI mode - let default SIGINT behavior happen
    return $((128 + $1))
}

# Register widgets
zle -N _qai_tab_handler
zle -N _qai_start
zle -N _qai_exit
zle -N _qai_submit

# Bind keys - ONLY Tab and Enter need custom handling
# Tab: triggers AI mode when buffer is "ai", otherwise normal completion
# Enter: submits query in AI mode, otherwise normal accept-line
bindkey '^I' _qai_tab_handler  # Tab
bindkey '^M' _qai_submit       # Enter
# Ctrl+C is handled by TRAPINT above (signal level, not bindkey)
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
    fn test_zsh_init_script_contains_ai_mode_state() {
        let script = ZSH_INIT_SCRIPT;

        // Must have AI mode state variable
        assert!(script.contains("_qai_in_ai_mode=0"));

        // Must have AI mode prompt
        assert!(script.contains("_qai_ai_prompt="));
        assert!(script.contains("🤖"));
    }

    #[test]
    fn test_zsh_init_script_tab_handler() {
        let script = ZSH_INIT_SCRIPT;

        // Must have tab handler function
        assert!(script.contains("_qai_tab_handler()"));

        // Tab handler checks for "ai" buffer
        assert!(script.contains(r#""$BUFFER" == "ai""#));

        // Calls _qai_start when buffer is "ai"
        assert!(script.contains("_qai_start"));
    }

    #[test]
    fn test_zsh_init_script_start_function() {
        let script = ZSH_INIT_SCRIPT;

        // Must have start function
        assert!(script.contains("_qai_start()"));

        // Validates API key before entering mode
        assert!(script.contains("qai validate-api"));

        // Sets AI mode flag
        assert!(script.contains("_qai_in_ai_mode=1"));

        // Saves and sets prompt
        assert!(script.contains("_qai_saved_prompt"));
        assert!(script.contains(r#"PROMPT="$_qai_ai_prompt""#));
    }

    #[test]
    fn test_zsh_init_script_exit_function() {
        let script = ZSH_INIT_SCRIPT;

        // Must have exit function
        assert!(script.contains("_qai_exit()"));

        // Restores original prompt
        assert!(script.contains(r#"PROMPT="$_qai_saved_prompt""#));

        // Clears AI mode flag
        assert!(script.contains("_qai_in_ai_mode=0"));
    }

    #[test]
    fn test_zsh_init_script_submit_function() {
        let script = ZSH_INIT_SCRIPT;

        // Must have submit function
        assert!(script.contains("_qai_submit()"));

        // Checks if in AI mode
        assert!(script.contains(r#"$_qai_in_ai_mode -eq 1"#));

        // Has fzf integration
        assert!(script.contains("command -v fzf"));
        assert!(script.contains("qai query --multi"));
        assert!(script.contains("| fzf"));

        // Has fallback for no fzf
        assert!(script.contains("qai query \"$query\""));
    }

    #[test]
    fn test_zsh_init_script_widget_registration() {
        let script = ZSH_INIT_SCRIPT;

        // Core widgets registered
        assert!(script.contains("zle -N _qai_tab_handler"));
        assert!(script.contains("zle -N _qai_start"));
        assert!(script.contains("zle -N _qai_exit"));
        assert!(script.contains("zle -N _qai_submit"));
    }

    #[test]
    fn test_zsh_init_script_key_bindings() {
        let script = ZSH_INIT_SCRIPT;

        // Only Tab and Enter are bound - minimal footprint
        assert!(script.contains("bindkey '^I' _qai_tab_handler")); // Tab
        assert!(script.contains("bindkey '^M' _qai_submit")); // Enter
        // Ctrl+C handled by TRAPINT, not bindkey
    }

    #[test]
    fn test_zsh_init_script_trapint_handler() {
        let script = ZSH_INIT_SCRIPT;

        // TRAPINT handles Ctrl+C at signal level (only reliable way)
        assert!(script.contains("TRAPINT()"));
        assert!(script.contains("_qai_in_ai_mode -eq 1"));
        // Returns 128 to indicate handled
        assert!(script.contains("return 128"));
    }

    #[test]
    fn test_zsh_init_script_fallback_to_original_tab() {
        let script = ZSH_INIT_SCRIPT;

        // CRITICAL: Must fallback to original tab widget when NOT triggering AI mode
        assert!(
            script.contains(r#"zle "${_qai_original_tab_widget:-expand-or-complete}""#),
            "Script must call original tab widget with fallback for normal completion"
        );

        // Must properly parse bindkey output to get the widget name
        assert!(
            script.contains(r#"_qai_tab_binding="$(bindkey '^I' 2>/dev/null)""#),
            "Script must capture current tab binding"
        );

        // Must extract widget name from bindkey output
        assert!(
            script.contains(r#"${_qai_tab_binding##*\" }"#),
            "Script must extract widget name from bindkey output"
        );

        // Must have a fallback to expand-or-complete
        assert!(
            script.contains(r#"_qai_original_tab_widget="expand-or-complete""#),
            "Script must have expand-or-complete as default fallback"
        );
    }

    #[test]
    fn test_zsh_init_script_api_validation_error_handling() {
        let script = ZSH_INIT_SCRIPT;

        // Shows error message on validation failure
        assert!(script.contains(r#"zle -M "❌ $validation_result""#));

        // Clears buffer on failure
        assert!(script.contains(r#"BUFFER="""#));
    }

    #[test]
    fn test_zsh_init_script_shows_fetching_indicator() {
        let script = ZSH_INIT_SCRIPT;

        // Shows fetching indicator when querying
        assert!(script.contains("🔄 Fetching"));
    }

    #[test]
    fn test_zsh_init_script_fzf_options() {
        let script = ZSH_INIT_SCRIPT;

        // fzf should have height and reverse for dropdown style
        assert!(script.contains("--height"));
        assert!(script.contains("--reverse"));
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

    #[test]
    fn test_zsh_init_script_normal_enter_outside_ai_mode() {
        let script = ZSH_INIT_SCRIPT;

        // When not in AI mode, Enter should do normal accept-line
        assert!(script.contains("zle accept-line"));
    }
}
