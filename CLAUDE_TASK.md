# CLAUDE_TASK.md — Wire Agent Harness to TUI Streaming

## Project: /tmp/tua-rs (Rust, v1.1.0)

## Current State

The project has two disconnected components:
1. **`src/agent/mod.rs`** — `AgentLoop::run()` returns `Stream<Item = AgentEvent>` — fully working
2. **`src/tui.rs`** — `App` struct with tabs, input, command palette — NO run loop, NO agent connection
3. **`src/main.rs`** — `Commands::Tui` just prints "🦀 TUI mode" — doesn't launch anything

## Goal

When the user runs `tua-rs tui`, they get a working TUI where:
- Type a message → press Enter → agent responds with streaming text
- Agent tool calls appear in the UI
- Multiple tabs, command palette, scrolling all work

## Implementation Plan

### Step 1: Add `App::run()` to `src/tui.rs`

Add a `pub fn run(&mut self) -> anyhow::Result<()>` method that:
```
1. Initialize ratatui terminal via TerminalGuard::enter()
2. Enter main event loop: loop { event::poll(timeout); match event { ... } }
3. Handle keys: 
   - Enter → spawn agent task (see Step 2), send user message
   - Ctrl+C/Esc → quit
   - Ctrl+T/W/P → existing tab/palette handlers
   - Backspace → edit input
   - Char → append to input_buffer
4. Render loop: terminal.draw(|f| self.render(f))
```

Use `crossterm::event::poll(Duration::from_millis(50))` for non-blocking event loop.
Call `terminal.draw(|f| self.render(f))` on every iteration.

### Step 2: Connect Agent to TUI

When user presses Enter:
```
1. Take input_buffer, add as AgentMessage::User to active tab
2. Clear input_buffer
3. Spawn tokio task: tokio::spawn(async move {
     let stream = agent_loop.run(messages_clone);
     while let Some(event) = stream.next().await {
         match event {
             TextDelta(s) → append to tab's last assistant message
             ThinkingDelta(s) → show dimmed
             ToolCall { name, args } → show "🔧 {name}"
             ToolResult { result } → show "✅ ok" or "⚠️ fail"
             Error(msg) → show in red
             Done → finish
         }
     }
   })
```

### Step 3: Use MockProvider for TUI

Inside `App::run()`, create an `AgentLoop` with `MockProvider`:
```rust
let provider = Arc::new(MockProvider::new());
let tools = get_rust_tools();
let agent_loop = AgentLoop::new(provider, system_prompt, tools, AgentConfig::default());
```

Store `agent_loop` in `App` struct (requires `Arc<AgentLoop>`).

### Step 4: Add imports for `src/tui.rs`

You'll need:
```rust
use crate::agent::{AgentEvent, AgentLoop, AgentMessage, AgentConfig, AgentTool};
use crate::providers::mock::MockProvider;
use crate::tools::get_rust_tools;
use crate::prompts::rust_system_prompt::RUST_SYSTEM_PROMPT;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
```

### Step 5: Export modules in `src/lib.rs`

Make sure `tui` module is public and all agent types are exported.

### Step 6: Wire `Commands::Tui` in `src/main.rs`

```rust
Some(Commands::Tui) => {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut app = App::new();
        app.run()
    })?;
}
```

### Step 7: Add `tokio` to Activate rt-multi-thread

Make sure `Cargo.toml` has:
```toml
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync", "time"] }
```

## Implementation

1. Read ALL source files first: src/tui.rs, src/agent/mod.rs, src/main.rs, src/lib.rs, Cargo.toml
2. Modify src/tui.rs — add run loop + agent streaming
3. Modify src/main.rs — wire Commands::Tui
4. Modify src/lib.rs if needed — add tui module export
5. Run `cargo build` — fix any errors
6. Run `cargo test --lib` — verify all 282 tests still pass
7. Run `cargo clippy` — zero warnings

## Verification
```bash
cargo build          # must compile
cargo test --lib     # all 282 pass
cargo clippy         # zero warnings
```

## CRITICAL
- Do NOT remove any existing functionality
- The TUI `run()` runs in a loop — it only exits on Ctrl+C or Ctrl+Q
- Agent runs in background tokio task, events stream into a channel that the main loop reads
- Use `tokio::sync::mpsc::unbounded_channel()` for AgentEvent → TUI communication
- The main event loop polls both crossterm events AND the agent channel
- Use `tokio::task::spawn_blocking` or a separate thread for the crossterm event loop since ratatui + crossterm is not async-native
