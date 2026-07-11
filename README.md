# 🦀 Tua Agent RS

**Tua Agent** ported to pure Rust — a Rust-specialized AI coding agent.

## Architecture

```
tua-rs/
├── src/
│   ├── main.rs              # CLI entry point (clap)
│   ├── agent/mod.rs         # Agent harness: messages, tools, events, loop
│   ├── providers/           # AI model providers
│   │   └── openai_compatible.rs  # OpenAI-compatible API (SSE streaming)
│   ├── prompts/
│   │   └── rust_system_prompt.rs  # Rust-expert system prompt (2000+ chars)
│   ├── tools.rs             # Rust toolchain tools (cargo, clippy, rustc)
│   ├── config.rs            # TOML configuration loader
│   └── profiles.rs          # Rust coding profiles (5 profiles)
└── Cargo.toml
```

## Features

- 🧠 **Agent Harness** — Async streaming agent loop with tool execution
- 🔌 **OpenAI-Compatible Provider** — SSE streaming, reasoning_content fallback
- 🦀 **Rust System Prompt** — 20 Rust domains + Chain-of-Thought protocol
- 🔧 **Rust Tools** — cargo, clippy, rustfmt, rustc_explain
- 📋 **5 Profiles** — ferris, borrow-checker, rustacean, cargo-cult, strict
- ⚙️ **TOML Config** — `~/.tua-rs/config.toml`

## Quickstart

```bash
git clone https://github.com/anuphongsrinawong/tua-rs.git
cd tua-rs
cargo build --release

# One-shot agent mode
cargo run -- -p "write a Rust function to compute fibonacci"

# Show profiles
cargo run -- profiles

# Show config
cargo run -- config
```

## CLI

```
tua-rs [OPTIONS] [COMMAND]

Options:
  -p, --prompt TEXT     One-shot agent prompt
  --profile TEXT        Rust coding profile [default: rustacean]
  -m, --model TEXT      Model to use
  --cwd TEXT            Working directory
  --no-self-correct     Disable self-correction

Commands:
  providers  Show configured providers
  profiles   List available profiles
  config     Show current configuration
  check      Run cargo check
```

## Ported from Python Tua Agent v0.0.2

| Python Module | Rust Module | Lines |
|---|---|---|
| `rust_system_prompt.py` | `prompts/rust_system_prompt.rs` | 245 |
| `rust_tools.py` (14 tools) | `tools.rs` (4 core tools) | 150 |
| `rust_profiles.py` (8 profiles) | `profiles.rs` (5 profiles) | 90 |
| `config.py` | `config.rs` | 232 |
| `tui.py` + `cli.py` | `main.rs` | 135 |
| `harness.py` + `loop.py` | `agent/mod.rs` | 679 |
| `openai_compatible.py` | `providers/openai_compatible.rs` | 620 |

**Total: ~2,150 lines of Rust**

## License

MIT
