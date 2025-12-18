# QAI Architecture

**Version:** 0.1.0
**Status:** Implementation

## Overview

`qai` is a command-line tool that translates natural language queries into shell commands using an LLM (OpenAI GPT-4o-mini). It integrates with ZSH via ZLE (Zsh Line Editor) to provide inline command generation triggered by the Tab key.

## User Experience

### Basic Flow

```
User types:    qai git show the date of the first commit
User presses:  Tab
Buffer becomes: git log --reverse --format=%cd --date=short | head -n 1
User presses:  Enter (to execute)
```

### Trigger Conditions

Tab expansion only activates when ALL conditions are met:
1. Line starts with `qai ` (with space)
2. There is text beyond just `qai `
3. Tab key is pressed

If conditions are not met, Tab performs normal ZSH completion.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         ZSH Shell                           │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │ User Input  │───▶│  ZLE Widget  │───▶│  qai binary   │  │
│  │ "qai ..."   │    │  (Tab key)   │    │  query "..."  │  │
│  └─────────────┘    └──────────────┘    └───────┬───────┘  │
│                                                  │          │
│  ┌─────────────┐    ┌──────────────┐    ┌───────▼───────┐  │
│  │   BUFFER    │◀───│  Replace     │◀───│ OpenAI API    │  │
│  │  (result)   │    │  + cursor    │    │ GPT-4o-mini   │  │
│  └─────────────┘    └──────────────┘    └───────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Components

### 1. CLI Binary (`qai`)

Built with Rust + Clap. Subcommands:

| Command | Description |
|---------|-------------|
| `qai query "<text>"` | Send query to LLM, print resulting command |
| `qai shell-init zsh` | Print ZSH integration script to stdout |

### 2. ZSH Integration (`qai.zsh`)

ZLE widget that intercepts Tab key:

```zsh
qai-expand() {
    # Only trigger if line starts with "qai " and has content
    if [[ "$BUFFER" == qai\ * && ${#BUFFER} -gt 4 ]]; then
        local query="${BUFFER#qai }"
        local result=$(qai query "$query" 2>/dev/null)

        if [[ -n "$result" ]]; then
            BUFFER="$result"
            CURSOR=${#BUFFER}
        fi
    else
        # Fall through to normal tab completion
        zle expand-or-complete
    fi
}
```

Installation via: `eval "$(qai shell-init zsh)"` in `.zshrc`

### 3. Configuration (`qai.yml`)

Location priority:
1. `--config <path>` (explicit)
2. `~/.config/qai/qai.yml` (primary)
3. `./qai.yml` (fallback)

```yaml
# ~/.config/qai/qai.yml
api_key: "sk-..."           # Or use QAI_API_KEY env var
model: "gpt-4o-mini"        # Default model
api_base: "https://api.openai.com/v1"  # Optional override
debug: false
```

API key resolution order:
1. `QAI_API_KEY` environment variable
2. `api_key` in config file

### 4. Prompt Templates (`.pmt` files)

**Location priority:**
1. `~/.config/qai/prompts/system.pmt` (user override)
2. Embedded default (compiled into binary)

**Default `system.pmt`:**

```
You are a shell command generator. Given a natural language description, output ONLY the shell command(s) that accomplish the task.

Rules:
- Output ONLY the command, no explanations
- No markdown formatting or backticks
- No commentary before or after
- If multiple commands needed, separate with && or ;
- Use common Unix utilities when possible

Context:
- Shell: {{shell}}
- OS: {{os}}
- Working directory: {{cwd}}
```

Template variables:
- `{{shell}}` - User's shell (from `$SHELL`)
- `{{os}}` - Operating system
- `{{cwd}}` - Current working directory

## File Structure

```
qai/
├── Cargo.toml
├── build.rs                    # Git describe for version
├── docs/
│   └── architecture.md         # This file
├── prompts/
│   └── system.pmt              # Default prompt (embedded at compile)
├── src/
│   ├── main.rs                 # Entry point
│   ├── cli.rs                  # Clap argument definitions
│   ├── config.rs               # Configuration loading
│   ├── api.rs                  # OpenAI API client
│   ├── prompt.rs               # Prompt template loading
│   └── shell.rs                # Shell init script generation
└── qai.yml                     # Example config
```

## API Integration

### OpenAI Request

```rust
POST https://api.openai.com/v1/chat/completions
{
    "model": "gpt-4o-mini",
    "messages": [
        {"role": "system", "content": "<system.pmt content>"},
        {"role": "user", "content": "<user query>"}
    ],
    "temperature": 0,
    "max_tokens": 500
}
```

### Response Handling

- Success: Print command to stdout, exit 0
- API Error: Print error to stderr, exit 1
- Network Error: Print error to stderr, exit 1

## Error Handling

| Error Type | Behavior |
|------------|----------|
| No API key | Print error to stderr, exit 1 |
| Network failure | Print error to stderr, exit 1 |
| API rate limit | Print error to stderr, exit 1 |
| Invalid response | Print error to stderr, exit 1 |

ZLE widget leaves `BUFFER` unchanged on any error.

## Installation

```bash
# Install binary
cargo install --path .

# Add to ~/.zshrc
eval "$(qai shell-init zsh)"

# Create config
mkdir -p ~/.config/qai
cat > ~/.config/qai/qai.yml << 'EOF'
api_key: "sk-your-key-here"
model: "gpt-4o-mini"
EOF
```

## Security Considerations

1. **API Key Storage**: Prefer `QAI_API_KEY` env var over config file
2. **Command Execution**: User must explicitly press Enter to execute
3. **No Auto-Execute**: Intentionally no `-y` flag for safety

## Future Enhancements (Out of Scope for v1)

- [ ] Multiple LLM providers (Claude, Gemini)
- [ ] fzf integration for multiple suggestions
- [ ] Command history/caching
- [ ] Streaming responses
- [ ] Daemon mode for faster responses
