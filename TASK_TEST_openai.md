# Task: Add tests to src/providers/openai_compatible.rs (currently 3 tests)

Read src/providers/openai_compatible.rs first. Then add 9+ more tests.

Write tests for:
- `OpenAiCompatibleProvider::new()` — verify config
- SSE parsing edge cases: empty stream, partial chunks, malformed JSON
- Multiple content deltas
- Reasoning content handling
- Tool call accumulation
- Error handling

Add new tests into the EXISTING #[cfg(test)] mod.

Run `cargo test openai_compatible` to verify all pass. Fix errors.
