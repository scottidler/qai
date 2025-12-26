# Configurable Bindings Design

**Version:** 1.0
**Status:** Proposed
**Date:** 2025-12-25

---

## Problem Statement

The Tab key is currently hardcoded as the trigger for entering "AI mode" in qai. This conflicts with other zsh plugins that also bind Tab (e.g., fzf-tab, zsh-autosuggestions with Tab accept).

Users need the ability to configure which key triggers AI mode.

---

## Current Implementation

### Location of Hardcoded Binding

The Tab binding is in `src/shell.rs` as a static constant:

```rust
pub const ZSH_INIT_SCRIPT: &str = r#"
// ...
bindkey '^I' _qai_tab_handler  # Tab
bindkey '^M' _qai_submit       # Enter
"#;
```

### Current Trigger Logic

1. User types `ai`
2. User presses Tab (`^I`)
3. `_qai_tab_handler` checks if buffer equals "ai"
4. If yes, enters AI mode; if no, falls through to normal completion

---

## Proposed Solution

### 1. Config File Extension

Add a `bindings` section to `~/.config/qai/qai.yml`:

```yaml
# ~/.config/qai/qai.yml
api_key: "sk-..."
model: "gpt-4o-mini"
api_base: "https://api.openai.com/v1"
debug: false

# bindings configuration
bindings:
  # Key to trigger AI mode when buffer is "ai"
  # Default: "tab"
  trigger: "ctrl-space"
```

### 2. Key Name to Escape Sequence Mapping

Create a mapping from human-readable key names to zsh `bindkey` escape sequences. Users should never need to know escape sequences.

#### Supported Key Names

