//! 🧠 Memory Hierarchy — Short/Working/Long-term memory
//!
//! Three-tier memory system that prevents context window overflow
//! and preserves critical information across long sessions.
//!
//! - **Short-term**: Last 5 turns (detailed, auto-managed by agent loop)
//! - **Working**: Current task goal + key files (injected top of prompt)
//! - **Long-term**: Vault-based knowledge (pattern-matched on task keyword)

use std::collections::HashMap;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Working Memory
// ---------------------------------------------------------------------------

/// Working memory — a compact summary of the current task context.
///
/// Always injected at the top of the system prompt so the model
/// never loses track of the primary goal, even after many turns.
#[derive(Debug, Clone, Default)]
pub struct WorkingMemory {
    /// One-line summary of the current goal.
    pub goal: String,
    /// Key files being modified (with line counts).
    pub files: Vec<FileContext>,
    /// Recent decisions (keep last 3).
    pub decisions: Vec<String>,
    /// Open questions or blockers.
    pub blockers: Vec<String>,
    /// Current branch/workspace context.
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileContext {
    pub path: String,
    pub lines: usize,
    pub last_action: String, // "added tests", "fixed clippy", etc.
}

impl WorkingMemory {
    pub fn new(goal: &str) -> Self {
        Self {
            goal: goal.to_string(),
            ..Default::default()
        }
    }

    /// Record a file modification.
    pub fn touch(&mut self, path: &str, lines: usize, action: &str) {
        if let Some(fc) = self.files.iter_mut().find(|f| f.path == path) {
            fc.lines = lines;
            fc.last_action = action.to_string();
        } else {
            self.files.push(FileContext {
                path: path.to_string(),
                lines,
                last_action: action.to_string(),
            });
        }
        // Keep only last 5 files
        self.files.truncate(5);
    }

    /// Record a design decision.
    pub fn decide(&mut self, decision: &str) {
        self.decisions.push(decision.to_string());
        self.decisions.truncate(3);
    }

    /// Mark a blocker (something preventing progress).
    pub fn block(&mut self, reason: &str) {
        self.blockers.push(reason.to_string());
    }

    /// Clear blockers once resolved.
    pub fn unblock(&mut self) {
        self.blockers.clear();
    }

    /// Render working memory as a prompt prefix.
    pub fn render(&self) -> String {
        if self.goal.is_empty() && self.files.is_empty() {
            return String::new();
        }
        let mut out = String::from("## 🧠 Working Memory (current task context)\n\n");
        if !self.goal.is_empty() {
            out.push_str(&format!("**Goal:** {}\n\n", self.goal));
        }
        if !self.files.is_empty() {
            out.push_str("**Files:**\n");
            for f in &self.files {
                out.push_str(&format!(
                    "- `{}` ({} lines, {})\n",
                    f.path, f.lines, f.last_action
                ));
            }
            out.push('\n');
        }
        if !self.decisions.is_empty() {
            out.push_str("**Decisions:**\n");
            for d in &self.decisions {
                out.push_str(&format!("- {d}\n"));
            }
            out.push('\n');
        }
        if !self.blockers.is_empty() {
            out.push_str("**⚠️ Blockers:**\n");
            for b in &self.blockers {
                out.push_str(&format!("- {b}\n"));
            }
            out.push('\n');
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Long-term Memory (Vault integration)
// ---------------------------------------------------------------------------

/// Long-term memory backed by the Obsidian vault.
///
/// Stores code patterns, bug fixes, and project knowledge
/// that persists across sessions. Uses keyword matching to
/// retrieve relevant entries for the current task.
pub struct LongTermMemory {
    vault: PathBuf,
    /// Cached pattern index for fast lookup.
    index: HashMap<String, Vec<String>>,
}

impl LongTermMemory {
    pub fn new(vault_path: PathBuf) -> Self {
        Self {
            vault: vault_path,
            index: HashMap::new(),
        }
    }

    /// Index all vault files for keyword search.
    pub fn index_all(&mut self) {
        let patterns_dir = self.vault.join("rules");
        if let Ok(entries) = std::fs::read_dir(&patterns_dir) {
            for entry in entries.flatten() {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let keywords: Vec<String> = content
                        .lines()
                        .filter(|l| {
                            l.contains("use ") || l.contains("prefer ") || l.contains("never ")
                        })
                        .map(|l| {
                            l.trim()
                                .trim_start_matches("use ")
                                .trim_start_matches("prefer ")
                                .trim_start_matches("never ")
                                .to_string()
                        })
                        .collect();
                    self.index.insert(name, keywords);
                }
            }
        }
    }

    /// Query long-term memory for relevant patterns.
    pub fn query(&self, task: &str, max_results: usize) -> Vec<String> {
        let mut results = Vec::new();
        let lower = task.to_lowercase();
        for (file, keywords) in &self.index {
            let hits: usize = keywords
                .iter()
                .filter(|kw| lower.contains(&kw.to_lowercase()))
                .count();
            if hits > 0 {
                results.push((hits, file.clone()));
            }
        }
        results.sort_by_key(|(h, _)| -(*h as i32));
        results.truncate(max_results);
        results.into_iter().map(|(_, f)| f).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_working_memory_render_empty() {
        let wm = WorkingMemory::default();
        assert!(wm.render().is_empty());
    }

    #[test]
    fn test_working_memory_render_with_goal() {
        let wm = WorkingMemory::new("Add tests to parallel.rs");
        let rendered = wm.render();
        assert!(rendered.contains("Add tests to parallel.rs"));
    }

    #[test]
    fn test_touch_adds_file_context() {
        let mut wm = WorkingMemory::new("Refactor session module");
        wm.touch("src/session.rs", 350, "added tests");
        assert_eq!(wm.files.len(), 1);
        assert_eq!(wm.files[0].path, "src/session.rs");
    }

    #[test]
    fn test_touch_updates_existing() {
        let mut wm = WorkingMemory::new("Add tests");
        wm.touch("src/session.rs", 100, "initial test");
        wm.touch("src/session.rs", 350, "added more tests");
        assert_eq!(wm.files.len(), 1);
        assert_eq!(wm.files[0].lines, 350);
    }

    #[test]
    fn test_decide_truncates() {
        let mut wm = WorkingMemory::new("test");
        for i in 0..5 {
            wm.decide(&format!("decision {i}"));
        }
        assert_eq!(wm.decisions.len(), 3); // truncated to 3
    }

    #[test]
    fn test_block_and_unblock() {
        let mut wm = WorkingMemory::new("test");
        wm.block("Need to understand borrow checker error");
        assert_eq!(wm.blockers.len(), 1);
        wm.unblock();
        assert!(wm.blockers.is_empty());
    }
}
