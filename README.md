# QAI - Pure Zsh Implementation

Natural language to shell commands using OpenAI, implemented entirely in **zsh + jq**.

## Quick Start

```zsh
# Set your API key
export OPENAI_API_KEY="your-key-here"

# Run directly
./qai.zsh list files by size
# → ls -lhS

# Or add to PATH and use as 'qai'
ln -s $(pwd)/qai.zsh ~/.local/bin/qai
qai find python files modified today
# → find . -name "*.py" -mtime -1
```

## Shell Integration (Tab Expansion)

Add to your `~/.zshrc`:

```zsh
source /path/to/qai-init.zsh
```

Then use Tab to expand:

```
qai list files by size<Tab>
# Expands to: ls -lhS
```

**Important:** Normal tab completion continues to work for all other commands.

## Files

| File | Purpose |
|------|---------|
| `qai.zsh` | Main script - API calls, CLI interface |
| `qai-init.zsh` | ZLE integration for Tab expansion |
| `tests/test_qai.zsh` | Test suite (34 tests) |

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `OPENAI_API_KEY` | Yes | - | Your OpenAI API key |
| `QAI_MODEL` | No | `gpt-4o-mini` | Model to use |
| `QAI_CONFIG_FILE` | No | `~/.config/qai/qai.yml` | Config file path |

## Running Tests

```zsh
zsh tests/test_qai.zsh
```

---

# Comparison: Zsh vs Rust Implementation

## Lines of Code

| Component | Zsh | Rust |
|-----------|-----|------|
| Main script/binary | ~200 | ~800 (across 6 files) |
| Shell integration | ~70 | ~55 (embedded in binary) |
| Tests | ~350 | ~400 |
| **Total** | **~620** | **~1255** |

## Dependencies

### Zsh Implementation
- `zsh` (shell)
- `jq` (JSON parsing)
- `curl` (HTTP requests)

### Rust Implementation
- 80+ crates (reqwest, tokio, clap, serde, etc.)
- Compiles to ~5MB binary

## Features Comparison

| Feature | Zsh | Rust |
|---------|-----|------|
| Natural language → command | ✓ | ✓ |
| Tab expansion | ✓ | ✓ |
| Config file support | ✓ (simple) | ✓ (YAML parsing) |
| Error handling | Basic | Comprehensive |
| Async/concurrent | ✗ | ✓ |
| Cross-platform binary | ✗ | ✓ |
| Startup time | ~10ms | ~5ms |
| API call overhead | Same | Same |

## Pros/Cons

### Zsh Implementation

**Pros:**
- No compilation needed
- Easy to modify/debug
- Minimal dependencies (jq, curl)
- Transparent - you can read exactly what it does
- ~3x less code

**Cons:**
- Zsh-only (no bash/fish support planned)
- Config parsing is basic (no full YAML)
- Error messages less detailed
- No type safety

### Rust Implementation

**Pros:**
- Type-safe, catch errors at compile time
- Better error handling with `eyre`
- Single binary distribution
- Async HTTP (better for future features)
- More structured codebase

**Cons:**
- Requires Rust toolchain to modify
- Compile times
- ~3x more code
- Dependencies need security audits

## When to Use Which

**Use Zsh version if:**
- You want to understand/modify the code
- You only need basic functionality
- You want minimal dependencies
- You're prototyping

**Use Rust version if:**
- You want a polished, distributable tool
- You need robust error handling
- You plan to add complex features
- You prefer compiled binaries

## The Tab Completion Bug Fix

Both versions now correctly handle tab completion for non-qai commands.

**The bug:** Original code tried to parse `bindkey '^I'` output incorrectly:
```zsh
# BROKEN - [2] gets second CHARACTER, not word
_qai_original_tab_widget="${$(bindkey '^I')[2]:-expand-or-complete}"
```

**The fix:** Properly extract the widget name:
```zsh
# CORRECT - Parse the output format: "^I" widget-name
_qai_tab_binding="$(bindkey '^I' 2>/dev/null)"
if [[ "$_qai_tab_binding" == *'" '* ]]; then
    _qai_original_tab_widget="${_qai_tab_binding##*\" }"
fi
```

**Test coverage:** Both versions now have explicit tests verifying that non-qai commands (like `eza -halo --git .confi<Tab>`) use the original tab widget.

