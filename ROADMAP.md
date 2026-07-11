# Tua Agent RS — Improvement Roadmap

> Current: v0.4.0 | 18/19 features | 220 tests | 6,930 lines

---

## 🔴 Critical (must fix)

| # | Issue | Impact | Effort | Fix |
|---|---|---|---|---|
| 1 | **Tests太少** — 220 vs Python 795 | 🔥🔥🔥 | สูง | เพิ่ม test ทุก module: agent loop, provider streaming, tools, checkpoint, review, skills, tui |
| 2 | **TUI diff viewer** — #8 ยังไม่มี | 🔥🔥 | กลาง | เพิ่ม diff rendering ใน tui.rs |
| 3 | **Clippy warnings** — 7 warnings | 🔥🔥 | ต่ำ | `cargo clippy --fix` |
| 4 | **Agent loop ยังไม่เชื่อม TUI** | 🔥🔥🔥 | สูง | Wire up AgentHarness → TUI event loop |

## 🟡 Should Improve

| # | Issue | Impact | Effort | Fix |
|---|---|---|---|---|
| 5 | **System prompt สั้นไป** — 245 lines vs Python 188 | 🔥🔥 | ต่ำ | Expand rust_system_prompt ให้ครบ 20 domains |
| 6 | **Error handling** — หลายที่ใช้ unwrap() | 🔥🔥 | กลาง | เปลี่ยนเป็น Result + thiserror ทุกจุด |
| 7 | **Dashboard static** — ไม่มี real-time update | 🔥 | กลาง | WebSocket หรือ polling |
| 8 | **No bench tests** | 🔥 | ต่ำ | criterion benchmarks สำหรับ agent loop |
| 9 | **Documentation** — ไม่มี GUIDE.md แบบ Python | 🔥 | กลาง | เขียนคู่มือแบบละเอียด |
| 10 | **Config validation** — ไม่ validate profiles/tools | 🔥 | ต่ำ | เพิ่ม validation ใน config.rs |

## 🟢 Nice to Have

| # | Issue | Impact | Effort | Fix |
|---|---|---|---|---|
| 11 | **Skills from files** — ปัจจุบัน hardcoded | 🔥 | กลาง | Load from ~/.tua-rs/skills/ directory |
| 12 | **Multi-model** — ปัจจุบัน hardcode 9Router | 🔥🔥 | กลาง | Support multiple providers via config |
| 13 | **Session persistence** — ไม่ save history | 🔥🔥 | สูง | JSONL session storage เหมือน Python |
| 14 | **REPL mode** — ไม่มี interactive mode | 🔥 | กลาง | Readline loop without TUI |
| 15 | **GitHub Actions CI** | 🔥 | ต่ำ | CI pipeline with test + clippy + build |
| 16 | **Release binary** | 🔥🔥 | ต่ำ | GitHub Releases with pre-built binaries |

---

## 📊 Priority Matrix

```
ผลกระทบสูง ▲
          │ #4 agent+TUI    #1 tests
          │ #6 error handling
          │
          │ #3 clippy    #2 diff viewer
          │ #5 prompt    #10 validation
          │
          │ #12 providers  #13 sessions
          │ #14 REPL       #8 benches
          │ #15 CI         #16 releases
──────────┼──────────────────────────► ความง่าย
          ต่ำ                              สูง
```

## 🎯 Recommended Sprint (2-3 hours)

```
Phase 1 (30 min):
  ├── #3  Fix 7 clippy warnings
  ├── #5  Expand system prompt to 500+ lines
  └── #10 Add config validation

Phase 2 (60 min):
  ├── #1  Add 100+ tests (agent, provider, tools)
  ├── #2  Add TUI diff viewer (#8)
  └── #6  Replace unwrap() with proper errors

Phase 3 (60 min):
  ├── #4  Wire agent harness → TUI
  ├── #15 GitHub Actions CI
  └── #9  Write GUIDE.md
```

## 📈 After Improvements

| Metric | Now | Target |
|---|---|---|
| Tests | 220 | **500+** |
| Clippy | 7 warnings | **0** |
| Features | 18/19 | **19/19** |
| System prompt | 245 lines | **500+** |
| Error handling | some unwrap() | **zero unwrap** |
| CI/CD | ❌ | ✅ |
| Release binary | ❌ | ✅ |

## 🏁 Ultimate Goal

```
Python: 19/19 features, 795 tests, mature
Rust:   19/19 features, 500+ tests, production-ready
        + 26x faster startup
        + 10x less memory
        + single binary deploy
        + compile-time safety
```
