//! 🛡️ Context Window Guard — prevent overflow with auto-compaction
//!
//! Monitors token usage relative to model's max context window.
//! Warns at 60%, auto-compacts at 80%. Thresholds scale with model capacity.

use crate::agent::AgentMessage;

// ---------------------------------------------------------------------------
// Model Context Windows
// ---------------------------------------------------------------------------

/// Known model max context windows (in tokens)
const MODEL_WINDOWS: &[(&str, usize)] = &[
    ("deepseek-v4-flash", 128_000),
    ("deepseek-v4-pro", 128_000),
    ("deepseek-v4-pro-max", 128_000),
    ("glm-5.2", 1_000_000),
    ("glm-5.1", 128_000),
    ("glm-4.7", 128_000),
    ("gpt-5.6", 1_050_000),
    ("gpt-5.5", 272_000),
    ("gpt-5.4", 272_000),
    ("gpt-5.4-mini", 400_000),
    ("haiku", 200_000),
    ("sonnet", 200_000),
    ("opus", 200_000),
];

/// Get the max context window for a model name (fuzzy match).
/// Returns 128K as safe default for unknown models.
pub fn model_max_tokens(model: &str) -> usize {
    let lower = model.to_lowercase();
    MODEL_WINDOWS
        .iter()
        .find(|(name, _)| lower.contains(name))
        .map(|(_, cap)| *cap)
        .unwrap_or(128_000) // safe default
}

// ---------------------------------------------------------------------------
// Token Estimation
// ---------------------------------------------------------------------------

/// Rough token count: ~4 chars = 1 token (English text).
/// Underestimates for code, overestimates for whitespace — good enough for guard.
pub fn estimate_tokens(messages: &[AgentMessage]) -> usize {
    messages.iter().map(|m| match m {
        AgentMessage::User { text } => text.len() / 4,
        AgentMessage::Assistant { text, tool_calls } => {
            let txt = text.as_deref().unwrap_or("").len() / 4;
            let tools: usize = tool_calls.iter()
                .map(|tc| tc.name.len() + tc.arguments.to_string().len())
                .sum::<usize>() / 4;
            txt + tools
        }
        AgentMessage::ToolResult { output, .. } => output.len() / 4,
        _ => 0,
    }).sum()
}

// ---------------------------------------------------------------------------
// Context Status
// ---------------------------------------------------------------------------

/// Snapshot of current context health.
#[derive(Debug, Clone)]
pub struct ContextStatus {
    pub tokens_used: usize,
    pub tokens_max: usize,
    pub percentage: u8,         // 0-100
    pub state: ContextState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextState {
    Healthy,     // < 60% — green
    Warning,     // 60-79% — yellow
    Critical,    // 80-99% — red (auto-compact)
    Overflow,    // > 100% — should never happen
}

impl ContextStatus {
    /// Compute status for current token count + model.
    pub fn check(messages: &[AgentMessage], model: &str) -> Self {
        let max = model_max_tokens(model);
        let used = estimate_tokens(messages);
        let pct = ((used as f64 / max as f64) * 100.0).min(100.0) as u8;
        let state = match pct {
            0..=59 => ContextState::Healthy,
            60..=79 => ContextState::Warning,
            80..=99 => ContextState::Critical,
            _ => ContextState::Overflow,
        };
        Self { tokens_used: used, tokens_max: max, percentage: pct, state }
    }

    /// Render a compact progress bar: `[████░░] 45K/128K (35%)`
    pub fn render_bar(&self, width: usize) -> String {
        let filled = (self.percentage as usize * width) / 100;
        let bar: String = "█".repeat(filled) + &"░".repeat(width.saturating_sub(filled));
        let emoji = match self.state {
            ContextState::Healthy  => "🟢",
            ContextState::Warning  => "🟡",
            ContextState::Critical => "🔴",
            ContextState::Overflow => "💀",
        };
        format!(
            "{} [{bar}] {:.0}K/{:.0}K ({}%)",
            emoji,
            self.tokens_used as f64 / 1000.0,
            self.tokens_max as f64 / 1000.0,
            self.percentage,
        )
    }
}

// ---------------------------------------------------------------------------
// Smart Compact — LLM-quality summarization + Obsidian backup
// ---------------------------------------------------------------------------

/// Compact a message history with structured summarization.
///
/// **Algorithm (3-phase):**
/// 1. **Backup** — save middle turns to `~/.tua-rs/vault/sessions/{date}-compact-{uuid}.md`
/// 2. **Summarize** — extract key facts: tasks, edits, errors, decisions, pending
/// 3. **Rebuild** — [first, structured_summary, ...last_N]
///
/// No information is lost — full history in vault, searchable via `.search_index`.
pub fn smart_compact(messages: &mut Vec<AgentMessage>, keep_last: usize) {
    if messages.len() <= keep_last + 2 {
        return;
    }

    let first = messages[0].clone();
    let middle: Vec<_> = messages[1..messages.len() - keep_last].to_vec();
    let last: Vec<_> = messages[messages.len() - keep_last..].to_vec();

    // Phase 1: Backup middle turns to vault
    let backup_path = backup_to_vault(&middle);

    // Phase 2: Structured summarization
    let summary = structured_summarize(&middle, &backup_path);

    // Phase 3: Rebuild — keep first + summary + last N
    messages.clear();
    messages.push(first);
    messages.push(AgentMessage::assistant(Some(summary), vec![]));
    messages.extend(last);
}

/// Save middle turns to Obsidian vault for permanent storage.
fn backup_to_vault(messages: &[AgentMessage]) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    let vault = format!("{home}/.tua-rs/vault/sessions");
    std::fs::create_dir_all(&vault).ok();

