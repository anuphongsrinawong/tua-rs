# Tua Agent RS — Project Context

## Identity
🦀 **Tua Agent RS** — Rust-specialized AI coding agent in pure Rust.
A CLI tool + TUI for AI-assisted Rust development: write code, fix bugs,
run cargo commands, review pull requests, and orchestrate multiple agents.

## Architecture (25 modules, 17K lines)

```
src/
├── main.rs              CLI (clap) — tua-rs [COMMAND]
├── agent/mod.rs         Agent harness: messages, tools, events, streaming loop
├── tui.rs               Terminal UI (ratatui + crossterm): tabs, streaming, diff viewer, provider picker
├── orchestrator.rs      Multi-agent orchestration: split tasks, plan parallel groups, dispatch workers
├── providers/           AI model providers
│   ├── mod.rs           Provider registry, config loading, validation
│   ├── openai_compatible.rs  OpenAI-compatible API (SSE streaming, reasoning_content)
│   ├── anthropic.rs     Anthropic API (SSE parsing, tool conversion)
│   ├── ollama.rs        Local Ollama provider
│   └── mock.rs          MockProvider for testing (builder pattern)
├── prompts/
│   └── rust_system_prompt.rs  1,148-line Rust expert system prompt
├── tools.rs             Rust toolchain: cargo, rustc, clippy, rustfmt, wasm-pack (14 tools)
├── config.rs            TOML config loader (~/.tua-rs/config.toml)
├── profiles.rs          8 coding profiles (ferris, rustacean, borrow-checker, etc.)
├── session.rs           Session persistence: save/load JSONL, list sessions
├── workspace.rs         Cargo workspace detection + member listing
├── parallel.rs          Parallel task runner with thread pool
├── learning.rs          Self-improvement: track fixes, suggest patterns (SQLite)
├── checkpoint.rs        Git checkpoint/rollback helpers
├── review.rs            Code review system
├── completion.rs        Rust code completion engine
├── wasm.rs              WebAssembly compilation
├── skills.rs            Rust knowledge base (hardcoded)
└── dashboard.rs         Web dashboard (axum HTTP server)
```

## Tech Stack

| Category | Crates |
|---|---|
| **Async runtime** | tokio (full features) |
| **HTTP client** | reqwest (json, stream, rustls-tls) |
| **Web server** | axum + tower-http (cors) |
| **CLI** | clap (derive) |
| **TUI** | ratatui + crossterm |
| **Serialization** | serde + serde_json |
| **Config** | toml |
| **Error handling** | thiserror, anyhow |
| **Database** | rusqlite (bundled SQLite) |
| **UUID** | uuid (v4, serde) |
| **Async traits** | async-trait |
| **Benchmarks** | criterion (async_tokio) |
| **Paths** | dirs |

## Coding Conventions

- **Error handling**: Use `AgentResult<T>`, `SessionResult<T>` — never bare `.unwrap()` in production
- **Async**: All tool executors return `AgentResult<T>`, use `Box::pin(async move { ... })` for streams
- **Tests**: `#[cfg(test)] mod tests` at bottom of each file, use `#[tokio::test]` for async
- **Clippy**: Must pass with 0 warnings (`cargo clippy -- -D warnings`)
- **Formatting**: `cargo fmt` before commit
- **Profiles**: 8 profiles — `rustacean` is default, `strict` bans unwrap/unsafe
- **System prompt**: Located at `src/prompts/rust_system_prompt.rs` (1,148 lines)

## Key Types

```rust
// Agent events streaming from LLM
enum AgentEvent { TextDelta, ThinkingDelta, ToolCall, ToolResult, Error, Done }

// Session persistence
struct Session { meta: SessionMeta, messages: Vec<AgentMessage> }

// TUI app state
struct App { tabs: Vec<Tab>, mode: AppMode, picker: Picker, edits: Vec<FileEdit> }

// Orchestrator
struct SubTask { id, description, files, prompt, can_parallel }
fn orchestrate(description, max_parallel) -> OrchestrationResult
```

## Common Tasks

| Task | Command |
|---|---|
| Run all tests | `cargo test --lib` |
| Check clippy | `cargo clippy -- -D warnings` |
| Build release | `cargo build --release` |
| Launch TUI | `cargo run -- tui` |
| Orchestrate | `cargo run -- orchestrate "task" --parallel 4` |
| One-shot agent | `cargo run -- -p "prompt"` |

## Current Stats
- Tests: 374 passed, 0 failed
- Clippy: 0 warnings
- Git tag: v1.1.0
- Lines: ~17,300
