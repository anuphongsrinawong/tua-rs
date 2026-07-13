# Task: Add indicatif + inquire + syntect to tua-rs

## Safety First
- Read the task file COMPLETELY before modifying anything
- Run `cargo check` after EVERY file change
- If `cargo check` fails, undo the last change with `git checkout -- <file>`
- NEVER modify the running binary — only source files
- Commit only after ALL changes compile and tests pass

## Feature 1: indicatif — Progress Bars & Spinners

### Why
Replace manual progress bars in `orchestrator.rs` with real ones.
Replace `println!("...")` status messages with styled spinners.

### Steps
1. Add to Cargo.toml: `indicatif = "0.17"`
2. In `orchestrator.rs`, replace `render_progress_bar()` with `ProgressBar::new()`
3. In CLI agent (`main.rs` `run_cli_agent`), add a spinner while the agent is thinking
4. Run `cargo check` — must pass

### Example (orchestrator.rs)
```rust
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total as u64);
pb.set_style(ProgressStyle::default_bar()
    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
    .unwrap());

for result in &results {
    pb.inc(1);
    pb.set_message(format!("✓ {}", result.description));
}
pb.finish_with_message("✅ All tasks complete");
```

## Feature 2: inquire — Beautiful Interactive Prompts

### Why
Replace manual `stdin().read_line()` in `src/setup.rs` with styled prompts.

### Steps
1. Add to Cargo.toml: `inquire = "0.7"`
2. In `src/setup.rs`, replace:
   - `ask_number()` → `inquire::Select::new()` 
   - `ask_yes_no()` → `inquire::Confirm::new()`
   - `ask_string()` → `inquire::Text::new()`
3. Keep the same functionality (defaults, validation)
4. Keep `read_line` fallback for non-interactive terminals
5. Run `cargo check` — must pass

### Example
```rust
use inquire::{Confirm, Select, Text};

let provider = Select::new("Choose provider:", vec!["9Router", "DeepSeek", "OpenAI"])
    .with_starting_cursor(0)
    .prompt()?;

let api_key = Text::new("API key:")
    .with_default("free")
    .prompt()?;
```

## Feature 3: syntect — Syntax Highlighting in TUI

### Why
Show code blocks in the TUI chat with proper Rust syntax highlighting.

### Steps
1. Add to Cargo.toml: `syntect = "5"`
2. Create `src/highlight.rs`:
   - Function `highlight_rust(code: &str) -> Vec<ratatui::text::Span>`
   - Uses syntect's Rust syntax definition
   - Returns colored spans ready for ratatui rendering
3. In `src/tui.rs` `render_chat_area()`:
   - Detect code blocks (```rust ... ```) in messages
   - Apply syntax highlighting to code blocks
4. Run `cargo check` — must pass

### Example (highlight.rs)
```rust
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::html::highlighted_html_for_string;
use ratatui::style::{Color, Style};
use ratatui::text::Span;

pub fn highlight_rust(code: &str) -> Vec<Span<'static>> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let syntax = ps.find_syntax_by_extension("rs").unwrap();
    // ... parse and return colored spans
}
```

## Final Verification
```bash
cargo check          # must pass — 0 errors
cargo test --lib     # must pass — all tests
cargo fmt            # format all
git add -A && git commit -m "feat: indicatif + inquire + syntect integration"
```

## Files to Modify
- `Cargo.toml` — add 3 dependencies
- `src/orchestrator.rs` — indicatif progress bars
- `src/main.rs` — spinner in CLI agent
- `src/setup.rs` — inquire prompts
- `src/highlight.rs` — NEW: syntax highlighting
- `src/tui.rs` — integrate highlighted code blocks
- `src/lib.rs` — add `pub mod highlight`

## CRITICAL RULES
1. Read ALL files before modifying
2. `cargo check` after every change
3. If CHECK FAILS → `git checkout -- <file>` to undo
4. Keep existing functionality — these are enhancements, not rewrites
5. NO new dependencies beyond indicatif, inquire, syntect
