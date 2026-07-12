pub mod rust_system_prompt;

pub use rust_system_prompt::build_system_prompt as build_rust_system_prompt;

// ── Dynamic Prompting ────────────────────────────────────────────────

/// Detect the task type from the user's prompt and return a
/// mode-specific prefix to inject into the system prompt.
///
/// # Task Types
/// - `debug` — fixing bugs, compiler errors, runtime issues
/// - `feature` — new functionality, API design, greenfield code
/// - `refactor` — restructuring, improving existing code
/// - `test` — writing tests, coverage, test infrastructure
/// - `review` — code review, audit, security check
pub fn detect_task_mode(prompt: &str) -> &'static str {
    let lower = prompt.to_lowercase();
    
    // Debug: mentions errors, bugs, fix, broken, panic
    if lower.contains("error") || lower.contains("fix") || lower.contains("bug") 
       || lower.contains("broken") || lower.contains("panic") || lower.contains("fail") {
        return "debug";
    }
    // Test: mentions test, coverage, spec
    if lower.contains("test") || lower.contains("coverage") || lower.contains("spec") {
        return "test";
    }
    // Refactor: mentions refactor, restructure, clean, improve
    if lower.contains("refactor") || lower.contains("restructure") 
       || lower.contains("clean") || lower.contains("improve") {
        return "refactor";
    }
    // Review: mentions review, audit, security, check
    if lower.contains("review") || lower.contains("audit") 
       || lower.contains("security") || lower.contains("check for") {
        return "review";
    }
    // Default: new feature
    "feature"
}

/// Return a prompt prefix tuned for the detected task mode.
pub fn mode_prefix(mode: &str) -> &'static str {
    match mode {
        "debug" => "\n\n## 🐛 DEBUG MODE\n\
            You are debugging a Rust project. Prioritize:\n\
            1. Read error messages carefully — every line matters\n\
            2. Use `cargo check` and `rustc --explain` before guessing\n\
            3. Make minimal changes — fix ONE thing at a time\n\
            4. After each fix, verify with `cargo check`\n\
            5. If stuck, use the `grep` tool to find related code\n",
        
        "test" => "\n\n## 🧪 TEST MODE\n\
            You are writing tests. Prioritize:\n\
            1. Cover both happy paths AND edge cases\n\
            2. Use `#[should_panic]` for expected failures\n\
            3. Add doc-tests for public API examples\n\
            4. Aim for at least 80% coverage on new code\n\
            5. Run `cargo test` after writing tests\n",
        
        "refactor" => "\n\n## 🔧 REFACTOR MODE\n\
            You are refactoring existing code. Prioritize:\n\
            1. Preserve all existing behavior — don't break APIs\n\
            2. Run `cargo test` before AND after each change\n\
            3. Extract small, focused functions\n\
            4. Replace `.unwrap()` with `Result<T, E>`\n\
            5. Update all callers and imports\n",
        
        "review" => "\n\n## 👀 REVIEW MODE\n\
            You are reviewing code. Prioritize:\n\
            1. Check for `.unwrap()` and `.expect()` — suggest Result\n\
            2. Look for missing error handling\n\
            3. Verify all public APIs have documentation\n\
            4. Check for performance: unnecessary clones, allocations\n\
            5. Suggest improvements, don't rewrite unless asked\n",
        
        _ => "\n\n## 🚀 BUILD MODE\n\
            You are building a new feature. Prioritize:\n\
            1. Start with the public API — design the interface first\n\
            2. Write a doc-test showing usage before implementing\n\
            3. Build incrementally — cargo check after each step\n\
            4. Handle errors with Result, never panic\n\
            5. Add tests as you go, not at the end\n",
    }
}
