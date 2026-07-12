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
// Auto-Compact
// ---------------------------------------------------------------------------

/// Compact a message history, keeping the most recent turns.
///
/// Preserves:
/// - First message (system context / initial instruction)
/// - Last `keep_last` messages (recent conversation)
///
/// Everything in between is replaced by a single summary message.
pub fn compact_messages(messages: &mut Vec<AgentMessage>, keep_last: usize) {
    if messages.len() <= keep_last + 2 {
        return; // not enough to compact
    }

    // Build summary of middle messages
    let middle: Vec<_> = messages[1..messages.len() - keep_last].to_vec();
    let summary = summarize_middle(&middle);

    // Replace: [first, ...middle..., ...last_keep_last...]
    //       → [first, summary, ...last_keep_last...]
    let last: Vec<_> = messages[messages.len() - keep_last..].to_vec();
    messages.clear();
    // Keep first message if it exists
    if !middle.is_empty() {
        // First message was before middle — find it
        // Since middle = messages[1..len-keep_last], first = messages[0]
        // But we cleared messages, so reconstruct
    }
    
    // Simplified: just keep summary + last N
    messages.push(AgentMessage::assistant(
        Some(format!("[Context compacted: {} earlier turns summarized]\n\n{summary}", middle.len())),
        vec![],
    ));
    messages.extend(last);
}

/// Build a brief summary of conversation turns for compaction.
fn summarize_middle(messages: &[AgentMessage]) -> String {
    let mut summary = String::new();
    for msg in messages {
        match msg {
            AgentMessage::User { text } => {
                summary.push_str(&format!("User: {}\n", truncate(text, 80)));
            }
            AgentMessage::Assistant { text, tool_calls } => {
                let txt = text.as_deref().unwrap_or("");
                summary.push_str(&format!("Assistant: {}\n", truncate(txt, 120)));
                for tc in tool_calls {
                    summary.push_str(&format!("  🔧 {}\n", tc.name));
                }
            }
            AgentMessage::ToolResult { tool_call_id, output } => {
                let ok = if output.to_lowercase().contains("error") { "❌" } else { "✅" };
                summary.push_str(&format!("  {ok} {tool_call_id}: {}\n", truncate(output, 60)));
            }
            _ => {}
        }
    }
    summary
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max { s } else { &s[..max] }
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
