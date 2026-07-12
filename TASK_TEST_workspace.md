# Task: Add tests to src/workspace.rs (currently 5 tests)

Read src/workspace.rs first. Then add 5+ more tests to the existing #[cfg(test)] mod.

Write tests for:
- Cargo workspace detection
- Member crate listing
- Edge cases: empty workspace, single crate, nested workspaces
- Error handling for invalid Cargo.toml

Add new tests into the EXISTING #[cfg(test)] mod.

Run `cargo test workspace` to verify all pass. Fix errors.
