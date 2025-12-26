//! Shell integration scripts
//!
//! This module provides shell init scripts that are printed to stdout when
//! `qai shell-init <shell>` is called. Users add `eval "$(qai shell-init zsh)"`
//! to their shell config.

use crate::bindings::key_name_to_sequence;
use crate::config::Config;

/// Generate ZSH init script with configurable trigger and submit keys
///
/// The trigger and submit keys are read from the config and converted to zsh bindkey sequences.
pub fn generate_zsh_init_script(config: &Config) -> Result<String, String> {
    let trigger_sequence = key_name_to_sequence(&config.bindings.trigger)?;
    let submit_sequence = key_name_to_sequence(&config.bindings.submit)?;

    Ok(format!(
        r#"
# qai - Natural language to shell commands via AI
# Add to your .zshrc: eval "$(qai shell-init zsh)"
# Trigger key: {trigger_name} ({trigger_seq})
# Submit key: {submit_name} ({submit_seq})

# State variable: are we in AI mode?
_qai_in_ai_mode=0
_qai_saved_prompt=""
_qai_ai_prompt="🤖 ai> "

# Store original binding for trigger key (parse the widget name from bindkey output)
# bindkey '{trigger_seq}' outputs: "{trigger_seq}" widget-name
# We extract the widget name using parameter expansion
_qai_original_trigger_widget=""
if (( ${{+widgets[expand-or-complete]}} )); then
    _qai_original_trigger_widget="expand-or-complete"
fi
# Try to get actual current binding
_qai_trigger_binding="$(bindkey '{trigger_seq}' 2>/dev/null)"
if [[ "$_qai_trigger_binding" == *'" '* ]]; then
    _qai_original_trigger_widget="${{_qai_trigger_binding##*\" }}"
fi
# Fallback to expand-or-complete if nothing found
[[ -z "$_qai_original_trigger_widget" ]] && _qai_original_trigger_widget="expand-or-complete"
unset _qai_trigger_binding

# Trigger key handler - dispatch based on buffer content and mode
_qai_trigger_handler() {{
    if [[ "$BUFFER" == "ai" && $_qai_in_ai_mode -eq 0 ]]; then
        _qai_start
    else
        # Normal completion/action for this key
        zle "${{_qai_original_trigger_widget:-expand-or-complete}}"
    fi
}}

# Start AI mode session
_qai_start() {{
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
}}

# Exit AI mode session
_qai_exit() {{
    if [[ $_qai_in_ai_mode -eq 1 ]]; then
        _qai_in_ai_mode=0
        PROMPT="$_qai_saved_prompt"
        BUFFER=""
        CURSOR=0
        zle reset-prompt
    fi
}}

# Submit query in AI mode
_qai_submit() {{
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
                    CURSOR=${{#BUFFER}}
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
                CURSOR=${{#BUFFER}}
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
}}

# TRAPINT handles Ctrl+C at signal level (the ONLY reliable way in zsh)
# This fires BEFORE any widget, so we can intercept cleanly
# NOTE: Cannot modify BUFFER here - it's read-only in signal trap context
TRAPINT() {{
    if [[ $_qai_in_ai_mode -eq 1 ]]; then
        _qai_in_ai_mode=0
        PROMPT="$_qai_saved_prompt"
        print ""  # newline
        zle && zle reset-prompt
        return 128  # indicate interrupt was handled, don't propagate
    fi
    # Not in AI mode - let default SIGINT behavior happen
    return $((128 + $1))
}}

# Register widgets
zle -N _qai_trigger_handler
zle -N _qai_start
zle -N _qai_exit
zle -N _qai_submit

# Bind keys
# Trigger: activates AI mode when buffer is "ai", otherwise falls through to original binding
bindkey '{trigger_seq}' _qai_trigger_handler
# Submit: submits query in AI mode, otherwise normal accept-line
bindkey '{submit_seq}' _qai_submit
# Ctrl+C is handled by TRAPINT above (signal level, not bindkey)
"#,
        trigger_name = config.bindings.trigger,
        trigger_seq = trigger_sequence,
        submit_name = config.bindings.submit,
        submit_seq = submit_sequence
    ))
}

/// Generate shell init script for the specified shell
///
/// # Arguments
/// * `shell` - Shell name (e.g., "zsh")
/// * `config` - Configuration containing bindings
///
/// # Returns
/// * `Some(Ok(script))` - Successfully generated script
/// * `Some(Err(msg))` - Invalid configuration (e.g., unknown key name)
/// * `None` - Unsupported shell
pub fn generate_init_script(shell: &str, config: &Config) -> Option<Result<String, String>> {
    match shell.to_lowercase().as_str() {
        "zsh" => Some(generate_zsh_init_script(config)),
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
    use crate::config::BindingsConfig;

    fn default_config() -> Config {
        Config::default()
    }

    fn config_with_trigger(trigger: &str) -> Config {
        Config {
            bindings: BindingsConfig {
                trigger: trigger.to_string(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn test_zsh_init_script_contains_ai_mode_state() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Must have AI mode state variable
        assert!(script.contains("_qai_in_ai_mode=0"));

        // Must have AI mode prompt
        assert!(script.contains("_qai_ai_prompt="));
        assert!(script.contains("🤖"));
    }

    #[test]
    fn test_zsh_init_script_trigger_handler() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Must have trigger handler function
        assert!(script.contains("_qai_trigger_handler()"));

        // Trigger handler checks for "ai" buffer
        assert!(script.contains(r#""$BUFFER" == "ai""#));

        // Calls _qai_start when buffer is "ai"
        assert!(script.contains("_qai_start"));
    }

    #[test]
    fn test_zsh_init_script_start_function() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

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
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Must have exit function
        assert!(script.contains("_qai_exit()"));

        // Restores original prompt
        assert!(script.contains(r#"PROMPT="$_qai_saved_prompt""#));

        // Clears AI mode flag
        assert!(script.contains("_qai_in_ai_mode=0"));
    }

    #[test]
    fn test_zsh_init_script_submit_function() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

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
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Core widgets registered
        assert!(script.contains("zle -N _qai_trigger_handler"));
        assert!(script.contains("zle -N _qai_start"));
        assert!(script.contains("zle -N _qai_exit"));
        assert!(script.contains("zle -N _qai_submit"));
    }

    #[test]
    fn test_zsh_init_script_default_tab_binding() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Default is Tab (^I)
        assert!(script.contains("bindkey '^I' _qai_trigger_handler"));
        assert!(script.contains("bindkey '^M' _qai_submit")); // Enter
    }

    #[test]
    fn test_zsh_init_script_custom_ctrl_space_binding() {
        let config = config_with_trigger("ctrl-space");
        let script = generate_zsh_init_script(&config).unwrap();

        // Should use ^@ for ctrl-space
        assert!(script.contains("bindkey '^@' _qai_trigger_handler"));
        // Enter should still be ^M
        assert!(script.contains("bindkey '^M' _qai_submit"));
    }

    #[test]
    fn test_zsh_init_script_custom_f1_binding() {
        let config = config_with_trigger("f1");
        let script = generate_zsh_init_script(&config).unwrap();

        // Should use F1 escape sequence
        assert!(script.contains("bindkey '^[OP' _qai_trigger_handler"));
    }

    #[test]
    fn test_zsh_init_script_custom_ctrl_g_binding() {
        let config = config_with_trigger("ctrl-g");
        let script = generate_zsh_init_script(&config).unwrap();

        // Should use ^G for ctrl-g
        assert!(script.contains("bindkey '^G' _qai_trigger_handler"));
    }

    #[test]
    fn test_zsh_init_script_invalid_key_returns_error() {
        let config = config_with_trigger("invalid-key");
        let result = generate_zsh_init_script(&config);

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown key 'invalid-key'"));
        assert!(err.contains("Valid keys:"));
    }

    #[test]
    fn test_zsh_init_script_trapint_handler() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // TRAPINT handles Ctrl+C at signal level (only reliable way)
        assert!(script.contains("TRAPINT()"));
        assert!(script.contains("_qai_in_ai_mode -eq 1"));
        // Returns 128 to indicate handled
        assert!(script.contains("return 128"));
    }

    #[test]
    fn test_zsh_init_script_fallback_to_original_trigger() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // CRITICAL: Must fallback to original widget when NOT triggering AI mode
        assert!(
            script.contains(r#"zle "${_qai_original_trigger_widget:-expand-or-complete}""#),
            "Script must call original trigger widget with fallback for normal completion"
        );

        // Must have a fallback to expand-or-complete
        assert!(
            script.contains(r#"_qai_original_trigger_widget="expand-or-complete""#),
            "Script must have expand-or-complete as default fallback"
        );
    }

    #[test]
    fn test_zsh_init_script_api_validation_error_handling() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Shows error message on validation failure
        assert!(script.contains(r#"zle -M "❌ $validation_result""#));

        // Clears buffer on failure
        assert!(script.contains(r#"BUFFER="""#));
    }

    #[test]
    fn test_zsh_init_script_shows_fetching_indicator() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // Shows fetching indicator when querying
        assert!(script.contains("🔄 Fetching"));
    }

    #[test]
    fn test_zsh_init_script_fzf_options() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // fzf should have height and reverse for dropdown style
        assert!(script.contains("--height"));
        assert!(script.contains("--reverse"));
    }

    #[test]
    fn test_generate_init_script_zsh() {
        let result = generate_init_script("zsh", &default_config());
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_generate_init_script_zsh_uppercase() {
        let result = generate_init_script("ZSH", &default_config());
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_generate_init_script_zsh_mixed_case() {
        let result = generate_init_script("Zsh", &default_config());
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
    }

    #[test]
    fn test_generate_init_script_unsupported() {
        assert!(generate_init_script("bash", &default_config()).is_none());
        assert!(generate_init_script("fish", &default_config()).is_none());
        assert!(generate_init_script("", &default_config()).is_none());
        assert!(generate_init_script("invalid", &default_config()).is_none());
    }

    #[test]
    fn test_generate_init_script_with_invalid_key() {
        let config = config_with_trigger("not-a-key");
        let result = generate_init_script("zsh", &config);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
    }

    #[test]
    fn test_supported_shells() {
        let shells = supported_shells();
        assert!(!shells.is_empty());
        assert!(shells.contains(&"zsh"));
    }

    #[test]
    fn test_zsh_init_script_normal_enter_outside_ai_mode() {
        let script = generate_zsh_init_script(&default_config()).unwrap();

        // When not in AI mode, Enter should do normal accept-line
        assert!(script.contains("zle accept-line"));
    }

    #[test]
    fn test_zsh_init_script_shows_trigger_key_in_comment() {
        let config = config_with_trigger("ctrl-space");
        let script = generate_zsh_init_script(&config).unwrap();

        // Script should document the configured trigger key
        assert!(script.contains("Trigger key: ctrl-space"));
    }

    #[test]
    fn test_zsh_init_script_case_insensitive_key() {
        // Test that "TAB" works same as "tab"
        let config = config_with_trigger("TAB");
        let script = generate_zsh_init_script(&config).unwrap();
        assert!(script.contains("bindkey '^I' _qai_trigger_handler"));
    }
}
