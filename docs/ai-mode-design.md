# QAI AI Mode Design Document

**Version:** 1.0
**Status:** Design Phase

---

## Executive Summary

This document specifies the architecture for an enhanced "AI Mode" in qai, transforming the current single-command expansion into an interactive, multi-result selection experience. Users type `ai<TAB>` to enter AI mode, describe what they want in natural language, and select from multiple command suggestions using fzf (with graceful fallback to single-result mode).

### Key Features

1. **Mode-based interaction**: `ai<TAB>` enters AI mode, `Ctrl+C` exits
2. **Multi-result selection**: AI returns multiple command options
3. **fzf integration**: Interactive fuzzy selection of commands
4. **Graceful fallback**: Single-result mode when fzf unavailable
5. **Learning system**: Track queries, results, selections, and corrections
6. **Optional daemon mode**: Persistent process for faster responses
7. **Tool discovery**: Detect modern CLI tools (eza, rg, fd, etc.) and validate commands

---

## User Experience Flow

### Happy Path

```
❯ ai<TAB>
[AI] ❯ find all rust files modified in the last week
⣾ Thinking...
┌─ Select command ─────────────────────────────────────────┐
│ > find . -name "*.rs" -mtime -7                          │
│   find . -type f -name "*.rs" -mtime -7 -ls              │
│   fd -e rs --changed-within 1week                        │
│   find . -name "*.rs" -newermt "1 week ago"              │
└──────────────────────────────────────────────────────────┘
  ↑/↓: navigate  Enter: select  Esc: cancel
```

User presses Enter on selection:

```
❯ find . -name "*.rs" -mtime -7
```

### Mode States

```
┌─────────────────────────────────────────────────────────────────────┐
│                          STATE DIAGRAM                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌──────────┐   ai<TAB>    ┌──────────┐    <CR>     ┌───────────┐  │
│   │  NORMAL  │ ───────────► │ AI_MODE  │ ──────────► │ FETCHING  │  │
│   └──────────┘              └──────────┘             └───────────┘  │
│        ▲                         │                         │        │
│        │ <ESC> or <C-c>          │ <ESC> or <C-c>         ▼        │
│        │                         │                  ┌───────────┐  │
│        └─────────────────────────┴─────────────────►│ SELECTING │  │
│                                                      └───────────┘  │
│                                                            │        │
│        ┌───────────────────────────────────────────────────┘        │
│        │ <Enter>                                                    │
│        ▼                                                            │
│   ┌──────────┐                                                      │
│   │ SELECTED │  → Command in BUFFER, cursor at end                  │
│   └──────────┘                                                      │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Architecture Overview

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                           ZSH SHELL                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  ┌─────────────────┐    ┌──────────────────┐    ┌────────────────┐  │
│  │  ZLE Widgets    │    │   qai binary     │    │    fzf         │  │
│  │                 │    │                  │    │                │  │
│  │ _qai_tab_handler│───►│ qai query        │───►│ Selection UI   │  │
│  │ _qai_submit     │    │ --multi-result   │    │                │  │
│  │ _qai_select     │◄───│                  │◄───│                │  │
│  └─────────────────┘    └──────────────────┘    └────────────────┘  │
│           │                      │                                   │
│           │                      ▼                                   │
│           │             ┌──────────────────┐                        │
│           │             │  History Store   │                        │
│           │             │  (SQLite/files)  │                        │
│           │             └──────────────────┘                        │
│           │                      │                                   │
│           │                      ▼                                   │
│           │             ┌──────────────────┐                        │
│           └────────────►│  OpenAI API      │                        │
│                         │  (via daemon or  │                        │
│                         │   direct call)   │                        │
│                         └──────────────────┘                        │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Optional Daemon Architecture

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   ZLE Widgets   │    │   qai client     │    │   qai-daemon    │
│                 │    │   (thin client)  │    │  (persistent)   │
├─────────────────┤    ├──────────────────┤    ├─────────────────┤
│ _qai_submit()   │───►│ query request    │───►│ API connection  │
│                 │    │ via Unix socket  │    │ pool            │
│                 │◄───│                  │◄───│ history cache   │
│                 │    │ fallback if      │    │ context cache   │
│                 │    │ daemon down      │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

---

## Phase 1: Core AI Mode

### 1.1 ZLE Widget Implementation

```zsh
# Global state
typeset -g _QAI_MODE=0           # 0=normal, 1=ai_mode
typeset -g _QAI_ORIGINAL_PROMPT  # Store original prompt

# Tab key handler - dispatch based on buffer content
_qai_tab_handler() {
    if [[ "$BUFFER" == "ai" ]]; then
        _qai_start
    else
        # Not "ai", use original tab behavior
        zle "${_qai_original_tab_widget:-expand-or-complete}"
    fi
}

# Start AI mode session
_qai_start() {
    # Validate API key by calling OpenAI (non-inference endpoint, no token usage)
    local validation_result
    validation_result=$(qai validate-api 2>&1)
    local exit_code=$?

    if [[ $exit_code -ne 0 ]]; then
        zle -M "❌ $validation_result"
        BUFFER=""
        return 1
    fi

    _QAI_MODE=1
    _QAI_ORIGINAL_PROMPT="$PROMPT"

    # Change prompt to indicate AI mode
    PROMPT='[AI] ❯ '
    BUFFER=""
    CURSOR=0

    zle reset-prompt
}

