# Task: TUI Improvements — Install, Themes, Setup Wizard

## Overview
Three feature requests from user:
1. After `cargo build --release`, run `tua-rs` from anywhere (install binary to PATH)
2. TUI text visibility — add theme/style system (Dark/Light mode, color selection)
3. Add `tua setup` command for first-time configuration wizard

**CRITICAL: All work MUST maintain cross-platform support (Linux, macOS, Windows).**

---

## Feature 1: Binary in PATH (`cargo install` support)

### Current State
- Binary lands in `target/release/tua-rs`
- User must be in the project directory to run it

### Requirements
1. **Support `cargo install --path .`** — the binary should be installable globally
2. **Post-build hook** — after `cargo build --release`, print a helpful message:
   ```
   ✓ Build complete: target/release/tua-rs
   💡 Install globally: cargo install --path .
      Then run: tua-rs tui
   ```
3. **Add to README** — document the install command prominently
4. **Verify on all OS** — `~/.cargo/bin/tua-rs` should work on Linux/macOS/Windows

### Implementation
- Add a `[package.metadata]` section to Cargo.toml if needed for cargo-install
- The `main.rs` already uses clap with subcommands — `cargo install` will work OOTB
- If any asset files are needed at runtime, use `dirs::config_dir()` to find `~/.tua-rs/` or equivalent per OS

### Files
- `Cargo.toml` — add metadata if needed
- `README.md` — add install instructions
- Possibly add a build script (`build.rs`) that prints the post-build message

---

## Feature 2: TUI Theme System (Dark/Light Mode)

### Current State
The TUI (`src/tui.rs`, ~2210 lines) uses hardcoded colors throughout:
```rust
Style::default().fg(Color::Yellow).bg(Color::Reset)
Style::default().fg(Color::Black).bg(Color::Cyan)
Style::default().fg(Color::White).bg(Color::DarkGray)
```
The user reports text is NOT VISIBLE — likely because terminal background clashes with hardcoded colors.

### Requirements
1. **Theme struct** with named color slots:
   - `bg` — background
   - `fg` — primary text  
   - `accent` — highlights, active tabs, borders
   - `user_msg` — user message color
   - `agent_msg` — agent message color
   - `error` — error messages
   - `dim` — secondary text (timestamps, labels)
   - `border` — panel borders
   - `input_bg` — input field background
   - `palette_bg` — command palette background

2. **Two built-in themes**:
   - `dark` (default) — dark background, light text, cyan accent
   - `light` — light background, dark text, blue accent

3. **Theme config file**: `~/.tua-rs/theme.toml`
   ```toml
   [theme]
   name = "dark"  # or "light" or "custom"
   
   [theme.colors]
   bg = "#1a1a2e"
   fg = "#e0e0e0"
   accent = "#00d4ff"
   user_msg = "#00ff88"
   agent_msg = "#00d4ff"
   error = "#ff4444"
   dim = "#666666"
   border = "#333355"
   input_bg = "#0d0d1a"
   palette_bg = "#16213e"
   ```

4. **TUI command to switch**: `/theme dark`, `/theme light` in command palette
5. **CLI flag**: `tua-rs tui --theme dark` or `tua-rs tui --theme light`
6. **Auto-detect** terminal background on startup (optional, via `COLORFGBG` env var or OSC 11 query)

### Implementation
- Create `src/theme.rs` — Theme struct, built-in themes, TOML loading
- Modify `src/tui.rs` — replace all hardcoded `Color::*` with theme references
- Add `--theme` flag to `Commands::Tui` in `src/main.rs`
- Add `/theme` command in TUI command palette
- Add `theme` field to `AppState`

### Files
- `src/theme.rs` (NEW)
- `src/tui.rs` (MODIFY — replace hardcoded colors)
- `src/main.rs` (MODIFY — add `--theme` flag)
- `src/lib.rs` (MODIFY — add `pub mod theme`)
- `Cargo.toml` (no new deps needed — serde already included)

### Color Strategy (ratatui)
Use `ratatui::style::Color::Rgb(r, g, b)` for all colors to ensure consistency across terminals. Parse hex strings (`"#1a1a2e"`) → `Color::Rgb(26, 26, 46)`.

---

## Feature 3: `tua setup` Wizard

### Current State
Users must manually create config files:
- `~/.tua-rs/config.toml`
- `~/.tau/catalog.toml`
- `~/.tau/credentials.json`
- `~/.tau/providers.json`

### Requirements
1. **New subcommand**: `tua setup` (added to `Commands` enum in main.rs)
2. **Interactive wizard** that asks:
   - ✅ Which provider? (9router local, DeepSeek, OpenAI, Anthropic, Ollama, etc.)
   - ✅ API key (or "skip" for local/Ollama)
   - ✅ Default model
   - ✅ TUI theme preference (dark/light)
   - ✅ Self-correction on/off
   - ✅ Tool timeout
3. **Creates config files**:
   - `~/.tua-rs/config.toml`
   - `~/.tau/catalog.toml` (if doesn't exist)
   - `~/.tau/credentials.json` (if doesn't exist)
4. **Non-destructive** — never overwrite existing files without asking
5. **Works cross-platform** — use `dirs::config_dir()` for paths
6. **After setup, offer to launch TUI**: "Setup complete! Launch TUI now? [Y/n]"

### Implementation
- Create `src/setup.rs` (NEW)
- Add `Commands::Setup` variant to main.rs
- Use `dialoguer` crate for interactive prompts OR implement simple stdin/stdout prompts (to avoid adding dependencies)

### Preferred approach: NO new dependencies
Use `std::io::stdin().read_line()` for prompts — keep it simple, avoid `dialoguer` dependency. The setup is straightforward Q&A, not complex menus.

### Files
- `src/setup.rs` (NEW)  
- `src/main.rs` (MODIFY — add Setup variant)
- `src/lib.rs` (MODIFY — add `pub mod setup`)

---

## Acceptance Criteria

1. ✅ `cargo install --path .` succeeds and `tua-rs --version` works from any directory
2. ✅ `tua-rs tui --theme light` shows light theme, `tua-rs tui --theme dark` shows dark theme
3. ✅ `/theme dark` and `/theme light` commands work in TUI
4. ✅ All text is VISIBLE in both themes (high contrast)
5. ✅ `tua setup` wizard completes without errors and creates valid config files
6. ✅ `tua-rs tui` launches immediately after `tua setup`
7. ✅ `cargo test` passes all tests
8. ✅ `cargo clippy` passes (no warnings)
9. ✅ Cross-platform: Linux, macOS, Windows

---

## Implementation Order (for Tua rs agent)

1. **Feature 3 first** — `tua setup` wizard (creates config files)
2. **Feature 2 second** — Theme system (depends on config structure)
3. **Feature 1 third** — Binary install (simple metadata changes)

Run `cargo test` after EACH feature. Do NOT commit until all 3 are working and tested.

---

## Pitfalls to Avoid

- ❌ Do NOT introduce new crate dependencies (use serde/toml which are already in Cargo.toml)
- ❌ Do NOT hardcode Unix paths — use `dirs::config_dir()`
- ❌ Do NOT break existing subcommands (Profiles, Config, Check, Test, etc.)
- ❌ The TUI must still work with NO config file (fall back to dark theme defaults)
- ❌ Do NOT change the existing provider catalog format
