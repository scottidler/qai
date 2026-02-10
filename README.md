# qai

Natural language to shell commands via an OpenAI‑compatible API.

## Requirements

- Rust toolchain (stable)
- A compatible API endpoint (OpenAI or local)
- `zsh` for shell integration
- `fzf` (optional, for multi‑choice selection)

## Build

```bash
cargo build --release
```

Binary location: `target/release/qai`

## Install (optional)

```bash
cp target/release/qai /usr/local/bin/
```

## Configuration

Config file locations (first found wins):

1. `~/.config/qai/qai.yml`
2. `./qai.yml`

Example:

```yaml
# API base URL
api-base: "http://localhost:8000/v1"

# Model to use
model: "gpt-4o-mini"

# API key (optional for local servers)
# api-key: "sk-..."

# Allow running without an API key (for local OpenAI‑compatible servers)
allow-no-api-key: true

# Max tokens to generate
max-tokens: 800

# HTTP timeout in seconds
http-timeout-secs: 45

# Keybindings for zsh integration
bindings:
  trigger: tab
  submit: enter
```

Notes:
- `api_key`, `allow_no_api_key`, `max_tokens`, and `http_timeout_secs` (snake_case) are also accepted.
- If `allow-no-api-key: true` is set and no key is provided, `qai validate-api` becomes a no‑op.

## Usage

Single query:

```bash
qai query how to find files with 'ai' in their name
```

Validate API (non‑inference `/v1/models` call):

```bash
qai validate-api
```

## Zsh Integration (interactive mode)

Add to your `~/.zshrc`:

```zsh
eval "$(qai shell-init zsh)"
```

Open a new shell or `source ~/.zshrc`.

Notes:
- The quotes around `$(qai shell-init zsh)` are required to avoid word-splitting.
- If the trigger key does not work, make sure this line is at the very end of your `.zshrc` so it is not overridden by later keybindings.

Workflow:

1. Type exactly `ai` and press **Tab** (default trigger).
2. Prompt changes to `🤖 ai>`.
3. Type your query and press **Enter**.
4. If `fzf` is installed, you’ll see multiple options. Otherwise, you’ll get a single command.

## Local Models

For a local OpenAI‑compatible server, point `api-base` at your server (including `/v1`) and allow no API key if your server doesn’t require one.

Example `qai.yml`:

```yaml
api-base: "http://localhost:8000/v1"
model: "gpt-oss-120b"
allow-no-api-key: true
```

Optional tuning:

```yaml
max-tokens: 800
http-timeout-secs: 60
```

## Troubleshooting

- **No choices shown**: ensure `fzf` is installed and on `PATH`.
- **Trigger doesn’t activate**: confirm you typed `ai` exactly, and `qai shell-init zsh` is loaded.
- **API key errors**: confirm config path and `api-key` value, or set `allow-no-api-key: true` for local servers.
- **Local server**: make sure `api-base` includes `/v1`.

Quick checks:

```zsh
which qai
qai --help
qai shell-init zsh | head -n 5
command -v fzf
```
