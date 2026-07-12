//! 🧠 Smart Context Engine — 5-level context optimization
//!
//! Level 2: Session Summarization — auto-compress old sessions to key facts
//! Level 4: Code Chunking — only include modules relevant to the task
//! Level 5: Keyword Relevance — TF-IDF-lite search for best matches
//! Level 6: Hierarchical Dispatch — orchestrator compresses context for workers
//! Level 7: Persistent Search Index — cached relevance scores

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Level 2: Session Summarization
// ---------------------------------------------------------------------------

/// Extract key facts from a session log, keeping only actionable lessons.
/// Reduces ~800 bytes to ~200 bytes (75% compression).
pub fn summarize_session(content: &str) -> String {
    let mut summary = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        // Keep only lines with actionable info
        if trimmed.starts_with("## ")
            || trimmed.starts_with("### ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.starts_with("1. ")
            || trimmed.starts_with("✅")
            || trimmed.starts_with("❌")
            || trimmed.starts_with("🔥")
        {
            summary.push_str(line);
            summary.push('\n');
        }
    }

    if summary.is_empty() {
        // Fallback: first 3 non-empty lines
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        summary
    }
}

// ---------------------------------------------------------------------------
// Level 4: Code Chunking
// ---------------------------------------------------------------------------

/// Module info extracted from PROJECT.md or vault INDEX.md
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    pub name: String,
    pub path: String,
    pub description: String,
}

/// Parse module list from PROJECT.md content (e.g., "tools(79t) session(26t)")
pub fn parse_modules(project_md: &str) -> Vec<ModuleInfo> {
    project_md
        .lines()
        .filter(|l| l.contains('(') && l.contains("t)"))
        .flat_map(|line| {
            line.split_whitespace().filter_map(|word| {
                if let Some((name, rest)) = word.split_once('(') {
                    Some(ModuleInfo {
                        name: name.to_string(),
                        path: format!("src/{}.rs", name),
                        description: rest.trim_end_matches(')').to_string(),
                    })
                } else {
                    None
                }
            })
        })
        .collect()
}

/// Select modules relevant to the task by name matching
pub fn chunk_by_task<'a>(task: &str, modules: &'a [ModuleInfo], max: usize) -> Vec<&'a ModuleInfo> {
    let task_lower = task.to_lowercase();
    let mut scored: Vec<(i32, &ModuleInfo)> = modules
        .iter()
        .map(|m| {
            let score = if task_lower.contains(&m.name) {
                10
            } else if task_lower.contains(&m.name.replace('-', "")) {
                8
            } else if m.description.contains(&task_lower) {
                5
            } else {
                0
            };
            (score, m)
        })
        .filter(|(s, _)| *s > 0)
        .collect();

    scored.sort_by_key(|(s, _)| -s);
    scored.truncate(max);
    scored.into_iter().map(|(_, m)| m).collect()
}

// ---------------------------------------------------------------------------
// Level 5: Keyword Relevance (TF-IDF-lite)
// ---------------------------------------------------------------------------

/// Simple keyword-based relevance score for a document against a query.
/// TF = term frequency in doc, IDF = inverse doc frequency in corpus.
#[derive(Default)]
pub struct RelevanceIndex {
    /// Map: word → (doc_count_containing_word)
    idf: HashMap<String, usize>,
    /// Total documents indexed
    doc_count: usize,
}