_qai_exit() {
    _QAI_MODE=0
    PROMPT="$_QAI_ORIGINAL_PROMPT"
    BUFFER=""
    zle reset-prompt
}

# Handle Enter in AI mode
_qai_submit() {
    if [[ $_QAI_MODE -eq 0 ]]; then
        zle accept-line
        return
    fi

    local query="$BUFFER"
    if [[ -z "$query" ]]; then
        _qai_exit
        return
    fi

    # Show thinking indicator
    zle -M "⣾ Thinking..."

    # Get results from qai
    local results
    results=$(qai query --multi "$query" 2>/dev/null)
    local exit_code=$?

    zle -M ""  # Clear message

    if [[ $exit_code -ne 0 || -z "$results" ]]; then
        zle -M "Error: No results from AI"
        return
    fi

    # Select result
    local selected
    selected=$(_qai_select_result "$results")

    if [[ -n "$selected" ]]; then
        _qai_exit
        BUFFER="$selected"
        CURSOR=${#BUFFER}
    fi
}

_qai_select_result() {
    local results="$1"
    local line_count=$(echo "$results" | wc -l)

    if command -v fzf &>/dev/null && [[ $line_count -gt 1 ]]; then
        # Use fzf for multi-result selection
        echo "$results" | fzf --height=40% --reverse --no-info \
            --header="Select command (Enter=select, Esc=cancel)"
    else
        # Fallback: return first result
        echo "$results" | head -1
    fi
}

# Ctrl+C handler
_qai_interrupt() {
    if [[ $_QAI_MODE -eq 1 ]]; then
        _qai_exit
    else
        zle send-break
    fi
}

# Widget registration
zle -N _qai_tab_handler
zle -N _qai_submit
zle -N _qai_exit
zle -N _qai_interrupt

# Key bindings
bindkey '^I' _qai_tab_handler  # Tab
bindkey '^M' _qai_submit            # Enter (in AI mode)
bindkey '^C' _qai_interrupt         # Ctrl+C
```

### 1.2 Rust CLI Changes

New command structure:

```rust
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Convert natural language to shell command(s)
    Query {
        /// Return multiple command options
        #[arg(short, long)]
        multi: bool,

        /// Number of results to return (default: 5)
        #[arg(short = 'n', long, default_value = "5")]
        count: usize,

        /// The natural language query
        query: Vec<String>,
    },

    /// Output shell integration script
    ShellInit {
        /// Shell type (zsh)
        shell: String,
    },

    /// Validate API key (calls OpenAI, no token usage)
    /// Used by shell integration before entering AI mode
    #[command(name = "validate-api")]
    ValidateApi,

    /// Show query history
    History {
        /// Number of entries to show
        #[arg(short = 'n', long, default_value = "20")]
        count: usize,
    },

    /// Daemon management (optional)
    #[cfg(feature = "daemon")]
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
}
```

### 1.3 API Key Validation

Before entering AI mode, we validate the API key by calling a **non-inference endpoint**. This confirms the key is actually valid without consuming tokens.

```rust
/// Validate API key by calling GET /v1/models
///
/// This endpoint:
/// - Authenticates the key
/// - Does NOT run inference
/// - Does NOT consume tokens
/// - Is fast and stable
pub async fn validate_api_key(api_key: &str) -> Result<(), ApiValidationError> {
    let client = reqwest::Client::new();

    let response = client
        .get("https://api.openai.com/v1/models")
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await?;

    match response.status() {
        StatusCode::OK => Ok(()),
        StatusCode::UNAUTHORIZED => Err(ApiValidationError::InvalidKey(
            "API key is invalid or revoked".to_string()
        )),
        StatusCode::FORBIDDEN => Err(ApiValidationError::AccessDenied(
            "API key lacks required permissions".to_string()
        )),
        status => Err(ApiValidationError::UnexpectedError(
            format!("Unexpected response: {}", status)
        )),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApiValidationError {
    #[error("Invalid API key: {0}")]
    InvalidKey(String),

    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("API key not configured. Set OPENAI_API_KEY environment variable.")]
    NotConfigured,

    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("{0}")]
    UnexpectedError(String),
}
```

**CLI Handler:**

```rust
fn handle_validate_api() -> i32 {
    // First check if key exists
    let api_key = match get_api_key() {
        Some(key) => key,
        None => {
            eprintln!("API key not configured. Set OPENAI_API_KEY or add to config.");
            return 1;
        }
    };

    // Actually validate with OpenAI
    let rt = tokio::runtime::Runtime::new().unwrap();
    match rt.block_on(validate_api_key(&api_key)) {
        Ok(()) => {
            // Silent success for shell integration
            0
        }
        Err(e) => {
            eprintln!("{}", e);
            1
        }
    }
}
```

**Validation Results:**

| HTTP Status | Meaning | Exit Code |
|-------------|---------|-----------|
| `200 OK` | Key is valid | 0 |
| `401 Unauthorized` | Key is invalid or revoked | 1 |
| `403 Forbidden` | Key lacks access | 1 |
| Network error | Can't reach API | 1 |

### 1.4 Multi-Result API Prompt

Updated system prompt for multi-result mode:

```rust
const MULTI_RESULT_SYSTEM_PROMPT: &str = r#"
You are a shell command generator. Convert natural language to shell commands.

