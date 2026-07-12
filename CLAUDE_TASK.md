# CLAUDE_TASK.md — Fix 3 remaining clippy warnings in tua-rs

## Project: /tmp/tua-rs (Rust, v1.1.0)

## 3 Warnings to Fix

### 1. `field 'message' never read` — src/providers/anthropic.rs:212
```rust
MessageStart { message: AnthropicMessageStart },
```
This is a serde deserialization variant. Add `#[allow(dead_code)]` on the enum or the field.

### 2. `fields 'input', 'text', 'thinking', 'signature' never read` — src/providers/anthropic.rs:249-263
```rust
struct AnthropicContentStart {
    // ...
    input: Option<serde_json::Value>,
    text: Option<String>,
    thinking: Option<String>,
    signature: Option<String>,
}
```
These are serde deserialization fields (read via serde, not directly). Add `#[allow(dead_code)]` to the struct.

### 3. `very complex type used` — src/tui.rs:507
```rust
) -> Result<(TerminalGuard, Terminal<CrosstermBackend<Stdout>>), Box<dyn std::error::Error>>
```
Extract the return type into a `type` alias:
```rust
type TuiResult = Result<(TerminalGuard, Terminal<CrosstermBackend<Stdout>>), Box<dyn std::error::Error>>;
```

## Verification
```bash
cargo clippy 2>&1 | grep -c "warning:"
# Should output: 0 (minus the "generated N warnings" line)
cargo test --lib  # all pass
```

## After fixing
```bash
git add -A && git commit -m "fix: resolve remaining clippy warnings" && git push origin main
```

CRITICAL: Only fix these 3 warnings. Do NOT modify anything else.