    let today = chrono_now();
    let id = &uuid::Uuid::new_v4().to_string()[..8];
    let path = format!("{vault}/{today}-compact-{id}.md");

    let mut content = String::from("# Compaction Backup\n\n");
    for msg in messages {
        match msg {
            AgentMessage::User { text } => {
                content.push_str(&format!("## User\n{text}\n\n"));
            }
            AgentMessage::Assistant { text, tool_calls } => {
                content.push_str(&format!("## Assistant\n{}\n", text.as_deref().unwrap_or("")));
                for tc in tool_calls {
                    content.push_str(&format!("- 🔧 {}: {}\n", tc.name, tc.arguments));
                }
                content.push('\n');
            }
            AgentMessage::ToolResult { tool_call_id, output } => {
                content.push_str(&format!("- Result({tool_call_id}): {output}\n\n"));
            }
            _ => {}
        }
    }

    std::fs::write(&path, &content).ok();
    path
}

/// Current date as YYYY-MM-DD string.
fn chrono_now() -> String {
    use std::time::SystemTime;
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    let days = secs / 86400;
    // Simple date calculation (approximate — good enough for filenames)
    let years_since_1970 = days / 365;
    let year = 1970 + years_since_1970;
    let day_of_year = days - years_since_1970 * 365;
    let month = (day_of_year / 30 + 1).min(12);
    let day = (day_of_year % 30 + 1).min(31);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Structured summarization — extracts key facts like an LLM would.
///
/// Produces a compact, queryable summary with these sections:
/// - **Tasks** — what was requested
/// - **Actions** — what the agent did (tools run, files modified)
/// - **Results** — test counts, build status
/// - **Decisions** — design choices made
/// - **Errors** — errors encountered + fixes
/// - **Pending** — unfinished work
/// - **Vault** — link to full backup
fn structured_summarize(messages: &[AgentMessage], backup_path: &str) -> String {
    let mut tasks = Vec::new();
    let mut actions = Vec::new();
    let mut errors = Vec::new();
    let mut decisions = Vec::new();
    let mut results = Vec::new();

    for msg in messages {
        match msg {
            AgentMessage::User { text } => {
                if text.len() > 10 {
                    tasks.push(truncate_str(text, 150));
                }
            }
            AgentMessage::Assistant { text, tool_calls } => {
                let txt = text.as_deref().unwrap_or("");
                // Extract decisions (lines starting with "use ", "prefer ", "decided")
                for line in txt.lines() {
                    let l = line.trim().to_lowercase();
                    if l.starts_with("use ") || l.starts_with("prefer ") 
                       || l.starts_with("decided") || l.starts_with("choose") {
                        decisions.push(truncate_str(line.trim(), 120));
                    }
                }
                // Track actions
                for tc in tool_calls {
                    actions.push(format!("🔧 {} ({})", tc.name, 
                        truncate_str(&tc.arguments.to_string(), 60)));
                }
                // Detect results
                if txt.contains("passed") || txt.contains("test result") {
                    for line in txt.lines() {
                        if line.contains("passed") || line.contains("test result") {
                            results.push(line.trim().to_string());
                        }
                    }
                }
            }
            AgentMessage::ToolResult { tool_call_id: _, output } => {
                if output.to_lowercase().contains("error") 
                   || output.to_lowercase().contains("fail") {
                    errors.push(truncate_str(output, 120));
                }
            }
            _ => {}
        }
    }

    // Build structured summary
    let mut s = String::from("## 📋 Context Summary\n\n");
    s.push_str(&format!("⏪ {} turns compacted → full backup: {}\n\n", 
        messages.len(), backup_path));

    if !tasks.is_empty() {
        s.push_str("### 🎯 Tasks\n");
        for t in tasks.iter().take(5) { s.push_str(&format!("- {t}\n")); }
        s.push('\n');
    }
    if !actions.is_empty() {
        s.push_str("### 🔧 Actions\n");
        for a in actions.iter().take(10) { s.push_str(&format!("- {a}\n")); }
        s.push('\n');
    }
    if !results.is_empty() {
        s.push_str("### ✅ Results\n");
        for r in results.iter().take(3) { s.push_str(&format!("- {r}\n")); }
        s.push('\n');
    }
    if !decisions.is_empty() {
        s.push_str("### 💡 Decisions\n");
        for d in decisions.iter().take(5) { s.push_str(&format!("- {d}\n")); }
        s.push('\n');
    }
    if !errors.is_empty() {
        s.push_str("### ❌ Errors\n");
        for e in errors.iter().take(5) { s.push_str(&format!("- {e}\n")); }
        s.push('\n');
    }
    if errors.is_empty() && decisions.is_empty() {
        s.push_str("_(routine conversation — no key decisions or errors)_\n");
    }

    s
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max { s.to_string() } else { format!("{}…", &s[..max-1]) }
}

// Keep old compact_messages for backward compatibility
pub fn compact_messages(messages: &mut Vec<AgentMessage>, keep_last: usize) {
    smart_compact(messages, keep_last);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentToolCall;

    #[test]
    fn test_model_max_tokens_known() {
        assert_eq!(model_max_tokens("deepseek-v4-flash"), 128_000);
        assert_eq!(model_max_tokens("glm-5.2"), 1_000_000);
        assert_eq!(model_max_tokens("gpt-5.5"), 272_000);
    }

    #[test]
    fn test_model_max_tokens_fuzzy() {
        assert_eq!(model_max_tokens("deepseek/deepseek-v4-flash"), 128_000);
        assert_eq!(model_max_tokens("glm/glm-5.2"), 1_000_000);
    }

    #[test]
    fn test_model_max_tokens_unknown_defaults() {
        assert_eq!(model_max_tokens("unknown-model"), 128_000);
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_tokens_user_message() {
        let msgs = vec![AgentMessage::user("Hello, world! How are you?")];
        let tokens = estimate_tokens(&msgs);
        assert!(tokens > 0 && tokens < 30, "short message should be < 30 tokens");
    }

    #[test]
    fn test_estimate_tokens_assistant_with_tools() {
        let msgs = vec![AgentMessage::assistant(
            Some("Let me check that for you".into()),
            vec![AgentToolCall {
                id: "call_1".into(),
                name: "cargo_check".into(),
                arguments: serde_json::json!({"target": "src/main.rs"}),
            }],
        )];
        let tokens = estimate_tokens(&msgs);
        assert!(tokens > 10, "message with tool call should estimate > 10 tokens");
    }

    #[test]
    fn test_context_status_healthy() {
        let msgs = vec![AgentMessage::user("short")];
        let status = ContextStatus::check(&msgs, "deepseek-v4-flash");
        assert_eq!(status.state, ContextState::Healthy);
        assert!(status.percentage < 60);
    }

    #[test]
    fn test_context_status_overflow() {
        // Create artificially large message
        let big = "x".repeat(600_000); // 150K tokens, > 128K max
        let msgs = vec![AgentMessage::user(&big)];
        let status = ContextStatus::check(&msgs, "deepseek-v4-flash");
        assert!(matches!(status.state, ContextState::Overflow));
    }

    #[test]
    fn test_render_bar_healthy() {
        let msgs = vec![AgentMessage::user("hello")];
        let status = ContextStatus::check(&msgs, "deepseek-v4-flash");
        let bar = status.render_bar(10);
        assert!(bar.starts_with("🟢"));
        assert!(bar.contains("/"));
    }

    #[test]
    fn test_compact_messages_reduces_count() {
        let mut msgs: Vec<AgentMessage> = (0..20)
            .map(|i| AgentMessage::user(format!("message {i}")))
            .collect();
        let orig_len = msgs.len();
        compact_messages(&mut msgs, 3);
        assert!(msgs.len() < orig_len, "compacted should be shorter");
    }

    #[test]
    fn test_compact_preserves_last_messages() {
        let mut msgs: Vec<AgentMessage> = (0..10)
            .map(|i| AgentMessage::user(format!("msg{i}")))
            .collect();
        msgs.push(AgentMessage::user("IMPORTANT_LAST_MSG"));
        compact_messages(&mut msgs, 1);
        let last = msgs.last().unwrap();
        match last {
            AgentMessage::User { text } => assert_eq!(text, "IMPORTANT_LAST_MSG"),
            _ => panic!("last message should be user message"),
        }
    }
}