RULES:
1. Return {count} alternative commands, one per line
2. Order by: most likely what user wants first
3. Include variety: different tools/approaches when applicable
4. Output ONLY commands - no explanations, no markdown, no numbering
5. Each line must be a complete, valid command

CONTEXT:
- Shell: zsh on Linux/macOS
- Current directory context may be provided

Example input: "find large files"
Example output:
find . -type f -size +100M
du -ah . | sort -rh | head -20
fd --size +100m
ls -lhS | head -20
ncdu .
"#;
```

---

## Phase 2: History & Learning System

### 2.1 Data Model

```rust
/// A single query interaction
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryRecord {
    /// Unique identifier
    pub id: Uuid,

    /// When this query was made
    pub timestamp: DateTime<Utc>,

    /// The natural language query
    pub query: String,

    /// Commands returned by the AI
    pub results: Vec<String>,

    /// Which command the user selected (index into results)
    pub selected_index: Option<usize>,

    /// If user edited the command before executing
    pub edited_command: Option<String>,

    /// Whether the command was actually executed
    pub executed: bool,

    /// Working directory when query was made
    pub cwd: Option<PathBuf>,

    /// Model used for this query
    pub model: String,
}

/// Aggregated statistics for a query pattern
#[derive(Debug, Serialize, Deserialize)]
pub struct QueryPattern {
    /// Normalized query (lowercase, trimmed)
    pub normalized_query: String,

    /// Number of times this pattern was queried
    pub query_count: u32,

    /// Most frequently selected command
    pub preferred_command: Option<String>,

    /// All commands ever selected for this pattern
    pub command_history: Vec<CommandSelection>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandSelection {
    pub command: String,
    pub selection_count: u32,
    pub last_selected: DateTime<Utc>,
}
```

### 2.2 Storage Options

#### Option A: SQLite (Recommended)

```sql
-- Schema for qai history database

CREATE TABLE queries (
    id TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL,
    query TEXT NOT NULL,
    cwd TEXT,
    model TEXT NOT NULL
);

CREATE TABLE results (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query_id TEXT NOT NULL REFERENCES queries(id),
    command TEXT NOT NULL,
    position INTEGER NOT NULL,  -- Order in results
    selected BOOLEAN DEFAULT FALSE,
    edited_to TEXT,  -- If user modified before executing
    executed BOOLEAN DEFAULT FALSE
);

CREATE TABLE patterns (
    normalized_query TEXT PRIMARY KEY,
    query_count INTEGER DEFAULT 1,
    preferred_command TEXT,
    last_used TEXT
);

-- Indexes for common queries
CREATE INDEX idx_queries_timestamp ON queries(timestamp);
CREATE INDEX idx_queries_query ON queries(query);
CREATE INDEX idx_results_query_id ON results(query_id);
CREATE INDEX idx_patterns_count ON patterns(query_count DESC);
```

**Pros:**
- Single file, easy backup/sync
- Complex queries (analytics, patterns)
- ACID transactions
- Well-supported in Rust (rusqlite)

**Cons:**
- Additional dependency
- Slightly more complex than flat files

#### Option B: Flat Files (JSON Lines)

```
~/.local/share/qai/
├── history.jsonl          # Append-only query log
├── patterns.json          # Aggregated patterns (rebuilt periodically)
└── corrections.jsonl      # User corrections/edits
```

**Pros:**
- Human-readable
- Easy to edit/debug
- No dependencies
- Git-friendly (can version control)

**Cons:**
- Slower for large histories
- No complex queries
- Manual aggregation needed

### 2.3 Learning Features

#### Personalized Ranking

```rust
impl HistoryStore {
    /// Re-rank AI results based on user history
    pub fn personalize_results(
        &self,
        query: &str,
        ai_results: Vec<String>,
    ) -> Vec<String> {
        let normalized = normalize_query(query);

        // Check if we have history for this pattern
        if let Some(pattern) = self.get_pattern(&normalized) {
            // Boost commands user has selected before
            let mut scored: Vec<(String, f32)> = ai_results
                .into_iter()
                .map(|cmd| {
                    let score = self.score_command(&cmd, &pattern);
                    (cmd, score)
                })
                .collect();

            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            scored.into_iter().map(|(cmd, _)| cmd).collect()
        } else {
            ai_results
        }
    }

