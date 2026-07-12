# Task: Add tests to src/parallel.rs (currently 0 tests)

Read src/parallel.rs first. Then add #[cfg(test)] mod tests with 10+ unit tests.

Write tests for:
- `split_work` — verify it splits items evenly across N workers
- `run_parallel` — basic integration test
- Any public functions in the module
- Edge cases: empty input, single item, more workers than items
- Verify results are correct

Add tests at the BOTTOM of the file (after all existing code).

Run `cargo test parallel` to verify all pass. Fix errors.
