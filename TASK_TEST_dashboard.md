# Task: Add tests to src/dashboard.rs (currently 0 tests)

Read src/dashboard.rs first. Then add #[cfg(test)] mod tests with 8+ unit tests.

Write tests for:
- `Dashboard::new()` — verify default state
- Any public functions: data collection, formatting, status methods
- Edge cases: empty project, missing files
- Verify HTTP response formatting

Add tests at the BOTTOM of the file.

Run `cargo test dashboard` to verify all pass. Fix errors.