| Category | Key Names | ZSH Sequence | Notes |
|----------|-----------|--------------|-------|
| **Tab/Enter** | `tab` | `^I` | Default trigger |
| | `enter`, `return` | `^M` | Used for submit |
| **Escape** | `escape`, `esc` | `^[` | |
| **Backspace** | `backspace` | `^?` | |
| **Ctrl+Letter** | `ctrl-a` | `^A` | |
| | `ctrl-b` | `^B` | |
| | `ctrl-c` | `^C` | ⚠️ Usually interrupt |
| | `ctrl-d` | `^D` | ⚠️ Usually EOF |
| | `ctrl-e` | `^E` | |
| | `ctrl-f` | `^F` | |
| | `ctrl-g` | `^G` | Often unused |
| | `ctrl-h` | `^H` | Alt backspace |
| | `ctrl-i` | `^I` | Same as Tab |
| | `ctrl-j` | `^J` | Newline |
| | `ctrl-k` | `^K` | |
| | `ctrl-l` | `^L` | Usually clear |
| | `ctrl-m` | `^M` | Same as Enter |
| | `ctrl-n` | `^N` | |
| | `ctrl-o` | `^O` | |
| | `ctrl-p` | `^P` | |
| | `ctrl-q` | `^Q` | ⚠️ XON flow control |
| | `ctrl-r` | `^R` | ⚠️ Usually reverse search |
| | `ctrl-s` | `^S` | ⚠️ XOFF flow control |
| | `ctrl-t` | `^T` | |
| | `ctrl-u` | `^U` | |
| | `ctrl-v` | `^V` | |
| | `ctrl-w` | `^W` | |
| | `ctrl-x` | `^X` | |
| | `ctrl-y` | `^Y` | |
| | `ctrl-z` | `^Z` | ⚠️ Usually suspend |
| **Ctrl+Special** | `ctrl-space` | `^@` | ✅ Good choice |
| | `ctrl-backslash` | `^\` | |
| | `ctrl-]` | `^]` | |
| | `ctrl-^` | `^^` | |
| | `ctrl-_` | `^_` | |
| **Function Keys** | `f1` | `^[OP` | |
| | `f2` | `^[OQ` | |
| | `f3` | `^[OR` | |
| | `f4` | `^[OS` | |
| | `f5` | `^[[15~` | |
| | `f6` | `^[[17~` | |
| | `f7` | `^[[18~` | |
| | `f8` | `^[[19~` | |
| | `f9` | `^[[20~` | |
| | `f10` | `^[[21~` | |
| | `f11` | `^[[23~` | |
| | `f12` | `^[[24~` | |
| **Arrow Keys** | `up` | `^[[A` | |
| | `down` | `^[[B` | |
| | `right` | `^[[C` | |
| | `left` | `^[[D` | |
| **Navigation** | `home` | `^[[H` | |
| | `end` | `^[[F` | |
| | `insert` | `^[[2~` | |
| | `delete` | `^[[3~` | |
| | `page-up`, `pageup` | `^[[5~` | |
| | `page-down`, `pagedown` | `^[[6~` | |

#### Recommended Alternatives to Tab

If Tab conflicts with another plugin, these are good alternatives:

1. **`ctrl-space`** - Very common for special actions, rarely conflicts
2. **`ctrl-g`** - Traditionally "cancel" in Emacs, often unused in zsh
3. **`ctrl-]`** - Rarely used
4. **`f1`** - Function keys are usually available

---

## Implementation Plan

### Phase 1: Config Extension

**File: `src/config.rs`**

Add bindings configuration:

```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(default)]
pub struct BindingsConfig {
    /// Key to trigger AI mode (when buffer is "ai")
    /// Examples: "tab", "ctrl-space", "ctrl-a", "f1"
    pub trigger: String,
}

impl Default for BindingsConfig {

    fn default() -> Self {
        Self {
            trigger: "tab".to_string(),
        }
    }
}
```

Update main `Config` struct:

```rust
#[derive(Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub api_key: Option<String>,
    pub model: String,
    pub api_base: String,
    pub debug: bool,
    pub bindings: BindingsConfig,  // NEW
}
```

### Phase 2: Key Mapping Module

**File: `src/bindings.rs`** (new file)

```rust
use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Mapping from friendly key names to zsh bindkey sequences
static KEY_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
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
    // ... all letters ...
    m.insert("ctrl-z", "^Z");

    // Ctrl + special
    m.insert("ctrl-space", "^@");
    m.insert("ctrl-backslash", "^\\");
    m.insert("ctrl-]", "^]");

    // Function keys
    m.insert("f1", "^[OP");
    m.insert("f2", "^[OQ");
    // ... f3-f12 ...

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
pub fn key_name_to_sequence(name: &str) -> Result<&'static str, String> {
    let normalized = name.to_lowercase().replace(' ', "-");

    KEY_MAP.get(normalized.as_str()).copied().ok_or_else(|| {
        let valid_keys: Vec<_> = KEY_MAP.keys().copied().collect();
        format!(
            "Unknown key '{}'. Valid keys: {}",
            name,
            valid_keys.join(", ")
        )
    })
}

/// Get all valid key names (for documentation/help)
pub fn valid_key_names() -> Vec<&'static str> {
    let mut keys: Vec<_> = KEY_MAP.keys().copied().collect();
    keys.sort();
    keys
}
```

### Phase 3: Dynamic Shell Script Generation

**File: `src/shell.rs`**

Convert the static `ZSH_INIT_SCRIPT` to a function that accepts the configured key:

```rust
use crate::bindings::key_name_to_sequence;
use crate::config::Config;

/// Generate ZSH init script with configurable bindings
pub fn generate_zsh_init_script(config: &Config) -> Result<String, String> {
    let trigger_sequence = key_name_to_sequence(&config.bindings.trigger)?;

    Ok(format!(r#"
# qai - Natural language to shell commands via AI
# Add to your .zshrc: eval "$(qai shell-init zsh)"

# State variable: are we in AI mode?
_qai_in_ai_mode=0
_qai_saved_prompt=""
_qai_ai_prompt="🤖 ai> "

# Store original binding for trigger key
_qai_original_trigger_widget=""
if (( ${{+widgets[expand-or-complete]}} )); then
    _qai_original_trigger_widget="expand-or-complete"
fi
_qai_trigger_binding="$(bindkey '{trigger}' 2>/dev/null)"
if [[ "$_qai_trigger_binding" == *'" '* ]]; then
    _qai_original_trigger_widget="${{_qai_trigger_binding##*\" }}"
fi
[[ -z "$_qai_original_trigger_widget" ]] && _qai_original_trigger_widget="expand-or-complete"
unset _qai_trigger_binding

# Trigger key handler - dispatch based on buffer content and mode
_qai_trigger_handler() {{
    if [[ "$BUFFER" == "ai" && $_qai_in_ai_mode -eq 0 ]]; then
        _qai_start
    else
        # Normal completion
        zle "${{_qai_original_trigger_widget:-expand-or-complete}}"
    fi
}}

# ... rest of the script (unchanged) ...

# Register widgets
zle -N _qai_trigger_handler
zle -N _qai_start
zle -N _qai_exit
zle -N _qai_submit

# Bind keys
bindkey '{trigger}' _qai_trigger_handler  # Configured trigger key
bindkey '^M' _qai_submit                   # Enter
"#, trigger = trigger_sequence))
}

/// Generate shell init script for the specified shell
pub fn generate_init_script(shell: &str, config: &Config) -> Option<Result<String, String>> {
    match shell.to_lowercase().as_str() {
        "zsh" => Some(generate_zsh_init_script(config)),
        _ => None,
    }
}
```

### Phase 4: Update shell-init Command

**File: `src/main.rs`**

Update `handle_shell_init` to load config and pass to script generation:

```rust
fn handle_shell_init(shell: &str, config: &Config) -> Result<()> {
    match shell::generate_init_script(shell, config) {
        Some(Ok(script)) => {
            print!("{}", script);
            Ok(())
        }
        Some(Err(e)) => {
            Err(eyre::eyre!("Invalid binding configuration: {}", e))
        }
        None => {
            let supported = shell::supported_shells().join(", ");
            Err(eyre::eyre!(
                "Unsupported shell: '{}'. Supported shells: {}",
                shell,
                supported
            ))
        }
    }
}
```

---

## File Changes Summary

| File | Change Type | Description |
|------|-------------|-------------|
| `src/config.rs` | Modify | Add `NindingsConfig` struct |
| `src/bindings.rs` | **New** | Key name → sequence mapping |
| `src/shell.rs` | Modify | Convert static script to template function |
| `src/main.rs` | Modify | Pass config to shell-init, add `mod keys` |
| `qai.yml` | Modify | Add bindings example |

---

## Example Usage

### Default (Tab)

```yaml
# No bindings section needed, defaults to tab
model: "gpt-4o-mini"
```

User types `ai<Tab>` → enters AI mode.

### Custom (Ctrl+Space)

```yaml
model: "gpt-4o-mini"
bindings:
  trigger: "ctrl-space"
```

User types `ai<Ctrl+Space>` → enters AI mode. Tab works normally.

### Custom (F1)

```yaml
model: "gpt-4o-mini"
bindings:
  trigger: "f1"
```

User types `ai<F1>` → enters AI mode.

---

## Error Handling

### Invalid Key Name

```
$ qai shell-init zsh
Error: Invalid binding configuration: Unknown key 'ctrl-foo'.
Valid keys: backspace, ctrl-a, ctrl-b, ..., tab, up
```

### Missing Config (Graceful Default)

If no config file exists or bindings section is missing, defaults to `tab`.

---

## Backwards Compatibility

- Default behavior unchanged (Tab triggers AI mode)
- Users who don't add bindings section see no difference
- Users must re-source their `.zshrc` after changing config

---

## Testing Plan

1. **Unit Tests**
   - Key name mapping (all valid names)
   - Case insensitivity (`Tab`, `TAB`, `tab`)
   - Invalid key name error message

2. **Integration Tests**
   - Script generation with default config
   - Script generation with custom trigger
   - Config loading with bindings section

3. **Manual Tests**
   - Tab trigger (default)
   - Ctrl+Space trigger
   - F1 trigger
   - Verify original key binding is preserved for fallback

---

## Future Considerations

- **Multiple triggers**: Allow array of triggers (e.g., `["tab", "ctrl-space"]`)
- **Per-shell config**: Different bindings for zsh vs bash (when bash is supported)
- **Key combinations**: Support for chords like `ctrl-x ctrl-a`
