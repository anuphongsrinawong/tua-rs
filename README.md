# 🦀 Tua Agent RS

**Tua Agent RS** — Rust-specialized AI coding agent in pure Rust.
CLI agent loop, interactive TUI, 22+ dependencies, 467 tests.

[![Tests](https://img.shields.io/badge/tests-467%20passed-brightgreen)]()
[![CI](https://img.shields.io/badge/CI-passing-brightgreen)]()
[![Version](https://img.shields.io/badge/version-1.1.0-blue)]()

## 🚀 Quickstart

```bash
# Install globally from GitHub
cargo install --git https://github.com/anuphongsrinawong/tua-rs

# Or clone + build
git clone https://github.com/anuphongsrinawong/tua-rs.git
cd tua-rs && cargo build --release

# First-time setup
tua-rs setup

# Interactive TUI
tua-rs tui                     # dark mode (default)
tua-rs tui --theme light       # light mode

# CLI agent (streaming)
tua-rs -p "add unit tests for src/config.rs"

# List profiles
tua-rs profiles
```

## ✨ Features

| Category | Feature |
|----------|---------|
| 🧠 **Agent** | Async streaming agent loop, tool execution, self-correction, context guard |
| 🖥️ **TUI** | Multi-tab chat, command palette (`/help`, `/model`, `/theme`, ...), scrollback |
| 🎨 **Themes** | Dark/light mode, configurable via `~/.tua-rs/theme.toml`, `/theme` toggle |
| 🛠️ **Setup Wizard** | `tua setup` — interactive provider, API key, model, theme configuration |
| 🔌 **Providers** | 9Router, DeepSeek, OpenAI, Anthropic, Ollama, custom OpenAI-compatible |
| 🔧 **Tools** | cargo, clippy, rustfmt, rustc, audit, bench, doc, wasm-pack, grep + more |
| 📋 **Profiles** | ferris, borrow-checker, rustacean, cargo-cult, unsafe-ferris, test-crab, doc-crab, strict |
| 📊 **Progress** | indicatif progress bars in orchestrator |
| 🎛️ **Prompts** | inquire Select/Text/Confirm in setup wizard |
| 🖌️ **Highlight** | syntect syntax highlighting for code blocks |
| 🎯 **Structured** | rstructor — type-safe JSON Schema extraction from LLM responses |
| 🎨 **Terminal** | rich-rs — styled terminal output |
| 📦 **Install** | `cargo install --git` / `cargo install --path .` with post-build hint |

## 📋 CLI

```
tua-rs [OPTIONS] [COMMAND]

Options:
  -p, --prompt TEXT      One-shot agent prompt (streaming)
  --profile TEXT         Profile [default: rustacean]
  -m, --model TEXT       Model name [default: deepseek/deepseek-v4-flash]
  --provider TEXT        Provider name [default: openai]

Commands:
  setup       Interactive setup wizard
  tui         Launch interactive TUI [--theme dark|light]
  profiles    List available profiles
  config      Show current configuration
  sessions    Session management
  orchestrate Task decomposition + parallel dispatch
  wasm        Compile to WebAssembly
  complete    Rust code completion
  bench       Run benchmarks
```

## 🏗️ Architecture

```
src/
├── main.rs              CLI (clap) + streaming agent loop
├── agent/mod.rs         Agent harness: messages, tools, events, loop
├── tui.rs               Terminal UI (ratatui + crossterm) with theme support
├── theme.rs             Dark/light theme system
├── setup.rs             Interactive setup wizard (inquire prompts)
├── highlight.rs         Syntax highlighting (syntect)
├── providers/           AI model providers (openai-compatible, mock)
├── prompts/             Rust system prompt
├── tools.rs             Rust toolchain tools
├── config.rs            TOML configuration + theme loading
├── profiles.rs          8 coding profiles
├── orchestrator.rs      Task decomposition + parallel dispatch (indicatif progress)
├── session.rs           Session persistence (JSONL)
├── context_guard.rs     Token budget management
├── workspace.rs         Cargo workspace detection
├── parallel.rs          Parallel task execution
├── learning.rs          Self-improvement loop
├── checkpoint.rs        Git checkpoint/rollback
├── review.rs            Code review
├── completion.rs        Rust code completion
├── wasm.rs              WebAssembly compilation
├── skills.rs            Rust skills/knowledge base
├── sandbox.rs           Sandboxed execution
├── dashboard.rs         Web dashboard (axum)
└── build.rs             Post-build install hint
```

## 📊 Metrics

| Metric | Value |
|--------|-------|
| Tests | 467 passed, 0 failed |
| CI | ✅ passing (Linux + macOS + Windows) |
| Modules | 28 |
| Dependencies | 25 |
| Tools | 23 |

## 🎨 Theme

```toml
# ~/.tua-rs/theme.toml
[theme]
name = "dark"  # or "light", "custom"

[theme.colors]
bg = "#1a1a2e"
fg = "#e0e0e0"
accent = "#00d4ff"
# ...
```

Toggle in TUI: `/theme` or start with `tua-rs tui --theme light`

## 🔄 Update

```bash
cargo install --force --git https://github.com/anuphongsrinawong/tua-rs
```

## 📄 License

MIT
