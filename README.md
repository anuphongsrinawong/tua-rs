# 🦀 Tua Agent RS

**Tua Agent RS** — Rust-specialized AI coding agent in pure Rust.
12+ features, 22 modules, 354 tests, 7MB release binary.

[![Tests](https://img.shields.io/badge/tests-354%20passed-brightgreen)]()
[![Clippy](https://img.shields.io/badge/clippy-0%20warnings-brightgreen)]()
[![Version](https://img.shields.io/badge/version-1.1.0-blue)]()

## Features

- 🧠 **Agent Harness** — Async streaming agent loop with tool execution + self-correction
- 🖥️ **TUI** — Interactive terminal UI with multi-tab chat, command palette, streaming agent
- 🎛️ **Provider Picker** — Interactive provider/model selection (9Router, OpenAI Codex, etc.)
- 💾 **Session Persistence** — Save/load sessions as JSONL (~/.tua-rs/sessions/)
- 🔌 **Multi-Provider** — OpenAI-compatible, Anthropic, Ollama (SSE streaming)
- 🔧 **14 Rust Tools** — cargo, clippy, rustfmt, rustc, cargo-audit, wasm-pack, etc.
- 📋 **8 Profiles** — ferris, borrow-checker, rustacean, cargo-cult, unsafe-ferris, test-crab, doc-crab, strict
- 🚀 **Release Binary** — 7MB single-file deployment

## Quickstart

```bash
git clone https://github.com/anuphongsrinawong/tua-rs.git
cd tua-rs
cargo build --release

# Interactive TUI (recommended)
./target/release/tua-rs tui

# One-shot agent mode
./target/release/tua-rs -p "write a Rust function to compute fibonacci"

# Show profiles
./target/release/tua-rs profiles
```

## CLI

```
tua-rs [OPTIONS] [COMMAND]

Options:
  -p, --prompt TEXT      One-shot agent prompt
  --profile TEXT         Profile [default: rustacean]
  -m, --model TEXT       Model name
  --provider TEXT        Provider name

Commands:
  tui         Launch interactive TUI
  profiles    List available profiles
  config      Show current configuration
  sessions    Session management
  wasm        Compile to WebAssembly
  complete    Code completion
  bench       Run benchmarks
```

## Architecture

```
src/
├── main.rs              CLI (clap)
├── agent/mod.rs         Agent harness: messages, tools, events, loop
├── tui.rs               Terminal UI (ratatui + crossterm)
├── providers/           AI model providers (openai, anthropic, ollama, mock)
├── prompts/             Rust system prompt
├── tools.rs             Rust toolchain tools (14 tools)
├── config.rs            TOML configuration
├── profiles.rs          8 coding profiles
├── session.rs           Session persistence (JSONL)
├── workspace.rs         Cargo workspace detection
├── parallel.rs          Parallel task execution
├── learning.rs          Self-improvement loop
├── checkpoint.rs        Git checkpoint/rollback
├── review.rs            Code review
├── completion.rs        Rust code completion
├── wasm.rs              WebAssembly compilation
├── skills.rs            Rust skills/knowledge base
└── dashboard.rs         Web dashboard (axum)
```

## Metrics

| Metric | Value |
|---|---|
| Tests | 354 passed, 0 failed |
| Clippy | 0 warnings |
| Lines | ~14,700 |
| Modules | 22 |
| Release binary | 7MB |

## License

MIT
