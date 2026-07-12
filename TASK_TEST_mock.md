# Task: Add tests to src/providers/mock.rs (currently 1 test)

Read src/providers/mock.rs first. Then add 9+ more tests to the existing #[cfg(test)] mod.

Write tests for:
- `MockProvider::new()` — verify default is empty
- `MockProvider::with_text()` — verify single text response
- `MockProviderBuilder` — test more builder patterns:
  - multiple text deltas
  - thinking + text sequence
  - tool call + tool result + text
  - error event
  - delay behavior
- Edge case: empty builder

Add new tests into the EXISTING #[cfg(test)] mod tests block.

Run `cargo test mock` to verify all pass. Fix errors.
