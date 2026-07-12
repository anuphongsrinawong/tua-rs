# Task: Implement Session Persistence (JSONL save/load)

## File: src/session.rs

The session module already has `Session` struct with `meta` and `messages: Vec<AgentMessage>`. But there's no save/load to disk yet.

## Requirements

### 1. Add `Session::save(&self, path: &Path) -> SessionResult<()>`
- Write session to a JSONL file:
  - Line 1: SessionMeta as JSON
  - Lines 2..N: Each AgentMessage serialized as JSON (one per line)
- Use `serde_json::to_string` for serialization
- Create parent directories if they don't exist
- Use `std::fs::write` for atomic write

### 2. Add `Session::load(path: &Path) -> SessionResult<Self>`
- Read JSONL file:
  - Line 1: deserialize as SessionMeta
  - Lines 2..N: deserialize each as AgentMessage
- Return Session with loaded meta + messages
- Error handling: file not found, invalid JSON, empty file

### 3. Add `Session::save_to_default(&self) -> SessionResult<()>`
- Save to `~/.tua-rs/sessions/{session_id}.jsonl`
- Create `~/.tua-rs/sessions/` directory if needed

### 4. Add `Session::load_from_default(id: &str) -> SessionResult<Self>`
- Load from `~/.tua-rs/sessions/{id}.jsonl`

### 5. Add tests (in existing test mod)
- `test_save_load_roundtrip` — save → load → verify messages match
- `test_save_overwrites_existing` — save twice, second overwrites
- `test_load_nonexistent_file` — returns error
- `test_save_load_empty_session` — session with no messages
- `test_save_load_with_messages` — session with user+assistant messages
- `test_save_to_default_creates_dir` — directory auto-created
- `test_save_load_roundtrip_with_tool_calls` — messages with tool calls survive

## Implementation Notes
- Add `use std::io::BufRead;` for line-by-line reading
- File format is JSONL (newline-delimited JSON)
- Use `SessionError::ReadFailed` and `SessionError::WriteFailed` for errors
- Make sure `SessionMeta` and `AgentMessage` derive `Serialize + Deserialize`

## Verification
```bash
cargo test session
cargo clippy
```

CRITICAL: Use Write tool. Read existing code first. Add new code, don't break existing.
