# Implement TUI Diff Viewer — src/tui.rs

Read src/tui.rs first. Add a diff viewer that shows file changes in the TUI.

## Requirements

1. **Track file edits** — when agent modifies a file, capture before/after content
2. **`/diff` command** — show summary of all edits in current tab
3. **`/diff <n>`** — show full unified diff for edit #n
4. **`/diff last`** — show most recent edit

## Implementation

Add to `App` struct:
```rust
pub struct FileEdit {
    pub path: String,
    pub before: String,
    pub after: String,
    pub timestamp: String,
}
pub edits: Vec<FileEdit>,
```

Add to App impl:
```rust
fn record_edit(&mut self, path: &str, before: &str, after: &str) { ... }
fn show_diff(&mut self, arg: &str) -> String { ... }
```

Add to slash command handler:
```rust
"/diff" => self.show_diff(""),
"/diff last" => self.show_diff("last"),
// "/diff <n>" for specific edit number
```

## Simple diff algorithm:
- Split before/after into lines
- Show `+added` lines in green, `-removed` lines in red
- Show context lines unchanged

## Tests
- test_record_file_edit
- test_diff_empty
- test_diff_single_edit
- test_diff_last

Run `cargo test tui` and fix errors. Use Write tool.
