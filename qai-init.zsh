# qai-init.zsh - ZLE integration for qai
# Source this file in your .zshrc: source /path/to/qai-init.zsh
#
# This provides Tab expansion for "qai <query>" commands.
# Normal tab completion works for everything else.

# Ensure qai.zsh is available
if ! command -v qai &>/dev/null && [[ ! -f "${0:A:h}/qai.zsh" ]]; then
    echo "Warning: qai not found in PATH or alongside qai-init.zsh" >&2
fi

# Helper function to call qai (uses qai.zsh if qai not in PATH)
_qai_cmd() {
    if command -v qai &>/dev/null; then
        qai "$@"
    elif [[ -f "${_QAI_SCRIPT_DIR:-}/qai.zsh" ]]; then
        zsh "${_QAI_SCRIPT_DIR}/qai.zsh" "$@"
    else
        echo "Error: qai not found" >&2
        return 1
    fi
}

# Store script directory for finding qai.zsh
_QAI_SCRIPT_DIR="${0:A:h}"

# Store original tab binding (parse the widget name from bindkey output)
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

# QAI expansion function
_qai_expand() {
    # Only trigger if:
    # 1. Line starts with "qai "
    # 2. There is text beyond just "qai "
    if [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]; then
        local query="${BUFFER#qai }"

        # Call qai and capture output
        local result
        result=$(_qai_cmd query "$query" 2>/dev/null)
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
        zle "${_qai_original_tab_widget:-expand-or-complete}"
    fi
}

# Register the widget
zle -N _qai_expand

# Bind to Tab
bindkey '^I' _qai_expand

# Also create a simple 'qai' function if not already available
if ! command -v qai &>/dev/null; then
    qai() {
        zsh "${_QAI_SCRIPT_DIR}/qai.zsh" "$@"
    }
fi

