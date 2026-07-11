# 🦀 Tua Agent RS — Comprehensive Guide

> **Version:** 0.3.0 | **Rust-native AI coding agent**  
> 14 tools · 8 profiles · self-correction · checkpointing · review · TUI

---

## Table of Contents

- [Quickstart](#quickstart)
- [Installation](#installation)
- [Features](#features)
  - [Feature 1: 14 Built-in Rust Tools](#feature-1-14-built-in-rust-tools)
  - [Feature 2: 8 Rust Coding Profiles](#feature-2-8-rust-coding-profiles)
  - [Feature 3: Self-Correction (cargo check)](#feature-3-self-correction-cargo-check)
  - [Feature 4: Git Checkpointing](#feature-4-git-checkpointing)
  - [Feature 5: Multi-Agent Review (clippy)](#feature-5-multi-agent-review-clippy)
  - [Feature 6: Prompt Caching](#feature-6-prompt-caching)
  - [Feature 7: OpenAI-compatible Provider](#feature-7-openai-compatible-provider)
  - [Feature 8: Streaming Chat Events](#feature-8-streaming-chat-events)
  - [Feature 9: TUI (Terminal User Interface)](#feature-9-tui-terminal-user-interface)
  - [Feature 10: Web Dashboard](#feature-10-web-dashboard)
  - [Feature 11: Skills System](#feature-11-skills-system)
  - [Feature 12: Built-in System Prompt](#feature-12-built-in-system-prompt)
  - [Feature 13: Config Loading](#feature-13-config-loading)
  - [Feature 14: Error Handling with thiserror](#feature-14-error-handling-with-thiserror)
  - [Feature 15: Agent Loop Harness](#feature-15-agent-loop-harness)
  - [Feature 16: SSE Stream Parsing](#feature-16-sse-stream-parsing)
  - [Feature 17: Command Palette (TUI)](#feature-17-command-palette-tui)
  - [Feature 18: Multi-Tab Chat (TUI)](#feature-18-multi-tab-chat-tui)
  - [Feature 19: Self-Correction with Rust Edits Detection](#feature-19-self-correction-with-rust-edits-detection)
- [Configuration Reference](#configuration-reference)
- [CLI Flags Reference](#cli-flags-reference)
- [Profiles Table](#profiles-table)
- [Tools Table](#tools-table)
- [Architecture Overview](#architecture-overview)
- [Known Limitations](#known-limitations)

---

## Quickstart

### 1. Run the agent with a prompt

```bash
# Basic usage
tua-rs -p "Create a new Rust module with a generic Stack type"

# With a specific profile
tua-rs --profile rustacean -p "Write a zero-cost iterator adapter"

# With a custom model
tua-rs -m "gpt-4o" -p "Explain lifetimes to me"
```

### 2. List available profiles

```bash
tua-rs profiles
```

Output:
```
🦀 Tua Agent RS v0.3.0 — 8 Rust Coding Profiles

  🚀 rustacean       — Idiomatic, performant Rust engineer
  🦀 ferris          — Friendly, beginner-friendly Rust
  🔍 borrow-checker  — Strict lifetime auditing
  📦 cargo-cult      — Dependency-smart, ecosystem-aware
  🔓 unsafe-ferris   — Unsafe Rust specialist
  🧪 test-crab       — Testing-focused, thorough coverage
  📖 doc-crab        — Documentation-focused, educational
  🛡️ strict          — All guardrails enabled
```

### 3. Check the current configuration

```bash
tua-rs config
```

### 4. Run the TUI

```bash
tua-rs tui
```

### 5. Launch the web dashboard

```bash
tua-rs dashboard
```

Then open `http://0.0.0.0:8765` in your browser.

### 6. Run a code review

```bash
tua-rs review --cwd /path/to/project
```

---

## Installation

### From source

```bash
git clone <repo-url>
cd tua-rs
cargo build --release
./target/release/tua-rs --help
```

### Requirements

- Rust 1.75+ (edition 2021)
- `git` (for checkpointing features)
- `rustc` + `cargo` (for tool execution)
- `clippy` (for review feature: `rustup component add clippy`)

### Optional dependencies

| Tool | Feature | Install |
|------|---------|---------|
| `wasm-pack` | WebAssembly tool | `cargo install wasm-pack` |
| `cargo-audit` | Security auditing | `cargo install cargo-audit` |
| `cargo-outdated` | Dependency updates | `cargo install cargo-outdated` |
| `cargo-udeps` | Unused dep detection | `cargo install cargo-udeps` |
| `cargo-deny` | License/advisory checks | `cargo install cargo-deny` |
| `cargo-criterion` | Benchmarks | `cargo install cargo-criterion` |

---

## Features

### Feature 1: 14 Built-in Rust Tools

The agent comes with 14 pre-registered tools that mirror the Python `tua-agent` tool set.
Each tool is defined with a name, description, JSON input schema, and an async executor.

**Usage in code:**

```rust
use tua_rs::tools::rust_tools;

let tools = rust_tools();
assert_eq!(tools.len(), 14);

// Find a tool by name
let cargo_tool = tools.iter().find(|t| t.name == "cargo").unwrap();
let result = cargo_tool
    .execute(serde_json::json!({"subcommand": "check"}))
    .await?;
```

**Available tools:** See [Tools Table](#tools-table) below.

---

### Feature 2: 8 Rust Coding Profiles

Each profile configures guardrails for the LLM — what it's allowed or required to do.
Profiles are defined at compile time in `src/profiles.rs`.

**Usage in code:**

```rust
use tua_rs::profiles::{get_profile, build_profile_context};

// Look up a profile (case-insensitive)
let profile = get_profile("rustacean").unwrap();
assert_eq!(profile.emoji, "🚀");

// Build a guardrail string for the system prompt
let context = build_profile_context(profile);
println!("{context}");
// Output includes:
//   ❌ .unwrap() / .expect() FORBIDDEN
//   ❌ unsafe code FORBIDDEN
//   ✅ Doc-tests REQUIRED on all public API
//   ✅ clippy::pedantic ENFORCED
```

**All 8 profiles are listed in the [Profiles Table](#profiles-table) below.**

---

### Feature 3: Self-Correction (cargo check)

After the agent edits `.rs` files, the harness can automatically run `cargo check`
and feed compiler errors back to the model as a user message, allowing it to fix
them.

**Configuration:**

```rust
use tua_rs::agent::{AgentConfig, AgentHarnessConfig};

let config = AgentConfig {
    max_tool_rounds: Some(50),
    harness_config: AgentHarnessConfig {
        self_correction: true,           // enable auto-correction
        max_self_corrections: 3,         // max fix attempts per round
    },
};
```

**Detection:** The harness scans tool results for keywords like `".rs"`, `"written"`,
`"modified"`, `"updated"`, and `"saved"` to determine whether a code edit occurred.

---

### Feature 4: Git Checkpointing

The agent can snapshot the working tree using git, then roll back changes if needed.

**Usage:**

```rust
use tua_rs::checkpoint::{checkpoint, rollback, last_commit_hash, is_git_repo};

if is_git_repo(None) {
    // Create a checkpoint
    match checkpoint("✨ agent checkpoint") {
        Ok(hash) => println!("Checkpoint created: {hash}"),
        Err(e) => eprintln!("Checkpoint failed: {e}"),
    }

    // Roll back the most recent checkpoint
    match rollback() {
        Ok(()) => println!("Rolled back to previous commit"),
        Err(e) => eprintln!("Rollback failed: {e}"),
    }

    // Get the current HEAD hash
    match last_commit_hash() {
        Ok(hash) => println!("HEAD is at {hash}"),
        Err(e) => eprintln!("Not a git repo: {e}"),
    }
}
```

**Error types** (using `thiserror`):

| Variant | Meaning |
|---------|---------|
| `CheckpointError::NotARepo` | Not in a git repository |
| `CheckpointError::StageFailed` | `git add -A` failed |
| `CheckpointError::CommitFailed` | `git commit` failed |
| `CheckpointError::RollbackFailed` | `git reset --hard HEAD~1` failed |
| `CheckpointError::HashFailed` | Could not retrieve commit hash |
| `CheckpointError::Io` | I/O error (spawn failure) |
| `CheckpointError::NoChanges` | Working tree is clean |

---

### Feature 5: Multi-Agent Review (clippy)

After code edits, the agent can run `cargo clippy --message-format=short` in the
background and parse structured findings (errors, warnings, info).

**Usage:**

```rust
use tua_rs::review::{review_edits, format_review, ReviewFinding};

// Run clippy on specific files
let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
match review_edits(&files, Some("/path/to/project")) {
    Ok(findings) => {
        println!("{}", format_review(&findings));
        for finding in &findings {
            println!("  {}:{} — {} [{}]", finding.file, finding.line, finding.message, finding.severity);
        }
    }
    Err(e) => eprintln!("Review failed: {e}"),
}
```

**Error types:**

| Variant | Meaning |
|---------|---------|
| `ReviewError::ClippyExecution` | `cargo clippy` could not be spawned |
| `ReviewError::InvalidUtf8` | Clippy output is not valid UTF-8 |
| `ReviewError::NoFiles` | No files specified for review |

---

### Feature 6: Prompt Caching

The configuration supports prompt caching (e.g., Anthropic-style). When enabled,
the agent may cache the system prompt between turns to reduce token usage and
latency.

**Configuration:**

```toml
# ~/.tua-rs/config.toml
prompt_caching = true
```

**Default:** `true`

---

### Feature 7: OpenAI-compatible Provider

The `OpenAiCompatibleProvider` works with any API that follows the OpenAI
`/chat/completions` streaming format — ChatGPT, DeepSeek, Qwen, Grok, Together AI,
and others.

**Usage:**

```rust
use tua_rs::providers::{OpenAiCompatibleProvider, ProviderConfig};
use std::sync::Arc;

let config = ProviderConfig {
    api_key: std::env::var("OPENAI_API_KEY")
        .expect("OPENAI_API_KEY must be set"),
    base_url: "https://api.openai.com/v1".to_string(),
    model: "gpt-4o".to_string(),
};

let provider = Arc::new(OpenAiCompatibleProvider::new(config)?);
```

**Key features:**
- Bearer token authentication with sensitive header marking
- 120-second request timeout
- SSE (Server-Sent Events) streaming
- Supports `reasoning_content` (DeepSeek, Qwen) and `thinking` (Anthropic compat)
- Tool-call delta accumulation across streaming chunks
- Malformed JSON chunks are skipped non-fatally

---

### Feature 8: Streaming Chat Events

The agent loop communicates through a typed event stream. Events are yielded
incrementally as the model generates text and calls tools.

**Usage:**

```rust
use futures::StreamExt;
use tua_rs::agent::{AgentEvent, AgentLoop};

let stream = agent_loop.run(messages);
let mut stream = std::pin::pin!(stream);

while let Some(event) = stream.next().await {
    match event {
        AgentEvent::TextDelta(chunk) => print!("{chunk}"),
        AgentEvent::ThinkingDelta(chunk) => print!("\x1b[90m{chunk}\x1b[0m"),
        AgentEvent::ToolCall(tc) => println!("\n🔧 Calling tool: {}", tc.name),
        AgentEvent::ToolResult { tool_call_id, output } => {
            println!("\n🔧 Result ({}): {}", tool_call_id, &output[..output.len().min(100)]);
        }
        AgentEvent::Error(e) => eprintln!("\n❌ Error: {e}"),
        AgentEvent::Done => println!("\n✅ Done"),
    }
}
```

**Event variants:**

| Variant | Description |
|---------|-------------|
| `TextDelta(String)` | A chunk of output text |
| `ThinkingDelta(String)` | Model's internal reasoning |
| `ToolCall(AgentToolCall)` | Model requests a tool invocation |
| `ToolResult { id, output }` | Result of executing a tool |
| `Error(String)` | Non-fatal error (loop may continue) |
| `Done` | Agent has finished processing |

---

### Feature 9: TUI (Terminal User Interface)

A full-featured terminal UI built with `ratatui` + `crossterm`. Supports multiple
chat tabs, a command palette, scrolling, and profile switching.

**Keybindings:**

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close active tab |
| `Ctrl+P` | Toggle command palette |
| `Ctrl+C` | Quit |
| `Esc` | Close palette |
| `Tab` / `Shift+Tab` | Next/previous tab |
| `PgUp` / `PgDn` | Scroll chat |
| `End` | Scroll to bottom |

**Command palette commands:**

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/profile` | Show active profile |
| `/model` | Show current model |
| `/tools` | List registered tools |
| `/skills` | List available skills |
| `/config` | Show configuration info |
| `/diff` | Show diff information |
| `/permissions` | Show permission mode |
| `/sessions` | Show active sessions |
| `/rollback` | Show checkpoint info |
| `/undo` | Show undo info |
| `/clear` | Clear chat history |

**Usage in code:**

```rust
use tua_rs::tui::{App, run_tui};

let mut app = App::new();
if let Err(e) = run_tui(&mut app) {
    eprintln!("TUI error: {e}");
}
```

---

### Feature 10: Web Dashboard

A dark-themed web dashboard serving project health metrics at `http://0.0.0.0:8765`.

**Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | HTML dashboard page |
| `/api/health` | GET | `{"status": "ok"}` |
| `/api/status` | GET | Full JSON project status |

**Status JSON includes:**

```json
{
  "project": {
    "name": "tua-rs",
    "version": "0.3.0",
    "profile": "🚀 rustacean",
    "rust_version": "rustc 1.85.0",
    "git_branch": "main",
    "git_commit": "abc123...",
    "git_date": "2025-01-15",
    "lines_of_code": 5142,
    "workspace_members": ["tua-rs"]
  },
  "build": {
    "last_check": "0d 12:34:56 UTC",
    "success": true,
    "error_count": 0,
    "warning_count": 0
  },
  "quality": {
    "clippy_warnings": 3,
    "clippy_errors": 0,
    "total_files": 14,
    "test_count": 120,
    "doc_test_count": 25
  },
  "tools": [
    { "name": "cargo", "description": "Run cargo commands..." },
    ...
  ]
}
```

**Usage:**

```bash
tua-rs dashboard
# Open http://0.0.0.0:8765
```

---

### Feature 11: Skills System

10 built-in Rust skills with detailed markdown references. Skills can be looked up
individually or formatted for inclusion in the system prompt.

**Usage:**

```rust
use tua_rs::skills::{get_skill, list_skills, format_skills_for_prompt};

// Look up a specific skill
let skill = get_skill("ownership-borrowing").unwrap();
assert!(skill.content.len() > 200);

// List all 10 skills
let all = list_skills();
assert_eq!(all.len(), 10);

// Format all skills as markdown for a system prompt
let prompt = format_skills_for_prompt();
println!("{prompt}");
```

**Available skills:**

1. `ownership-borrowing` — Ownership, borrowing, move semantics, borrow checker
2. `lifetimes` — Lifetime elision, annotations, `'static`, HRTB
3. `error-handling` — Result, Option, `?`, thiserror, anyhow
4. `async-rust` — async/await, Future, tokio, Stream, Pin
5. `macros` — macro_rules!, derive macros, proc macros
6. `testing` — Unit tests, integration tests, doc-tests, proptest
7. `smart-pointers` — Box, Rc, Arc, Cow, RefCell, Cell, Mutex, RwLock
8. `concurrency` — Send/Sync, Arc, Mutex, channels, Rayon, atomics
9. `cargo-workspace` — Workspace dependencies, features, patches
10. `wasm` — wasm-pack, wasm-bindgen, web-sys, js-sys

---

### Feature 12: Built-in System Prompt

A comprehensive Rust programming system prompt embedded at compile time, covering
the Rust mindset, ownership, lifetimes, traits, error handling, concurrency, async,
smart pointers, macros, unsafe Rust, Cargo ecosystem, testing, Clippy, security,
build performance, and API design.

**Usage:**

```rust
use tua_rs::prompts::build_rust_system_prompt;

let tools = vec!["cargo".to_string(), "clippy".to_string()];
let prompt = build_rust_system_prompt(&tools, "rustacean");
assert!(prompt.len() > 2000);
assert!(prompt.contains("Chain-of-Thought"));
assert!(prompt.contains("rustfmt REQUIRED"));
```

---

### Feature 13: Config Loading

Configuration loaded from `~/.tua-rs/config.toml`. Every field has a sensible default,
so the config file can be omitted entirely or contain only the fields you want to
override.

**Usage:**

```rust
use tua_rs::config::{load, TuaConfig, ConfigError};

match load() {
    Ok(cfg) => {
        println!("Default profile: {}", cfg.default_profile);
        println!("Self-correction: {}", cfg.self_correction);
    }
    Err(ConfigError::ReadFailed { path, source }) => {
        eprintln!("Cannot read {path}: {source}");
    }
    Err(ConfigError::ParseFailed { path, source }) => {
        eprintln!("Invalid config at {path}: {source}");
    }
}
```

**Error types:**

| Variant | Meaning |
|---------|---------|
| `ConfigError::ReadFailed` | Config file exists but is unreadable |
| `ConfigError::ParseFailed` | Invalid TOML content |

---

### Feature 14: Error Handling with thiserror

All modules use `thiserror` for typed, idiomatic error types with `Display` and
`Error` trait implementations.

**Available error types:**

| Module | Error Type | Variants |
|--------|-----------|----------|
| `agent` | `AgentError` | `UnknownTool`, `ToolExecution`, `MaxRoundsExceeded`, `StreamError`, `InvalidToolCall`, `ProviderInit` |
| `config` | `ConfigError` | `ReadFailed`, `ParseFailed` |
| `checkpoint` | `CheckpointError` | `NotARepo`, `StageFailed`, `CommitFailed`, `RollbackFailed`, `HashFailed`, `Io`, `NoChanges` |
| `review` | `ReviewError` | `ClippyExecution`, `InvalidUtf8`, `NoFiles` |

**Usage pattern:**

```rust
use tua_rs::agent::{AgentError, AgentResult};

fn my_function() -> AgentResult<String> {
    // Use ? for automatic error propagation
    let result = some_fallible_operation()?;
    Ok(result)
}
```

All `.unwrap()` and `.expect()` calls in production code have been replaced with
proper error propagation using the `?` operator and custom error types.

---

### Feature 15: Agent Loop Harness

A configurable orchestration harness that wraps a `ModelProvider`, system prompt,
and tool set. The harness:

1. Calls `provider.stream_chat()` with the conversation history
2. Forwards text/thinking deltas as events
3. Intercepts tool calls, looks up the tool, and executes it
4. Feeds tool results back into the conversation
5. Repeats until the model stops calling tools or the round limit is reached
6. Optionally runs `cargo check` for self-correction after code edits

**Usage:**

```rust
use std::sync::Arc;
use tua_rs::agent::{AgentLoop, AgentConfig, AgentHarnessConfig};

let loop_ = AgentLoop::with_config(
    provider,          // Arc<dyn ModelProvider>
    system_prompt,     // impl Into<String>
    tools,             // Vec<AgentTool>
    AgentConfig {
        max_tool_rounds: Some(50),
        harness_config: AgentHarnessConfig {
            self_correction: true,
            max_self_corrections: 3,
        },
    },
);

let stream = loop_.run(initial_messages);
// Consume the stream...
```

---

### Feature 16: SSE Stream Parsing

The `OpenAiCompatibleProvider` includes a full SSE (Server-Sent Events) parser
for the OpenAI `/chat/completions` streaming format.

**Features:**
- Line-delimited `data:` chunks
- `[DONE]` termination marker
- JSON parse errors are non-fatal (logged and skipped)
- Tool-call delta accumulation across multiple chunks
- Support for `reasoning_content` (DeepSeek, Qwen) and `thinking` fields
- Empty content deltas are skipped
- Heartbeat/comment lines (starting with `:`) are ignored

**Stream format handled:**

```text
data: {"choices":[{"index":0,"delta":{"content":"Hello"}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"get_weather","arguments":"{\"location\":"}}]}}]}

data: {"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"NYC\"}"}}]}}]}

data: [DONE]
```

---

### Feature 17: Command Palette (TUI)

The TUI includes a fuzzy-filter command palette with 12 built-in commands.
Access it with `Ctrl+P`.

**Filtering:** As you type, the palette filters commands by substring match
(case-insensitive). Navigate with `Up`/`Down` arrows and select with `Enter`.

**Usage:**

```rust
use tua_rs::tui::{CommandPalette, SLASH_COMMANDS};

let mut palette = CommandPalette::new();
palette.push_filter('/');
palette.push_filter('h');
assert_eq!(palette.filtered_commands(), vec!["/help"]);
```

---

### Feature 18: Multi-Tab Chat (TUI)

The TUI supports multiple concurrent conversation tabs, each with its own Rust
profile and message history.

**Features:**
- `Ctrl+T` to create a new tab
- `Ctrl+W` to close the active tab
- Tab bar shows profile emojis
- Each tab maintains its own scroll position
- Closing the last tab creates a fresh replacement
- Token count estimate per tab (1 token ≈ 4 characters)

**Usage in code:**

```rust
use tua_rs::tui::{App, Tab};
use tua_rs::profiles::get_profile;

let mut app = App::new();
app.add_tab();  // Creates "Chat 2" with the same profile
app.next_tab(); // Switch to next tab
app.close_tab(); // Close active tab
```

---

### Feature 19: Self-Correction with Rust Edits Detection

The harness intelligently detects when the agent has modified `.rs` files by
scanning tool output for:
- Paths ending in `.rs` combined with keywords: `written`, `wrote`, `modified`,
  `updated`, `saved`
- File-path-like patterns: `.rs\``, `.rs `, `.rs\n`, `.rs` at end of string

When a Rust edit is detected and self-correction is enabled, `cargo check` runs
automatically. If it fails, the compiler output is injected as a user message
for the model to fix.

**Detection logic:**

```rust
use tua_rs::agent::detect_rust_edits;

let messages = vec![
    AgentMessage::tool_result("call_1", "Written src/main.rs"),
];
assert!(detect_rust_edits(&messages));
```

---

## Configuration Reference

### `~/.tua-rs/config.toml`

All fields are optional. Defaults are used for any missing field.

```toml
# Default profile to use when none is explicitly requested.
default_profile = "rustacean"

# Maximum time (in seconds) a tool may run before being killed.
tool_timeout_secs = 30

# Maximum number of characters captured from a single tool invocation.
max_output_chars = 10000

# Whether the agent may revise its own output after generation.
self_correction = true

# Maximum number of consecutive self-correction rounds.
max_self_corrections = 3

# Whether to persist checkpoints for resumability.
checkpoint_enabled = true

# Context window size limit in tokens (model-dependent).
context_limit = 128000

# Whether prompt caching is enabled.
prompt_caching = true

# Whether review mode is active (human-in-the-loop).
review_enabled = true
```

### Default values

| Field | Default | Description |
|-------|---------|-------------|
| `default_profile` | `"default"` | Profile name |
| `tool_timeout_secs` | `30` | Tool execution timeout |
| `max_output_chars` | `10_000` | Max tool output chars |
| `self_correction` | `true` | Auto-fix code errors |
| `max_self_corrections` | `3` | Max fix attempts |
| `checkpoint_enabled` | `true` | Git checkpointing |
| `context_limit` | `128_000` | Token context window |
| `prompt_caching` | `true` | Prompt caching |
| `review_enabled` | `true` | Clippy review |

---

## CLI Flags Reference

```
Usage: tua-rs [OPTIONS] [COMMAND]

Commands:
  profiles   List available Rust coding profiles
  config     Show current configuration
  check      Run cargo check
  test       Run cargo test
  review     Run clippy code review
  skills     Show available skills
  dashboard  Launch web dashboard
  tui        Launch terminal user interface

Options:
  -p, --prompt <PROMPT>        Prompt to send to the agent
      --profile <PROFILE>      Active Rust profile [default: rustacean]
  -m, --model <MODEL>          Model identifier [default: deepseek/deepseek-v4-flash]
      --cwd <CWD>              Working directory
      --no-self-correct        Disable self-correction
      --no-checkpoint          Disable checkpointing
      --no-review              Disable code review
      --permission <PERMISSION> Permission mode [default: ask]
  -h, --help                   Print help
  -V, --version                Print version
```

---

## Profiles Table

| # | Name | Emoji | Description | Unwrap Ban | Unsafe Ban | Doc-Tests | Clippy Pedantic |
|---|------|-------|-------------|:----------:|:----------:|:---------:|:---------------:|
| 1 | `ferris` | 🦀 | Friendly, beginner-friendly Rust | No | Yes | Yes | No |
| 2 | `borrow-checker` | 🔍 | Strict lifetime auditing | No | No | No | Yes |
| 3 | `rustacean` | 🚀 | Idiomatic, performant Rust | Yes | Yes | Yes | Yes |
| 4 | `cargo-cult` | 📦 | Dependency-smart, ecosystem-aware | No | No | No | No |
| 5 | `unsafe-ferris` | 🔓 | Unsafe Rust specialist | No | No | Yes | Yes |
| 6 | `test-crab` | 🧪 | Testing-focused, thorough coverage | No | Yes | No | No |
| 7 | `doc-crab` | 📖 | Documentation-focused, educational | No | Yes | Yes | No |
| 8 | `strict` | 🛡️ | All guardrails enabled | Yes | Yes | Yes | Yes |

---

## Tools Table

| # | Tool Name | Description | Required Args | Optional Args |
|---|-----------|-------------|---------------|---------------|
| 1 | `cargo` | Run cargo commands (build, test, check, etc.) | `subcommand` | `args`, `cwd` |
| 2 | `rustc` | Run the Rust compiler directly | `action` | `target` |
| 3 | `rustfmt` | Format Rust code according to style guidelines | — | `check`, `files` |
| 4 | `clippy` | Run Clippy — 550+ lint rules | — | `deny_warnings`, `allow`, `fix` |
| 5 | `rustup` | Manage Rust toolchains | `action` | `target` |
| 6 | `cargo_audit` | Check dependencies for known vulnerabilities | — | `fix` |
| 7 | `cargo_outdated` | Display dependencies with newer versions | — | `workspace` |
| 8 | `cargo_udeps` | Detect unused dependencies | — | — |
| 9 | `cargo_deny` | Check licenses/advisories/bans/sources | — | `check` |
| 10 | `cargo_bench` | Run benchmarks | — | `args`, `cwd` |
| 11 | `cargo_doc` | Build API documentation | — | `open`, `args`, `cwd` |
| 12 | `cargo_test_doc` | Run doc-tests | — | `package`, `args`, `cwd` |
| 13 | `wasm_pack` | Build/test/pack Rust → Wasm | — | `subcommand`, `target`, `args`, `cwd` |
| 14 | `rustc_explain` | Get detailed Rust compiler error explanation | `error_code` | — |

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         main.rs                                 │
│   CLI (clap) → dispatches to subcommands / agent loop           │
└──────────────────────┬──────────────────────────────────────────┘
                       │
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
┌──────────────┐ ┌──────────┐ ┌──────────────┐
│   config.rs  │ │profiles.rs│ │   tools.rs   │
│ Load TOML    │ │8 profiles │ │14 tools      │
│ ConfigError  │ │Guardrails │ │ToolExecutor  │
└──────────────┘ └──────────┘ └──────┬───────┘
                                     │
        ┌──────────────┼──────────────┼──────────────┐
        ▼              ▼              ▼              ▼
┌──────────────┐ ┌──────────┐ ┌──────────────┐ ┌──────────┐
│  agent/      │ │providers/│ │ checkpoint.rs│ │review.rs │
│  mod.rs      │ │openai_   │ │ Git checkpoint│ │Clippy    │
│  AgentLoop   │ │compatible│ │ CheckpointErr │ │ReviewErr  │
│  AgentError  │ │ProviderCfg││               │ │          │
│  AgentEvent  │ │SSE parser│ │               │ │          │
│  ModelProv.  │ │          │ │               │ │          │
└──────────────┘ └──────────┘ └──────────────┘ └──────────┘
                                     │
                        ┌────────────┼────────────┐
                        ▼            ▼            ▼
                 ┌──────────┐ ┌──────────┐ ┌──────────┐
                 │  tui.rs  │ │dashboard │ │ skills.rs│
                 │  App,Tab │ │ .rs      │ │10 skills │
                 │  Terminal│ │ Axum web │ │Markdown  │
                 │  UI      │ │ server   │ │references│
                 └──────────┘ └──────────┘ └──────────┘
```

### Key dependencies

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime, process spawning, networking |
| `reqwest` | HTTP client for LLM API calls |
| `clap` | CLI argument parsing |
| `serde` / `serde_json` | JSON serialization/deserialization |
| `thiserror` | Typed error types with `Display` |
| `tracing` | Structured logging |
| `toml` | Configuration file parsing |
| `async-trait` | Async trait methods |
| `futures` | Stream combinators, channels |
| `axum` | Web dashboard HTTP server |
| `ratatui` / `crossterm` | Terminal UI rendering |

---

## Known Limitations

### 1. OpenAI-compatible provider only
Only the OpenAI `/chat/completions` streaming format is supported. Anthropic,
Google Gemini, and other non-OpenAI-compatible APIs are not supported out of
the box. To add support, implement the `ModelProvider` trait for a new provider
module.

### 2. No conversation persistence
Chat history lives in memory only. Restarting the TUI or agent loop loses all
conversation history. There is no built-in database or file-based storage for
persisting sessions across restarts.

### 3. Git checkpointing is destructive
`rollback()` uses `git reset --hard HEAD~1`, which permanently discards
uncommitted changes in the working tree. There is no "soft rollback" option
that preserves changes as staged but uncommitted. Use with caution.

### 4. Self-correction requires cargo
The self-correction feature runs `cargo check` as a subprocess. If the working
directory is not a Cargo project, or if `cargo` is not installed, the feature
silently skips the correction step. There is no fallback to `rustc` directly.

### 5. TUI is single-threaded
The terminal UI runs on the main thread using a polling event loop. Long-running
operations (e.g., LLM streaming) block the UI. For production use, the agent
loop should run in a separate tokio task.

### 6. Web dashboard is minimal
The dashboard provides only two API endpoints and static HTML rendering. There is
no real-time event stream (WebSocket), no authentication, and no per-session
isolation. It is designed for local development monitoring.

### 7. No streaming in review mode
The `review` subcommand runs `cargo clippy` synchronously and blocks until it
completes. For large projects, this may take several seconds. There is no
progressive output or timeout.

### 8. Token count is estimated
The token count shown in the TUI status bar is a rough estimate (1 token ≈ 4
characters). It does not use a real tokenizer and may be inaccurate, especially
for code with many symbols or non-ASCII characters.

### 9. Environment variable for API key
The agent expects `OPENAI_API_KEY` to be set in the environment. There is no
interactive prompt for API key entry, no keychain integration, and no `.env`
file loading. This is by design for security, but limits convenience.

### 10. No HTTP streaming timeout
The SSE parser has no per-chunk timeout. If the LLM API stops sending data
mid-stream without sending `[DONE]`, the agent loop will hang indefinitely.
A configurable stream idle timeout is planned for a future release.

### 11. Tool output is truncated
Tool output is capped at `max_output_chars` (default 10,000 characters). If a
tool (e.g., `cargo build`) produces more output, it is silently truncated.
There is no pagination or streaming of long tool outputs.

### 12. No language server integration
The agent does not integrate with `rust-analyzer` or any LSP server for
real-time diagnostics. It relies on `cargo check` and `cargo clippy` for
feedback, which are slower than an LSP-based approach.

### 13. Limited WASM support
The `wasm_pack` tool assumes `wasm-pack` is installed globally. There is no
fallback to raw `wasm-bindgen` or `cargo build --target wasm32-unknown-unknown`.
The tool also does not handle `wasm-opt` or WASI targets.

### 14. No progress indicator for long operations
Operations like `cargo build`, `cargo test`, or `cargo clippy` may take a long
time but provide no progress indication in the TUI or CLI output. The agent
simply waits for the subprocess to complete.

### 15. Single project focus
The agent is designed for single-project use. It does not support multi-workspace
scenarios or monorepos with multiple independent Cargo workspaces. The `cwd`
parameter must point to a single Cargo project.

---

## Contributing

See `CONTRIBUTING.md` for development setup, coding standards, and PR guidelines.

## License

MIT or Apache-2.0, at your option.
