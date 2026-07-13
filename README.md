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

## 📖 Beginner's Guide

### 1️⃣ Prerequisites

| Requirement | How to check | How to install |
|-------------|-------------|----------------|
| **Rust** (1.75+) | `rustc --version` | [rustup.rs](https://rustup.rs) |
| **Git** (optional) | `git --version` | `sudo apt install git` |
| **API Key** (or 9Router) | — | See [Setup](#2️⃣-first-time-setup) |

### 2️⃣ First-Time Setup

Run the interactive setup wizard — it will create all config files for you:

```bash
tua-rs setup
```

The wizard asks:
- 🤖 **Provider** — choose 9Router (free, local), DeepSeek, OpenAI, Anthropic, Ollama, or custom
- 🔑 **API Key** — your provider's key (or "free" for 9Router)
- 🧠 **Model** — which model to use (e.g. `ds/deepseek-v4-pro`)
- 🎨 **Theme** — dark or light TUI mode
- 🔁 **Self-correction** — auto-fix Rust compiler errors? (recommended: yes)

After setup, it offers to launch the TUI immediately.

> 💡 **No API key?** Choose "9Router" with key "free" — uses local proxy at `localhost:20128`.

### 3️⃣ Using the TUI (Interactive Mode)

```bash
tua-rs tui                 # dark mode
tua-rs tui --theme light   # light mode
```

**Layout:**
```
┌─ Tab Bar ──────────────────────────────────┐
│ ▶ 🦀 Chat 1  │  🦀 Chat 2                  │
├─ Chat Area ────────────────────────────────┤
│ 👤 fix the borrow checker error in main.rs │
│ 🤖 The issue is on line 42...              │
│                                            │
├─ Input ────────────────────────────────────┤
│ 💬 your message here                       │
├─ Status ───────────────────────────────────┤
│ 🦀 rustacean  🔧 23 tools  ████░░  Ctrl+… │
└────────────────────────────────────────────┘
```

**Keybindings:**

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+T` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+P` | Command palette |
| `Ctrl+C` / `Ctrl+Q` | Quit |
| `PgUp` / `PgDn` | Scroll chat |

**Slash Commands** (type `/` in chat):

| Command | Action |
|---------|--------|
| `/help` | Show all commands |
| `/theme` | Toggle dark/light mode |
| `/model` | Change AI model |
| `/profile` | Show current profile |
| `/clear` | Clear chat history |
| `/diff` | Show recent file changes |

### 4️⃣ CLI Mode (One-Shot Tasks)

Best for quick tasks without opening the TUI:

```bash
# Ask the agent to do something
tua-rs -p "add doc comments to all public functions in src/lib.rs"

# Use a specific provider/model
tua-rs --provider deepseek --model deepseek-chat -p "refactor this to use Result<T, E>"

# Use a different coding profile
tua-rs --profile borrow-checker -p "fix all lifetime issues"

# Run with orchestrator (parallel subtasks)
tua-rs orchestrate --task "add tests for all modules in src/"
```

### 5️⃣ Choosing a Profile

Profiles change how the agent approaches problems:

| Profile | Best for | Example |
|---------|----------|---------|
| 🦀 **Ferris** | Beginners, learning Rust | `tua-rs --profile ferris -p "explain how ownership works"` |
| 🔍 **BorrowChecker** | Debugging lifetime errors | `tua-rs --profile borrow-checker -p "fix E0597"` |
| 🚀 **Rustacean** | Production code (default) | General Rust coding |
| 📦 **CargoCult** | Dependency selection | `tua-rs --profile cargo-cult -p "which HTTP client should I use?"` |
| 🧪 **TestCrab** | Writing tests | `tua-rs --profile test-crab -p "add property tests"` |
| 📚 **DocCrab** | Documentation | `tua-rs --profile doc-crab -p "document this crate"` |

### 6️⃣ Customizing Your Theme

Edit `~/.tua-rs/theme.toml` for full color control:

```toml
[theme]
name = "custom"

[theme.colors]
bg = "#0d1117"        # GitHub dark
fg = "#c9d1d9"
accent = "#58a6ff"    # GitHub blue
user_msg = "#7ee787"  # GitHub green
agent_msg = "#58a6ff"
error = "#f85149"     # GitHub red
dim = "#484f58"
border = "#21262d"
input_bg = "#161b22"
palette_bg = "#161b22"
```

### 7️⃣ FAQ

**Q: TUI text is hard to read?**
> Try `tua-rs tui --theme light` or type `/theme` in TUI.

**Q: "Provider not found" error?**
> Run `tua-rs setup` again to create config. Or manually edit `~/.tau/catalog.toml`.

**Q: How to use my own API key?**
> During `tua-rs setup`, choose your provider and enter the key. Or edit `~/.tau/credentials.json`:
> ```json
> { "deepseek": "sk-your-key-here" }
> ```

**Q: Agent takes too long?**
> Try a faster model: `tua-rs -m deepseek/deepseek-v4-flash -p "quick task"`

**Q: How to update?**
> ```bash
> cargo install --force --git https://github.com/anuphongsrinawong/tua-rs
> ```

### 8️⃣ Tips & Tricks

- 💡 **Start with TUI** — type `/help` to see all commands
- 💡 **Use tabs** — `Ctrl+T` for new tab, keep different tasks separate
- 💡 **Try 9Router first** — free, local, no API key needed
- 💡 **Self-correction ON** — the agent auto-fixes compiler errors
- 💡 **Dark theme at night, light during day** — `tua-rs tui --theme light`

## 📋 CLI Reference

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
