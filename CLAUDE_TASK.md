# CLAUDE_TASK.md — Fix tua-rs Tests + Clippy Issues

## Project: /tmp/tua-rs (Rust, v1.1.0)
## Task: Fix 3 failing tests + 1 clippy error + 9 clippy warnings

---

## 1. FIX: Clippy ERROR (blocks `cargo clippy -- -D warnings`)

**File:** `src/session.rs:458`

```rust
let era = if z >= 0 { z } else { z - 146096 } / 146097;
```

`z` is unsigned (`days + 719468` where `days` is `i64` but `z` is inferred as unsigned), so `z >= 0` is always true. Fix: remove the `if` entirely since unsigned types are never negative, or cast properly if the algorithm needs it.

The function `seconds_to_datetime(secs: i64)` takes `i64`, so `z = days + 719468` should work with `i64`. Make sure the types are consistent.

---

## 2. FIX: 3 Failing Tests

### Test A: `providers::mock::tests::test_builder_complex`
**File:** `src/providers/mock.rs:372`

```
assertion `left == right` failed
  left: 6
 right: 7
```

The mock builder creates events and expects 7 but only 6 come through the stream. Check the builder chain and count events.

### Test B: `session::tests::test_push_message_updates_timestamps`
**File:** `src/session.rs:514`

```
assertion `left != right` failed
  left: "2026-07-12T01:04:43Z"
 right: "2026-07-12T01:04:43Z"
```

The test expects `created_at` and `updated_at` to be different after pushing a message, but they're the same. The push operation isn't updating the timestamp.

### Test C: `session::tests::test_seconds_to_datetime_known_values`
**File:** `src/session.rs:906`

```
assertion `left == right` failed
  left: 2
 right: 10
```

The year calculation is wrong — getting 2 instead of 10 (probably expecting 2010). The `seconds_to_datetime` algorithm has a bug in the year/era calculation.

---

## 3. FIX (optional but nice): 9 Clippy Warnings

Run `cargo clippy 2>&1` and fix warnings:
- `unused import: StreamExt` in mock.rs
- `field 'message' is never read`
- `fields input/text/thinking/signature never read`
- `function session_file_path is never used`
- doc list item overindented
- `while let` loop simplification (x2)
- `type_complexity` — type is too complex
- `unused_comparisons` in session.rs (same as #1 above)

---

## Verification

After all fixes:
```bash
cargo test --lib           # ALL tests pass (282 passed, 0 failed)
cargo clippy -- -D warnings # 0 errors, 0 warnings
```

## CRITICAL
- Do NOT add any new features — only fix existing bugs
- Read the source before modifying
- Run `cargo test --lib` after EACH fix to verify
- Use `git add -A && git commit -m "fix: resolve test failures and clippy issues" && git push origin main` when done
