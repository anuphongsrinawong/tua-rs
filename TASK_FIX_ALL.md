# Fix EVERYTHING in one run — ALL CI failures

You must fix ALL of these:

## 1. Tools tests (8 failures)
Run: cargo test tools --lib
- Tool count changed from 14 to 19
- Need to update: test_tool_count, test_all_tool_names, test_rust_tools_returns, test_tool_name_naming_convention

## 2. AST tests (2 failures)  
- parse_struct: need to handle "pub struct" prefix
- render_compact: update for new parser

## 3. Context test (1 failure)
- tokenize: "--all-features" is one token with hyphens

## 4. Hierarchy test (1 failure)
- detect_devops_expert: "deployment" contains "dep" — check DevOps BEFORE DepExpert

## 5. Cross-platform fixes
- orchestrator.rs line 223: hardcoded "/home/user" → use dirs::home_dir()
- checkpoint.rs: "/tmp/" in tests → use std::env::temp_dir()
- config.rs: HOME env tests → use temp_dir(), lock for parallel safety  
- workspace.rs: canonicalize paths for macOS /private/tmp

## 6. Clippy
- cargo clippy -- -D warnings must show 0 errors
- Fix: identical if blocks, unused imports, unreachable patterns, empty doc comments

## Final verification
cargo test --lib → 0 failures
cargo clippy -- -D warnings → 0 errors
cargo fmt --check → clean
git add -A && git commit
