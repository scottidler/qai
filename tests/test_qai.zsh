#!/usr/bin/env zsh
# test_qai.zsh - Test suite for qai.zsh
# Run: zsh tests/test_qai.zsh

# Don't use set -e as it breaks test harnesses
setopt NO_UNSET PIPE_FAIL

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Get script directory
SCRIPT_DIR="${0:A:h}"
QAI_SCRIPT="${SCRIPT_DIR}/../qai.zsh"

# Test helper functions
pass() {
    ((TESTS_PASSED++))
    echo "${GREEN}✓ PASS${NC}: $1"
}

fail() {
    ((TESTS_FAILED++))
    echo "${RED}✗ FAIL${NC}: $1"
    [[ -n "${2:-}" ]] && echo "  ${YELLOW}Details${NC}: $2"
}

run_test() {
    local name="$1"
    shift
    ((TESTS_RUN++))
    local result=0
    "$@" || result=$?
    if [[ $result -eq 0 ]]; then
        pass "$name"
    else
        fail "$name" "Test function returned non-zero ($result)"
    fi
}

assert_eq() {
    local expected="$1"
    local actual="$2"
    local msg="${3:-values should be equal}"
    if [[ "$expected" == "$actual" ]]; then
        return 0
    else
        echo ""
        echo "  Expected: '$expected'"
        echo "  Actual:   '$actual'"
        return 1
    fi
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local msg="${3:-should contain substring}"
    if [[ "$haystack" == *"$needle"* ]]; then
        return 0
    else
        echo ""
        echo "  String: '$haystack'"
        echo "  Should contain: '$needle'"
        return 1
    fi
}

assert_not_empty() {
    local value="$1"
    local msg="${2:-should not be empty}"
    if [[ -n "$value" ]]; then
        return 0
    else
        echo ""
        echo "  Value is empty"
        return 1
    fi
}

# ============================================================================
# Unit Tests
# ============================================================================

test_script_exists() {
    [[ -f "$QAI_SCRIPT" ]]
}

test_script_is_executable_syntax() {
    zsh -n "$QAI_SCRIPT"
}

test_help_output() {
    local output
    output=$(zsh "$QAI_SCRIPT" --help 2>&1)
    assert_contains "$output" "qai - Natural language"
}

test_usage_shows_examples() {
    local output
    output=$(zsh "$QAI_SCRIPT" --help 2>&1)
    assert_contains "$output" "qai list files by size"
}

test_shell_init_zsh_outputs_script() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    assert_contains "$output" "_qai_expand"
}

test_shell_init_contains_widget_registration() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    assert_contains "$output" "zle -N _qai_expand"
}

test_shell_init_contains_tab_binding() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    assert_contains "$output" "bindkey '^I' _qai_expand"
}

test_shell_init_contains_original_widget_capture() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    assert_contains "$output" "_qai_original_tab_widget"
}

test_shell_init_contains_fallback() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    assert_contains "$output" "expand-or-complete"
}

test_shell_init_checks_qai_prefix() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    assert_contains "$output" 'qai\ *'
}

test_shell_init_calls_original_widget_for_non_qai() {
    local output
    output=$(zsh "$QAI_SCRIPT" shell-init zsh 2>&1)
    # The else branch should call the original widget
    assert_contains "$output" 'zle "${_qai_original_tab_widget:-expand-or-complete}"'
}

test_shell_init_unsupported_shell_fails() {
    local output
    local exit_code=0
    output=$(zsh "$QAI_SCRIPT" shell-init bash 2>&1) || exit_code=$?
    [[ $exit_code -ne 0 ]] && assert_contains "$output" "Only 'zsh' is supported"
}

test_no_api_key_fails() {
    local output
    local exit_code=0
    # Unset API key and run
    output=$(OPENAI_API_KEY="" QAI_CONFIG_FILE="/nonexistent" zsh "$QAI_SCRIPT" query "test" 2>&1) || exit_code=$?
    [[ $exit_code -ne 0 ]]
}

test_no_api_key_shows_error() {
    local output
    output=$(OPENAI_API_KEY="" QAI_CONFIG_FILE="/nonexistent" zsh "$QAI_SCRIPT" query "test" 2>&1) || true
    assert_contains "$output" "OPENAI_API_KEY"
}