impl RelevanceIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a set of documents for later relevance queries
    pub fn index(&mut self, docs: &[(&str, &str)]) {
        self.doc_count = docs.len();
        for (_, content) in docs {
            let words: std::collections::HashSet<_> = tokenize(content).into_iter().collect();
            for w in words {
                *self.idf.entry(w).or_default() += 1;
            }
        }
    }

    /// Score how relevant a document is to a query (0.0 to 1.0)
    pub fn score(&self, query: &str, doc: &str) -> f64 {
        let query_words = tokenize(query);
        let doc_words: Vec<String> = tokenize(doc);
        if query_words.is_empty() || self.doc_count == 0 {
            return 0.0;
        }

        let mut total = 0.0;
        for qw in &query_words {
            let tf = doc_words.iter().filter(|dw| *dw == qw).count() as f64;
            if tf > 0.0 {
                let df = *self.idf.get(qw).unwrap_or(&1) as f64;
                let idf = ((self.doc_count as f64 + 1.0) / (df + 1.0)).ln() + 1.0;
                total += tf * idf;
            }
        }
        total / query_words.len() as f64
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let mut index = Self::new();
        for line in content.lines() {
            if let Some((word, count)) = line.split_once(':') {
                index
                    .idf
                    .insert(word.to_string(), count.parse().unwrap_or(1));
            }
        }
        index.doc_count = 100; // approximate
        Ok(index)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut content = String::new();
        for (word, count) in &self.idf {
            content.push_str(&format!("{word}:{count}\n"));
        }
        std::fs::write(path, content)
    }
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Level 6: Hierarchical Dispatch
// ---------------------------------------------------------------------------

/// Compress full context into a worker-friendly summary.
/// Orchestrator reads full vault → workers get only compressed version.
pub fn compress_for_worker(
    task: &str,
    project_md: &str,
    rules: &[String],
    error_matches: &[String],
    sessions: &[String],
    max_chars: usize,
) -> String {
    let mut ctx = String::new();
    let mut remaining = max_chars;

    // Always include: compressed project info (just module list)
    let modules = parse_modules(project_md);
    let relevant = chunk_by_task(task, &modules, 5);
    if !relevant.is_empty() {
        let mod_list: Vec<String> = relevant
            .iter()
            .map(|m| format!("{}({})", m.name, m.description))
            .collect();
        let line = format!("## Modules\n{}\n\n", mod_list.join(", "));
        ctx.push_str(&line);
        remaining = remaining.saturating_sub(line.len());
    }

    // Always include: rules (summarized)
    for rule in rules {
        let summarized = summarize_session(rule);
        if summarized.len() <= remaining {
            ctx.push_str(&format!("## Rules\n{summarized}\n\n"));
            remaining = remaining.saturating_sub(summarized.len() + 20);
        } else {
            break;
        }
    }

    // Include: matched errors
    for err in error_matches.iter().take(3) {
        if err.len() <= remaining {
            ctx.push_str(&format!("## Errors\n{err}\n\n"));
            remaining = remaining.saturating_sub(err.len() + 20);
        }
    }

    // Include: summarized sessions (last 2, compressed)
    for session in sessions.iter().take(2) {
        let summarized = summarize_session(session);
        if summarized.len() <= remaining {
            ctx.push_str(&format!("## History\n{summarized}\n\n"));
            remaining = remaining.saturating_sub(summarized.len() + 20);
        }
    }

    // Task prompt always fits
    if task.len() > remaining {
        ctx.push_str(&task[..remaining]);
    } else {
        ctx.push_str(task);
    }

    ctx
}

// ---------------------------------------------------------------------------
// Level 7: Persistent Search Index
// ---------------------------------------------------------------------------

/// Maintain a cached search index at ~/.tua-rs/vault/.search_index
pub struct SearchCache {
    pub index: RelevanceIndex,
    pub path: PathBuf,
}

