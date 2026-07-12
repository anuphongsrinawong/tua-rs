# Fix production unwraps in these 3 files

## File 1: src/session.rs (51 unwraps)

Replace ALL .unwrap() in production code (not in #[cfg(test)]) with proper error handling:
- Use `?` operator where the function returns Result
- Use `.ok_or_else(|| SessionError::...)` for Option unwraps
- Use `.expect("reason")` ONLY where the invariant is guaranteed (e.g., lock poisoning)

## File 2: src/tools.rs (49 unwraps)

Same pattern — replace unwraps in tool executors with proper error handling.
- Tool executors should return AgentResult, not panic
- Use `.unwrap_or_default()` for config lookups with safe defaults

## File 3: src/dashboard.rs (19 unwraps)

Same pattern — server code should never panic on malformed input.

CRITICAL: Do NOT modify test code. Only production functions. Run cargo test after each file.