test_empty_query_fails() {
    local exit_code=0
    zsh "$QAI_SCRIPT" query 2>/dev/null || exit_code=$?
    [[ $exit_code -ne 0 ]]
}

test_no_args_fails() {
    local exit_code=0
    zsh "$QAI_SCRIPT" 2>/dev/null || exit_code=$?
    [[ $exit_code -ne 0 ]]
}

# ============================================================================
# ZLE Integration Tests (mocked)
# ============================================================================

test_qai_expand_logic_qai_prefix() {
    # Test the pattern matching logic
    local BUFFER="qai list files"
    [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_logic_not_qai_prefix() {
    # Should NOT match non-qai commands
    local BUFFER="ls -la"
    ! [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_logic_just_qai() {
    # Should NOT match just "qai " (need more content)
    local BUFFER="qai "
    ! [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_logic_qai_no_space() {
    # Should NOT match "qaistuff" (no space after qai)
    local BUFFER="qailist"
    ! [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_logic_partial_command() {
    # Real-world test: normal command with tab
    local BUFFER="eza -halo --git .confi"
    ! [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_logic_cd_command() {
    local BUFFER="cd /home/user/doc"
    ! [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_logic_git_command() {
    local BUFFER="git sta"
    ! [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]
}

test_qai_expand_query_extraction() {
    local BUFFER="qai find large files"
    local query="${BUFFER#qai }"
    assert_eq "find large files" "$query"
}

# ============================================================================
# Config Loading Tests
# ============================================================================

test_api_key_from_env() {
    local output
    # Create a subshell test (suppress stderr from sourcing)
    output=$(
        export OPENAI_API_KEY="test-key-from-env"
        source "$QAI_SCRIPT" 2>/dev/null
        load_api_key
    )
    assert_eq "test-key-from-env" "$output"
}

test_api_key_from_config_file() {
    local tmpfile=$(mktemp)
    echo "api_key: test-key-from-config" > "$tmpfile"
    local output
    output=$(
        unset OPENAI_API_KEY
        export QAI_CONFIG_FILE="$tmpfile"
        source "$QAI_SCRIPT" 2>/dev/null
        load_api_key
    )
    rm -f "$tmpfile"
    assert_eq "test-key-from-config" "$output"
}

test_api_key_env_takes_precedence() {
    local tmpfile=$(mktemp)
    echo "api_key: config-key" > "$tmpfile"
    local output
    output=$(
        export OPENAI_API_KEY="env-key"
        export QAI_CONFIG_FILE="$tmpfile"
        source "$QAI_SCRIPT" 2>/dev/null
        load_api_key
    )
    rm -f "$tmpfile"
    assert_eq "env-key" "$output"
}

# ============================================================================
# JSON Payload Tests (requires jq)
# ============================================================================

test_jq_available() {
    command -v jq &>/dev/null
}

test_json_payload_structure() {
    local payload
    payload=$(jq -n \
        --arg model "gpt-4o-mini" \
        --arg system "You are helpful" \
        --arg user "test query" \
        '{
            model: $model,
            messages: [
                {role: "system", content: $system},
                {role: "user", content: $user}
            ],
            temperature: 0.1,
            max_tokens: 500
        }')
    
    # Verify structure
    local model=$(echo "$payload" | jq -r '.model')
    assert_eq "gpt-4o-mini" "$model"
}

test_json_payload_messages() {
    local payload
    payload=$(jq -n \
        --arg model "gpt-4o-mini" \
        --arg system "system prompt" \
        --arg user "user query" \
        '{
            model: $model,
            messages: [
                {role: "system", content: $system},
                {role: "user", content: $user}
            ]
        }')
    
    local msg_count=$(echo "$payload" | jq '.messages | length')
    assert_eq "2" "$msg_count"
}

test_json_escapes_special_chars() {
    local query='find files with "quotes" and $vars'
    local payload
    payload=$(jq -n --arg user "$query" '{content: $user}')
    
    # Should be valid JSON
    echo "$payload" | jq . >/dev/null 2>&1
}

# ============================================================================
# API Response Parsing Tests
# ============================================================================

test_parse_openai_response() {
    local response='{
        "choices": [
            {
                "message": {
                    "content": "ls -lhS"
                }
            }
        ]
    }'
    local command
    command=$(echo "$response" | jq -r '.choices[0].message.content // empty')
    assert_eq "ls -lhS" "$command"
}

test_parse_empty_response() {
    local response='{}'
    local command
    command=$(echo "$response" | jq -r '.choices[0].message.content // empty' 2>/dev/null)
    assert_eq "" "$command"
}

test_parse_response_strips_backticks() {
    local response='{
        "choices": [{"message": {"content": "```ls -la```"}}]
    }'
    local command
    command=$(echo "$response" | jq -r '.choices[0].message.content // empty')
    command=$(echo "$command" | sed 's/^[`]*//; s/[`]*$//')
    assert_eq "ls -la" "$command"
}

# ============================================================================
# Run All Tests
# ============================================================================

main() {
    echo "========================================"
    echo "QAI Test Suite (zsh/jq implementation)"
    echo "========================================"
    echo ""
    
    # Basic tests
    echo "--- Script Tests ---"
    run_test "Script exists" test_script_exists
    run_test "Script has valid syntax" test_script_is_executable_syntax
    run_test "Help output works" test_help_output
    run_test "Usage shows examples" test_usage_shows_examples
    
    # Shell init tests
    echo ""
    echo "--- Shell Init Tests ---"
    run_test "shell-init outputs script" test_shell_init_zsh_outputs_script
    run_test "shell-init has widget registration" test_shell_init_contains_widget_registration
    run_test "shell-init has tab binding" test_shell_init_contains_tab_binding
    run_test "shell-init captures original widget" test_shell_init_contains_original_widget_capture
    run_test "shell-init has fallback" test_shell_init_contains_fallback
    run_test "shell-init checks qai prefix" test_shell_init_checks_qai_prefix
    run_test "shell-init calls original for non-qai" test_shell_init_calls_original_widget_for_non_qai
    run_test "shell-init rejects unsupported shells" test_shell_init_unsupported_shell_fails
    
    # Error handling
    echo ""
    echo "--- Error Handling Tests ---"
    run_test "No API key fails" test_no_api_key_fails
    run_test "No API key shows error" test_no_api_key_shows_error
    run_test "Empty query fails" test_empty_query_fails
    run_test "No args fails" test_no_args_fails
    
    # ZLE logic tests (CRITICAL for the bug fix)
    echo ""
    echo "--- ZLE Integration Logic Tests ---"
    run_test "Matches 'qai ' prefix" test_qai_expand_logic_qai_prefix
    run_test "Does NOT match non-qai commands" test_qai_expand_logic_not_qai_prefix
    run_test "Does NOT match just 'qai '" test_qai_expand_logic_just_qai
    run_test "Does NOT match 'qaiXXX' (no space)" test_qai_expand_logic_qai_no_space
    run_test "Does NOT match partial commands like 'eza -halo'" test_qai_expand_logic_partial_command
    run_test "Does NOT match 'cd /path'" test_qai_expand_logic_cd_command
    run_test "Does NOT match 'git sta'" test_qai_expand_logic_git_command
    run_test "Query extraction works" test_qai_expand_query_extraction
    
    # Config tests
    echo ""
    echo "--- Config Loading Tests ---"
    run_test "API key from env" test_api_key_from_env
    run_test "API key from config file" test_api_key_from_config_file
    run_test "Env key takes precedence" test_api_key_env_takes_precedence
    
    # JSON tests
    echo ""
    echo "--- JSON/jq Tests ---"
    run_test "jq is available" test_jq_available
    run_test "JSON payload structure" test_json_payload_structure
    run_test "JSON payload has messages" test_json_payload_messages
    run_test "JSON escapes special chars" test_json_escapes_special_chars
    
    # Response parsing
    echo ""
    echo "--- Response Parsing Tests ---"
    run_test "Parse OpenAI response" test_parse_openai_response
    run_test "Parse empty response" test_parse_empty_response
    run_test "Parse strips backticks" test_parse_response_strips_backticks
    
    # Summary
    echo ""
    echo "========================================"
    echo "Results: $TESTS_PASSED/$TESTS_RUN passed"
    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo "${RED}$TESTS_FAILED tests failed${NC}"
        return 1
    else
        echo "${GREEN}All tests passed!${NC}"
        return 0
    fi
}

main "$@"