impl SearchCache {
    pub fn load_or_create(vault_path: &Path) -> Self {
        let path = vault_path.join(".search_index");
        let index = RelevanceIndex::load(&path).unwrap_or_else(|_| {
            let mut idx = RelevanceIndex::new();
            // Index vault files on first load
            if let Ok(entries) = std::fs::read_dir(vault_path) {
                let docs: Vec<(String, String)> = entries
                    .flatten()
                    .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().to_string();
                        std::fs::read_to_string(e.path()).ok().map(|c| (name, c))
                    })
                    .collect();
                let refs: Vec<(&str, &str)> =
                    docs.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect();
                idx.index(&refs);
                idx.save(&path).ok();
            }
            idx
        });
        Self { index, path }
    }

    /// Find best matching vault files for a query (returns filenames sorted by relevance)
    pub fn search(&self, query: &str, vault_path: &Path, top_n: usize) -> Vec<String> {
        let mut results: Vec<(f64, String)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(vault_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|x| x == "md")
                    && path.file_name().is_some_and(|n| n != ".search_index")
                {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let score = self.index.score(query, &content);
                        if score > 0.0 {
                            results.push((score, entry.file_name().to_string_lossy().to_string()));
                        }
                    }
                }
            }
        }
        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_n);
        results.into_iter().map(|(_, name)| name).collect()
    }

    /// Rebuild index (call periodically or after session)
    pub fn rebuild(&mut self, vault_path: &Path) {
        let _docs: Vec<(&str, &str)> = Vec::new();
        let mut doc_strings: Vec<(String, String)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(vault_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|x| x == "md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        doc_strings
                            .push((entry.file_name().to_string_lossy().to_string(), content));
                    }
                }
            }
        }
        let refs: Vec<(&str, &str)> = doc_strings
            .iter()
            .map(|(n, c)| (n.as_str(), c.as_str()))
            .collect();
        self.index.index(&refs);
        self.index.save(&self.path).ok();
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summarize_extracts_key_lines() {
        let session = "## Achievements\n- Added TUI\n- Fixed 3 bugs\n\n## Details\nLong text here\nMore details\n## Lessons\n- Don't parallel same file\n";
        let summary = summarize_session(session);
        assert!(summary.contains("Achievements"));
        assert!(summary.contains("Added TUI"));
        assert!(summary.contains("Lessons"));
        assert!(!summary.contains("Long text")); // details filtered out
    }

    #[test]
    fn test_parse_modules_from_project() {
        let md = "tools(79t) session(26t) agent(23t) tui(18t)";
        let modules = parse_modules(md);
        assert_eq!(modules.len(), 4);
        assert_eq!(modules[0].name, "tools");
        assert_eq!(modules[1].name, "session");
    }

    #[test]
    fn test_chunk_by_task_relevance() {
        let modules = vec![
            ModuleInfo {
                name: "tools".into(),
                path: "src/tools.rs".into(),
                description: "79t".into(),
            },
            ModuleInfo {
                name: "tui".into(),
                path: "src/tui.rs".into(),
                description: "18t".into(),
            },
            ModuleInfo {
                name: "session".into(),
                path: "src/session.rs".into(),
                description: "26t".into(),
            },
        ];
        let relevant = chunk_by_task("add tests to tools and fix tui", &modules, 5);
        assert_eq!(relevant.len(), 2);
        assert_eq!(relevant[0].name, "tools"); // exact match scores higher
    }

    #[test]
    fn test_relevance_index() {
        let mut idx = RelevanceIndex::new();
        let docs = [
            ("a", "rust async tokio spawn thread"),
            ("b", "python django flask web"),
            ("c", "rust cargo clippy test"),
        ];
        let refs: Vec<(&str, &str)> = docs.iter().map(|(n, c)| (*n, *c)).collect();
        idx.index(&refs);

        let score_rust = idx.score("rust cargo", "rust async tokio spawn thread");
        let score_python = idx.score("rust cargo", "python django flask web");
        assert!(
            score_rust > score_python,
            "rust doc should score higher for rust query"
        );
    }

    #[test]
    fn test_compress_for_worker_under_budget() {
        let task = "add tests to parallel.rs";
        let project = "parallel(10t) tools(79t) session(26t)";
        let rules = vec!["- Use Result not unwrap\n- FnMut over FnOnce".to_string()];
        let result = compress_for_worker(task, project, &rules, &[], &[], 500);
        assert!(result.len() <= 500);
        assert!(result.contains("parallel"));
        assert!(result.contains("Result"));
    }

    #[test]
    fn test_tokenize() {
        let words = tokenize("cargo check --all-features");
        assert!(words.contains(&"cargo".to_string()));
        assert!(words.contains(&"check".to_string()));
        assert!(words.contains(&"--all-features".to_string()));
    }
}
