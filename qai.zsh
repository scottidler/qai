#!/usr/bin/env zsh
# qai.zsh - Natural language to shell commands (pure zsh + jq implementation)
# Usage: qai <natural language query>
#        qai query <natural language query>
#        qai shell-init zsh

setopt ERR_EXIT NO_UNSET PIPE_FAIL

# Configuration
QAI_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/qai"
QAI_CONFIG_FILE="${QAI_CONFIG_FILE:-$QAI_CONFIG_DIR/qai.yml}"
QAI_MODEL="${QAI_MODEL:-gpt-4o-mini}"
QAI_API_URL="${QAI_API_URL:-https://api.openai.com/v1/chat/completions}"

# System prompt for the AI
QAI_SYSTEM_PROMPT='You are a shell command generator. Convert natural language to shell commands.

RULES:
1. Output ONLY the command - no explanations, no markdown, no backticks
2. Use common Unix tools (grep, find, awk, sed, etc.)
3. Prefer simple, readable commands over complex one-liners
4. Use safe options when available (e.g., -i for interactive rm)
5. If the request is ambiguous, make reasonable assumptions

CONTEXT:
- Shell: zsh
- OS: Linux/macOS

Examples:
- "list files by size" → ls -lhS
- "find large files over 100mb" → find . -type f -size +100M
- "count lines in all python files" → find . -name "*.py" -exec wc -l {} + | tail -1
- "show disk usage" → df -h
- "kill process on port 3000" → lsof -ti:3000 | xargs kill -9'

# Print usage
usage() {
    cat <<'EOF'
qai - Natural language to shell commands

Usage:
    qai <query>              Convert natural language to shell command
    qai query <query>        Same as above (explicit)
    qai shell-init zsh       Output shell integration script

Environment:
    OPENAI_API_KEY          Required. Your OpenAI API key
    QAI_MODEL               Model to use (default: gpt-4o-mini)
    QAI_CONFIG_FILE         Path to config file

Examples:
    qai list files by size
    qai find all python files modified today
    qai show memory usage

Shell Integration:
    Add to ~/.zshrc:
        source /path/to/qai-init.zsh
    
    Then type: qai <query><Tab> to expand
EOF
}

# Load API key from environment or config file
load_api_key() {
    # First check environment
    if [[ -n "${OPENAI_API_KEY:-}" ]]; then
        echo "$OPENAI_API_KEY"
        return 0
    fi
    
    # Try to load from config file
    if [[ -f "$QAI_CONFIG_FILE" ]]; then
        local key
        # Parse YAML with simple grep/sed (works for simple key: value format)
        key=$(grep -E '^[[:space:]]*api_key:' "$QAI_CONFIG_FILE" 2>/dev/null | sed 's/.*api_key:[[:space:]]*//' | tr -d '"'"'" | head -1)
        if [[ -n "$key" ]]; then
            echo "$key"
            return 0
        fi
    fi
    
    return 1
}

# Make API request to OpenAI
query_api() {
    local query="$1"
    local api_key
    
    api_key=$(load_api_key) || {
        echo "Error: OPENAI_API_KEY not set and not found in config" >&2
        return 1
    }
    
    # Build JSON payload using jq
    local payload
    payload=$(jq -n \
        --arg model "$QAI_MODEL" \
        --arg system "$QAI_SYSTEM_PROMPT" \
        --arg user "$query" \
        '{
            model: $model,
            messages: [
                {role: "system", content: $system},
                {role: "user", content: $user}
            ],
            temperature: 0.1,
            max_tokens: 500
        }')
    
    # Make request
    local response
    response=$(curl -sS --fail-with-body \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer $api_key" \
        -d "$payload" \
        "$QAI_API_URL" 2>&1) || {
        echo "Error: API request failed" >&2
        echo "$response" >&2
        return 1
    }
    
    # Extract the command from the response
    local command
    command=$(echo "$response" | jq -r '.choices[0].message.content // empty' 2>/dev/null)
    
    if [[ -z "$command" ]]; then
        echo "Error: No response from API" >&2
        echo "Raw response: $response" >&2
        return 1
    fi
    
    # Clean up the command (remove leading/trailing whitespace, backticks if present)
    command=$(echo "$command" | sed 's/^[`]*//; s/[`]*$//; s/^[[:space:]]*//; s/[[:space:]]*$//')
    
    echo "$command"
}

# Output shell init script
shell_init() {
    local shell_type="${1:-}"
    
    if [[ "$shell_type" != "zsh" ]]; then
        echo "Error: Only 'zsh' is supported" >&2
        echo "Usage: qai shell-init zsh" >&2
        return 1
    fi
    
    # Output the init script (will be sourced by user's shell)
    cat <<'INIT_SCRIPT'
# qai - Natural language to shell commands
# Source this file in your .zshrc or run: eval "$(qai shell-init zsh)"

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
        zle "${_qai_original_tab_widget:-expand-or-complete}"
    fi
}

# Register the widget
zle -N _qai_expand

# Bind to Tab
bindkey '^I' _qai_expand
INIT_SCRIPT
}

# Main entry point
main() {
    local cmd="${1:-}"
    
    case "$cmd" in
        -h|--help|help)
            usage
            return 0
            ;;
        shell-init)
            shift
            shell_init "$@"
            return $?
            ;;
        query)
            shift
            if [[ $# -eq 0 ]]; then
                echo "Error: No query provided" >&2
                usage >&2
                return 1
            fi
            query_api "$*"
            return $?
            ;;
        "")
            echo "Error: No command or query provided" >&2
            usage >&2
            return 1
            ;;
        *)
            # Treat everything as a query
            query_api "$*"
            return $?
            ;;
    esac
}

# Run if executed directly (not sourced)
if [[ "${ZSH_EVAL_CONTEXT:-}" != *:file:* ]]; then
    main "$@"
fi

