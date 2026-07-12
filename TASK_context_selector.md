# Smart Context Selector — Design

## Problem
Vault grows over time → context window overflow for small models.

## Solution: Relevance-Based Selection

### Algorithm
```rust
fn select_context(task: &str, vault: &Path, max_tokens: usize) -> String {
    // 1. ALWAYS include: INDEX.md + rust-do.md + rust-dont.md (rules never change)
    // 2. ERROR MATCH: if task mentions error code → include that error file
    // 3. KEYWORD MATCH: if task mentions module name → include recent sessions mentioning it
    // 4. RECENT SESSIONS: include last 3 sessions only
    // 5. TRUNCATE: if still over limit, trim oldest session logs first
}
```

### Priority Order (what gets included first)
```
1. PROJECT.md           — always (project structure)
2. rust-do.md           — always (coding rules)
3. rust-dont.md         — always (anti-patterns)
4. Error match          — if task has error code
5. Recent sessions (3)  — last 3 only
6. INDEX.md             — low priority (lookup only)
```

### Token Budget by Model
| Model | Max Context Tokens | Budget for Vault |
|---|---|---|
| deepseek-v4-flash | 128K | 50K |
| glm-5.2 | 1M | 500K |
| gpt-5.5 | 272K | 100K |