    fn score_command(&self, cmd: &str, pattern: &QueryPattern) -> f32 {
        let mut score = 0.0;

        // Exact match with preferred command
        if Some(cmd) == pattern.preferred_command.as_deref() {
            score += 10.0;
        }

        // Previously selected
        for selection in &pattern.command_history {
            if selection.command == cmd {
                score += (selection.selection_count as f32).ln();
            }
        }

        score
    }
}
```

#### Correction Learning

When user edits a selected command before execution:

```rust
impl HistoryStore {
    /// Record that user corrected a command
    pub fn record_correction(
        &mut self,
        query_id: Uuid,
        original: &str,
        corrected: &str,
    ) -> Result<()> {
        // Store the correction
        self.corrections.push(Correction {
            query_id,
            original: original.to_string(),
            corrected: corrected.to_string(),
            timestamp: Utc::now(),
        });

        // In the future, these corrections could be:
        // 1. Sent to fine-tune a personal model
        // 2. Used to adjust the system prompt
        // 3. Used as few-shot examples in the prompt

        Ok(())
    }
}
```

---

## Phase 3: Daemon Mode (Optional)

Following the `aka` daemon architecture pattern.

### 3.1 Benefits

- **Connection pooling**: Reuse HTTP connections to OpenAI
- **Response caching**: Cache identical queries
- **Streaming**: Stream tokens as they arrive for faster perceived response
- **Context caching**: Keep conversation context in memory
- **Background learning**: Aggregate patterns without blocking

### 3.2 Protocol Definition

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    /// Query for command suggestions
    Query {
        version: String,
        query: String,
        multi: bool,
        count: usize,
        cwd: Option<PathBuf>,
    },

    /// Record user's selection
    RecordSelection {
        version: String,
        query_id: Uuid,
        selected_index: usize,
        edited_command: Option<String>,
        executed: bool,
    },

    /// Health check
    Health,

    /// Get history
    History {
        version: String,
        count: usize,
    },

    /// Shutdown daemon
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum DaemonResponse {
    /// Query results
    QueryResult {
        query_id: Uuid,
        commands: Vec<String>,
        cached: bool,
    },

    /// Selection recorded
    SelectionRecorded,

    /// Health status
    Health {
        status: String,
        uptime_secs: u64,
        queries_served: u64,
        cache_hit_rate: f32,
    },

    /// History results
    HistoryResult {
        entries: Vec<HistoryEntry>,
    },

    /// Error
    Error {
        message: String,
    },

    /// Version mismatch
    VersionMismatch {
        daemon_version: String,
        client_version: String,
    },

    /// Shutdown acknowledgment
    ShutdownAck,
}
```

### 3.3 Socket Location

```rust
fn determine_socket_path() -> PathBuf {
    // XDG_RUNTIME_DIR (preferred, auto-cleaned on logout)
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir).join("qai/daemon.sock");
    }

    // Fallback
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("qai/daemon.sock")
}
```

### 3.4 Daemon Binary Structure

Following `aka` pattern with two binaries:

```
src/
├── bin/
│   ├── qai.rs          # Main CLI (client)
│   └── qai-daemon.rs   # Daemon process
├── lib.rs              # Shared library
├── api.rs              # OpenAI API client
├── protocol.rs         # IPC protocol
├── history.rs          # History/learning storage
└── shell.rs            # Shell integration
```

### 3.5 Service Management

```rust
pub enum ServiceManager {
    Systemd(SystemdManager),   // Linux
    Launchd(LaunchdManager),   // macOS
    Manual,                     // Fallback
}

impl ServiceManager {
    pub fn install(&self) -> Result<()>;
    pub fn start(&self) -> Result<()>;
    pub fn stop(&self) -> Result<()>;
    pub fn status(&self) -> Result<ServiceStatus>;
}
```

---

## Phase 4: Context Awareness

### 4.1 Context Providers

```rust
pub trait ContextProvider {
    fn get_context(&self) -> Option<String>;
    fn priority(&self) -> u8;  // Higher = more important
}

pub struct GitContextProvider;
impl ContextProvider for GitContextProvider {
    fn get_context(&self) -> Option<String> {
        // Check if in git repo
        let output = Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        // Get branch, status summary
        let branch = Command::new("git")
            .args(["branch", "--show-current"])
            .output()
            .ok()?;

        Some(format!(
            "Git repository, branch: {}",
            String::from_utf8_lossy(&branch.stdout).trim()
        ))
    }

    fn priority(&self) -> u8 { 5 }
}

pub struct DirectoryContextProvider;
impl ContextProvider for DirectoryContextProvider {
    fn get_context(&self) -> Option<String> {
        let cwd = std::env::current_dir().ok()?;

        // Check for common project indicators
        let indicators = [
            ("Cargo.toml", "Rust project"),
            ("package.json", "Node.js project"),
            ("pyproject.toml", "Python project"),
            ("go.mod", "Go project"),
            ("Makefile", "Make-based project"),
        ];

        for (file, description) in indicators {
            if cwd.join(file).exists() {
                return Some(description.to_string());
            }
        }

        None
    }

    fn priority(&self) -> u8 { 3 }
}
```

### 4.2 Enhanced System Prompt

```rust
fn build_system_prompt(context: &[String], multi: bool, count: usize) -> String {
    let mut prompt = if multi {
        format!(
            "You are a shell command generator. Return {} alternative commands.\n\n",
            count
        )
    } else {
        "You are a shell command generator. Return a single command.\n\n".to_string()
    };

    prompt.push_str("RULES:\n");
    prompt.push_str("1. Output ONLY commands - no explanations, no markdown\n");
    prompt.push_str("2. Use common Unix tools\n");
    prompt.push_str("3. Prefer safe options when available\n\n");

    if !context.is_empty() {
        prompt.push_str("CONTEXT:\n");
        for ctx in context {
            prompt.push_str(&format!("- {}\n", ctx));
        }
        prompt.push('\n');
    }

    prompt
}
```

---

## Phase 5: Tool Discovery & Validation

### 5.1 Overview

The AI knows about modern CLI tools (eza, rg, fd, bat, etc.) but the user may not have them installed. This phase ensures:

1. AI suggests commands using BOTH modern tools AND traditional Unix tools
2. Commands are validated against available binaries before presenting to user
3. Discovered tools are cached for future prompt enhancement

### 5.2 Dual-List Response Format

The system prompt requires the AI to return TWO discrete lists:

```
RESPONSE FORMAT:
Return commands in two sections:

MODERN:
<commands using modern tools like eza, rg, fd, bat, jq, delta, httpie, etc.>

STANDARD:
<commands using traditional Unix tools: ls, grep, find, cat, awk, sed, curl>

Always include at least 2 commands in STANDARD section.
The MODERN section may be empty if no modern tools are appropriate.
```

#### Example Response

```
User query: "find large files over 100MB"

MODERN:
fd --size +100m
dust -r -n 20

STANDARD:
find . -type f -size +100M
find . -type f -size +100M -exec ls -lh {} \;
du -ah . | sort -rh | head -20
```

### 5.3 Response Parsing

```rust
#[derive(Debug, Default)]
pub struct DualCommandList {
    /// Commands using modern tools (may be empty)
    pub modern: Vec<String>,
    /// Commands using standard Unix tools (guaranteed non-empty)
    pub standard: Vec<String>,
}

impl DualCommandList {
    /// Parse AI response into dual lists
    pub fn parse(response: &str) -> Self {
        let mut result = Self::default();
        let mut current_section: Option<&str> = None;

        for line in response.lines() {
            let line = line.trim();

            if line.eq_ignore_ascii_case("MODERN:") {
                current_section = Some("modern");
                continue;
            }
            if line.eq_ignore_ascii_case("STANDARD:") {
                current_section = Some("standard");
                continue;
            }

            // Skip empty lines and section markers
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            match current_section {
                Some("modern") => result.modern.push(line.to_string()),
                Some("standard") => result.standard.push(line.to_string()),
                None => {
                    // Fallback: treat as standard if no section marker
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
}
```

### 5.4 Binary Validation

```rust
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::SystemTime;

/// Cache for tool availability checks
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolCache {
    /// Tools confirmed to exist on this system
    pub available: HashSet<String>,

    /// Tools confirmed NOT to exist (avoid re-checking)
    pub unavailable: HashSet<String>,

    /// Cache version for format changes
    #[serde(default)]
    pub version: u32,

    /// When cache was last modified
    #[serde(skip)]
    pub dirty: bool,
}

impl ToolCache {
    const CACHE_VERSION: u32 = 1;

    /// Load cache from disk
    pub fn load() -> Self {
        let cache_path = Self::cache_path();
        if let Ok(content) = std::fs::read_to_string(&cache_path) {
            if let Ok(mut cache) = serde_json::from_str::<Self>(&content) {
                if cache.version == Self::CACHE_VERSION {
                    cache.dirty = false;
                    return cache;
                }
            }
        }
        Self::default()
    }

    /// Save cache to disk (if dirty)
    pub fn save(&self) -> eyre::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let cache_path = Self::cache_path();
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(cache_path, content)?;
        Ok(())
    }

    fn cache_path() -> PathBuf {
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

        // Slow path: check PATH
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
    /// Handles: sudo, env VAR=x, time, nice, etc.
    pub fn extract_binary(cmd: &str) -> Option<&str> {
        let skip_words = ["sudo", "env", "time", "nice", "nohup", "strace", "ltrace"];

        cmd.split_whitespace()
            .find(|word| {
                !word.contains('=') && !skip_words.contains(word)
            })
    }

    /// Get list of available tools for prompt enhancement
    pub fn available_tools_for_prompt(&self) -> String {
        if self.available.is_empty() {
            return String::new();
        }

        // Filter to "interesting" modern tools (not standard Unix)
        let standard_tools: HashSet<&str> = [
            "ls", "cat", "grep", "find", "awk", "sed", "sort", "uniq",
            "head", "tail", "cut", "wc", "du", "df", "ps", "top",
            "chmod", "chown", "cp", "mv", "rm", "mkdir", "rmdir",
            "curl", "wget", "tar", "gzip", "gunzip", "zip", "unzip",
        ].into_iter().collect();

        let modern_tools: Vec<&String> = self.available
            .iter()
            .filter(|t| !standard_tools.contains(t.as_str()))
            .collect();

        if modern_tools.is_empty() {
            return String::new();
        }

        format!(
            "User has these modern tools installed: {}\nPrefer these when appropriate.\n",
            modern_tools.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        )
    }
}
```

### 5.5 Command Filtering

```rust
impl ToolCache {
    /// Filter commands to only those with available binaries
    /// Returns (available_commands, unavailable_commands)
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
}
```

### 5.6 System Prompt Update

```rust
fn build_system_prompt_with_tools(
    context: &[String],
    tool_cache: &ToolCache,
    multi: bool,
    count: usize,
) -> String {
    let mut prompt = String::new();

    // Base instructions
    prompt.push_str("You are a shell command generator.\n\n");

    // Tool discovery prompt
    prompt.push_str(
        "RESPONSE FORMAT:
Return commands in two sections:

MODERN:
<commands using modern CLI tools: eza, rg, fd, bat, delta, jq, httpie, dust, procs, etc.>

STANDARD:
<commands using traditional Unix tools: ls, grep, find, cat, awk, sed, du, ps, curl, etc.>

RULES:
1. The MODERN section may be empty if no modern tools fit the task
2. The STANDARD section MUST have at least 2 commands
3. Output ONLY the commands - no explanations, no markdown, no backticks
4. Each command on its own line

"
    );

    // Add known available tools if we have them cached
    let tools_hint = tool_cache.available_tools_for_prompt();
    if !tools_hint.is_empty() {
        prompt.push_str(&tools_hint);
        prompt.push('\n');
    }

    // Result count
    if multi {
        prompt.push_str(&format!(
            "Return up to {} total commands across both sections.\n\n",
            count
        ));
    } else {
        prompt.push_str("Return 1 command from STANDARD section only.\n\n");
    }

    // Context
    if !context.is_empty() {
        prompt.push_str("CONTEXT:\n");
        for ctx in context {
            prompt.push_str(&format!("- {}\n", ctx));
        }
        prompt.push('\n');
    }

    prompt
}
```

### 5.7 Integration Flow

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        TOOL DISCOVERY & VALIDATION FLOW                       │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. User enters: "find rust files modified this week"                        │
│                                                                              │
│  2. Build prompt with tool hints (from cache):                               │
│     "User has these modern tools: eza, rg, fd, bat"                          │
│                                                                              │
│  3. AI returns dual-list response:                                           │
│                                                                              │
│     MODERN:                                                                  │
│     fd -e rs --changed-within 1week                                          │
│     rg --files -g "*.rs" -t rust                                             │
│                                                                              │
│     STANDARD:                                                                │
│     find . -name "*.rs" -mtime -7                                            │
│     find . -type f -name "*.rs" -mtime -7 -ls                                │
│                                                                              │
│  4. Parse response into DualCommandList                                      │
│                                                                              │
│  5. Validate each command's binary:                                          │
│     - fd ✓ (cache hit: available)                                            │
│     - rg ✓ (cache hit: available)                                            │
│     - find ✓ (always available)                                              │
│                                                                              │
│  6. Merge and present to user via fzf:                                       │
│     ┌─ Select command ─────────────────────────────────────────┐             │
│     │ > fd -e rs --changed-within 1week                        │             │
│     │   rg --files -g "*.rs" -t rust                           │             │
│     │   find . -name "*.rs" -mtime -7                          │             │
│     │   find . -type f -name "*.rs" -mtime -7 -ls              │             │
│     └──────────────────────────────────────────────────────────┘             │
│                                                                              │
│  7. Update cache with any newly discovered tools                             │
│                                                                              │
│  8. Save cache to ~/.cache/qai/tools.json                                    │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.8 Cache File Format

Location: `~/.cache/qai/tools.json`

```json
{
  "available": [
    "eza",
    "rg",
    "fd",
    "bat",
    "jq",
    "fzf",
    "docker",
    "kubectl",
    "gh"
  ],
  "unavailable": [
    "exa",
    "ag",
    "batcat",
    "rip"
  ],
  "version": 1
}
```

### 5.9 CLI Integration

```
❯ qai --help
...

TOOLS:
  ✅ fzf        0.54.0    (multi-select enabled)

DISCOVERED TOOLS:
  Modern alternatives: eza, rg, fd, bat, jq
  (Run 'qai tools --refresh' to re-scan)
```

New subcommand:

```
❯ qai tools
Discovered modern tools:
  ✅ eza    - modern ls replacement
  ✅ rg     - ripgrep (fast grep)
  ✅ fd     - fast find
  ✅ bat    - cat with syntax highlighting
  ✅ jq     - JSON processor
  ❌ exa    - not found
  ❌ ag     - not found

❯ qai tools --refresh
Clearing tool cache...
Tools will be re-discovered on next query.

❯ qai tools --clear
Cache cleared.
```

### 5.10 Configuration

```yaml
# ~/.config/qai/qai.yml

# Tool discovery settings
tools:
  # Enable/disable tool validation
  validate_commands: true

  # Cache TTL in hours (0 = never expire)
  cache_ttl: 0

  # Additional tools to always check for
  extra_modern_tools:
    - "custom-tool"
    - "my-script"

  # Tools to never suggest (even if installed)
  blocklist:
    - "rm -rf"  # dangerous patterns
```

---

## Implementation Phases

### Phase 1: Core AI Mode
- [ ] ZLE widget for `ai<TAB>` mode entry
- [ ] Mode state management (enter/exit)
- [ ] Visual prompt change for AI mode
- [ ] **API key validation on mode entry**:
  - [ ] `validate-api` subcommand that calls `GET /v1/models`
  - [ ] No token consumption, no inference
  - [ ] Handle 200 OK (valid), 401/403 (invalid/revoked)
  - [ ] Block mode entry if validation fails
  - [ ] Show clear error message (invalid key vs not configured vs network)
- [ ] `--multi` flag for qai CLI
- [ ] Multi-result system prompt
- [ ] fzf integration for selection
- [ ] Fallback to single-result when fzf unavailable
- [ ] CLI help enhancements:
  - [ ] `ToolStatus` for fzf detection with version
  - [ ] API key status check
  - [ ] Log path display in --help
  - [ ] Dynamic `after_help` in clap
  - [ ] Daemon status check (when enabled)
- [ ] Tests for ZLE logic
- [ ] Tests for multi-result parsing

### Phase 2: History & Learning
- [ ] Define data models (QueryRecord, QueryPattern)
- [ ] Implement SQLite storage (or flat file alternative)
- [ ] Record queries and results
- [ ] Record selections and corrections
- [ ] Implement pattern aggregation
- [ ] Personalized result ranking
- [ ] `qai history` command
- [ ] Tests for history storage
- [ ] Tests for ranking algorithm

### Phase 3: Daemon Mode (Optional)
- [ ] Define IPC protocol
- [ ] Implement Unix socket server
- [ ] Client with fallback to direct mode
- [ ] Connection pooling for API
- [ ] Response caching
- [ ] Token streaming (if supported)
- [ ] Service management (systemd/launchd)
- [ ] Version compatibility checking
- [ ] Tests for daemon lifecycle
- [ ] Tests for client-daemon communication

### Phase 4: Context Awareness
- [ ] Context provider interface
- [ ] Git context provider
- [ ] Directory context provider
- [ ] Recent commands context (shell history)
- [ ] Dynamic prompt building
- [ ] Tests for context providers

### Phase 5: Tool Discovery & Validation
- [ ] Dual-list response format (MODERN + STANDARD sections)
- [ ] Response parser for dual-list format
- [ ] `ToolCache` struct with available/unavailable sets
- [ ] Binary extraction from command strings
- [ ] PATH validation via `which` crate
- [ ] Cache persistence (~/.cache/qai/tools.json)
- [ ] Command filtering based on availability
- [ ] Prompt enhancement with known tools
- [ ] `qai tools` subcommand (list, refresh, clear)
- [ ] Configuration options (blocklist, extra tools)
- [ ] Tests for response parsing
- [ ] Tests for binary validation
- [ ] Tests for cache persistence

---

## Configuration

### Config File Structure

```yaml
# ~/.config/qai/qai.yml

# API Configuration
api_key: ${OPENAI_API_KEY}  # Or set directly
model: gpt-4o-mini
model_smart: gpt-4o  # For complex queries

# Multi-result settings
multi_result_count: 5
use_fzf: true  # Set to false to always use single-result

# History settings
history_enabled: true
history_max_entries: 10000
learn_from_corrections: true

# Daemon settings (optional feature)
daemon_enabled: false
daemon_socket: ~/.local/share/qai/daemon.sock

# Context settings
context_providers:
  - git
  - directory
  - recent_commands
context_max_length: 500  # Max chars for context
```

---

## CLI Help Output

Following the patterns from `aka` and `gx`, the qai CLI should display helpful status information in `--help` output.

### Expected Output

```
❯ qai --help
qai - Natural language to shell commands

Usage: qai [OPTIONS] [COMMAND]

Commands:
  query       Convert natural language to shell command(s)
  shell-init  Output shell integration script
  history     Show query history
  daemon      Manage qai daemon (optional)
  help        Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version

TOOLS:
  ✅ fzf        0.54.0    (multi-select enabled)

API:
  ✅ OPENAI_API_KEY configured

Logs are written to: ~/.local/share/qai/logs/qai.log

Daemon status: ✅ running (pid: 12345, uptime: 2h 34m)
```

### Status Indicators

#### Tool Status

```rust
pub struct ToolStatus {
    pub name: &'static str,
    pub required: bool,
    pub version: Option<String>,
    pub available: bool,
    pub note: Option<String>,
}

impl ToolStatus {
    pub fn check_fzf() -> Self {
        let output = Command::new("fzf").arg("--version").output();

        match output {
            Ok(out) if out.status.success() => {
                let version = String::from_utf8_lossy(&out.stdout)
                    .split_whitespace()
                    .next()
                    .unwrap_or("unknown")
                    .to_string();

                ToolStatus {
                    name: "fzf",
                    required: false,
                    version: Some(version),
                    available: true,
                    note: Some("multi-select enabled".to_string()),
                }
            }
            _ => ToolStatus {
                name: "fzf",
                required: false,
                version: None,
                available: false,
                note: Some("single-result mode".to_string()),
            },
        }
    }

    pub fn display(&self) -> String {
        let icon = if self.available { "✅" } else { "⚠️" };
        let version = self.version.as_deref().unwrap_or("not found");
        let note = self.note.as_ref()
            .map(|n| format!("({})", n))
            .unwrap_or_default();

        format!("  {} {:<10} {:<10} {}", icon, self.name, version, note)
    }
}
```

#### API Key Status

```rust
pub fn check_api_key_status() -> String {
    if std::env::var("OPENAI_API_KEY").is_ok() {
        "  ✅ OPENAI_API_KEY configured".to_string()
    } else if config_has_api_key() {
        "  ✅ API key in config file".to_string()
    } else {
        "  ❌ OPENAI_API_KEY not set".to_string()
    }
}
```

#### Daemon Status

```rust
pub fn check_daemon_status() -> String {
    match DaemonClient::new().health() {
        Ok(health) => {
            format!(
                "Daemon status: ✅ running (pid: {}, uptime: {})",
                health.pid,
                format_duration(health.uptime)
            )
        }
        Err(_) => {
            "Daemon status: ⚪ not running".to_string()
        }
    }
}
```

### Implementation

Using clap's `after_help` to append status information:

```rust
use clap::{CommandFactory, Parser};

#[derive(Parser)]
#[command(
    name = "qai",
    about = "Natural language to shell commands",
    after_help = "" // Set dynamically
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

fn main() {
    // Build dynamic after_help
    let after_help = build_status_footer();

    // Create command with dynamic help
    let matches = Cli::command()
        .after_help(&after_help)
        .get_matches();

    // ... rest of main
}

fn build_status_footer() -> String {
    let mut lines = Vec::new();

    // Tools section
    lines.push("TOOLS:".to_string());
    lines.push(ToolStatus::check_fzf().display());
    lines.push(String::new());

    // API section
    lines.push("API:".to_string());
    lines.push(check_api_key_status());
    lines.push(String::new());

    // Log location
    let log_path = get_log_path();
    lines.push(format!("Logs are written to: {}", log_path.display()));
    lines.push(String::new());

    // Daemon status (only if daemon feature enabled)
    #[cfg(feature = "daemon")]
    {
        lines.push(check_daemon_status());
    }

    lines.join("\n")
}
```

### Log Location

```rust
pub fn get_log_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("qai/logs/qai.log")
}

pub fn setup_logging() -> Result<()> {
    let log_path = get_log_path();

    // Ensure directory exists
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Configure logging
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    env_logger::Builder::from_env(
        Env::default().default_filter_or("info")
    )
    .target(Target::Pipe(Box::new(file)))
    .init();

    Ok(())
}
```

### Variations Based on Status

#### Everything OK

```
TOOLS:
  ✅ fzf        0.54.0    (multi-select enabled)

API:
  ✅ OPENAI_API_KEY configured

Logs are written to: ~/.local/share/qai/logs/qai.log

Daemon status: ✅ running (pid: 12345, uptime: 2h 34m)
```

#### fzf Missing (Graceful)

```
TOOLS:
  ⚠️  fzf        not found (single-result mode)

API:
  ✅ OPENAI_API_KEY configured

Logs are written to: ~/.local/share/qai/logs/qai.log

Daemon status: ⚪ not running
```

#### API Key Missing (Error)

```
TOOLS:
  ✅ fzf        0.54.0    (multi-select enabled)

API:
  ❌ OPENAI_API_KEY not set

Logs are written to: ~/.local/share/qai/logs/qai.log

Daemon status: ⚪ not running
```

---

## File Structure

```
qai/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── cli.rs               # Clap command definitions
│   ├── api.rs               # OpenAI API client
│   ├── config.rs            # Configuration loading
│   ├── shell.rs             # Shell integration scripts
│   ├── prompt.rs            # Prompt template loading
│   ├── history.rs           # History storage & learning
│   ├── context.rs           # Context providers
│   ├── protocol.rs          # IPC protocol (if daemon)
│   └── bin/
│       └── qai-daemon.rs    # Daemon binary (optional)
├── prompts/
│   └── system.pmt           # System prompt template
├── docs/
│   ├── architecture.md
│   └── ai-mode-design.md    # This document
└── tests/
    ├── integration/
    │   ├── shell_integration_test.rs
    │   ├── history_test.rs
    │   └── daemon_test.rs
    └── fixtures/
        └── ...
```

---

## Testing Strategy

### Unit Tests
- Shell script syntax validation
- ZLE widget logic (mode state, buffer handling)
- API response parsing
- History storage operations
- Result ranking algorithm
- Context provider output

### Integration Tests
- End-to-end query flow (mock API)
- fzf integration
- History recording and retrieval
- Daemon client-server communication
- Fallback behavior when daemon down

### Manual Testing Scenarios
1. `ai<TAB>` enters mode, prompt changes
2. Type query, press Enter, see fzf picker
3. Select command, it appears in BUFFER
4. `Ctrl+C` exits mode cleanly
5. Normal tab completion unaffected
6. Works without fzf (single result mode)
7. History shows past queries
8. Corrections improve future results

---

## Security Considerations

### API Key Management
- Read from environment variable (preferred)
- Read from config file (with warning about permissions)
- Never log or store API key in history

### History Privacy
- History stored locally only
- Option to disable history
- Easy purge command: `qai history --clear`

### Daemon Socket Security
- Unix socket with 0600 permissions
- Located in user-only directory
- No network exposure

---

## Success Metrics

### User Experience
- [ ] Mode transition feels instant (<50ms)
- [ ] fzf picker appears within 2s of Enter
- [ ] Normal shell operations unaffected
- [ ] Learning improves results over time

### Technical
- [ ] Test coverage >80%
- [ ] No regressions in existing functionality
- [ ] Daemon mode optional, not required
- [ ] Graceful degradation at every level

---

## References

- [aka daemon architecture](/home/saidler/repos/scottidler/aka/docs/daemon-architecture.md)
- [fzf](https://github.com/junegunn/fzf)
- [OpenAI API](https://platform.openai.com/docs/api-reference)

