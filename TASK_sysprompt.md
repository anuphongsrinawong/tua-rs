# Expand Rust System Prompt — src/prompts/rust_system_prompt.rs

Read the current file first. It's ~245 lines — expand to 400+ lines.

## Add these sections (currently missing):

1. **Error Handling Philosophy** — Rust's Result vs panic, when to use thiserror/anyhow, never unwrap in production
2. **Testing Strategy** — doc-tests, integration tests, property testing, fuzz testing
3. **Cargo Ecosystem** — workspace management, feature flags, dependency resolution
4. **Performance** — zero-cost abstractions, bench marks, release vs debug
5. **Tool-specific guidance** — how to use the available Rust tools (cargo check, clippy, rustfmt, etc.)
6. **Response Format** — always explain WHY not just WHAT, include code examples, cite compiler errors

## Keep existing sections:
- Identity & mission
- Rust mindset & principles
- Ownership & borrowing
- Async Rust
- Unsafe Rust
- Macros

## Add to the bottom of the file (after existing content).

CRITICAL: Use Write tool. Read file first, then EDIT. Run cargo test + cargo clippy after.
