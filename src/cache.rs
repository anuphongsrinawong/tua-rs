//! 💾 Prompt Caching — reduce token costs by caching static content
//!
//! OpenAI/Anthropic APIs support prompt caching: mark portions of the prompt
//! as cacheable, and subsequent requests with the same prefix get discounted
//! cache-read pricing (~90% cheaper).
//!
//! ## Strategy
//! - System prompt (1,148 lines) → ALWAYS cached (never changes)
//! - Tool definitions (14 schemas) → cached (change rarely)
//! - Message history → cache prefix (unchanged history gets cache hits)
//! - User's last message → NOT cached (changes every turn)

use crate::agent::AgentMessage;

/// Breakpoint marker for API-level prompt caching.
/// Insert this before cacheable content blocks.
pub const CACHE_BREAKPOINT: &str = "__CACHE_BP__";

/// Cache statistics for monitoring savings.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total input tokens sent (uncached + cached reads)
    pub total_input: u64,
    /// Tokens read from cache (billed at ~10% of normal input cost)
    pub cache_hits: u64,
    /// Tokens written to cache (billed at normal input cost)
    pub cache_writes: u64,
    /// Estimated savings from caching
    pub estimated_savings_tokens: u64,
}

impl CacheStats {
    /// Record a request's token usage.
    pub fn record(&mut self, input: u64, cache_read: u64, cache_write: u64) {
        self.total_input = self.total_input.saturating_add(input);
        self.cache_hits = self.cache_hits.saturating_add(cache_read);
        self.cache_writes = self.cache_writes.saturating_add(cache_write);
        // Cache reads are ~10% of normal input cost → 90% savings
        self.estimated_savings_tokens = self.estimated_savings_tokens
            .saturating_add((cache_read as f64 * 0.9) as u64);
    }

    /// Cache hit ratio (0.0 - 1.0).
    pub fn hit_ratio(&self) -> f64 {
        if self.total_input == 0 { 0.0 }
        else { self.cache_hits as f64 / self.total_input as f64 }
    }

    /// Human-readable savings report.
    pub fn report(&self) -> String {
        format!(
            "💾 Cache: {:.0}% hit rate | {}K saved | {}K input → {}K cached",
            self.hit_ratio() * 100.0,
            self.estimated_savings_tokens / 1000,
            self.total_input / 1000,
            self.cache_hits / 1000,
        )
    }
}

/// Build a cache-optimized message array for the API.
///
/// Places cache breakpoints before static content so the provider
/// can cache and reuse it across requests.
pub fn build_cached_messages(
    system_prompt: &str,
    history: &[AgentMessage],
    tools_json: &str,
) -> Vec<serde_json::Value> {
    let mut messages: Vec<serde_json::Value> = Vec::new();

    // System prompt — always cacheable (never changes during a session)
    messages.push(serde_json::json!({
        "role": "system",
        "content": [
            {"type": "text", "text": system_prompt, "cache_control": {"type": "ephemeral"}}
        ]
    }));

    // Message history — prefix is cacheable
    for (i, msg) in history.iter().enumerate() {
        let is_last = i == history.len() - 1;
        let cache_control = if !is_last {
            // Cacheable: this message won't change in future requests
            Some(serde_json::json!({"type": "ephemeral"}))
        } else {
            // Last message changes each turn → don't cache
            None
        };

        match msg {
            AgentMessage::User { text } => {
                let mut content = serde_json::json!({
                    "role": "user",
                    "content": text,
                });
                if let Some(cc) = cache_control {
                    content["cache_control"] = cc;
                }
                messages.push(content);
            }
            AgentMessage::Assistant { text, tool_calls } => {
                let txt = text.as_deref().unwrap_or("");
                if tool_calls.is_empty() {
                    let mut content = serde_json::json!({
                        "role": "assistant",
                        "content": txt,
                    });
                    if let Some(cc) = cache_control {
                        content["cache_control"] = cc;
                    }
                    messages.push(content);
                } else {
                    // Split: content as text, then tool_calls separately
                    let mut content = serde_json::json!({
                        "role": "assistant",
                        "content": txt,
                    });
                    if let Some(cc) = cache_control.as_ref() {
                        content["cache_control"] = cc.clone();
                    }
                    messages.push(content);

                    let mut tc_msg = serde_json::json!({
                        "role": "assistant",
                        "tool_calls": tool_calls.iter().map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.arguments.to_string(),
                                }
                            })
                        }).collect::<Vec<_>>(),
                    });
                    if let Some(cc) = cache_control {
                        tc_msg["cache_control"] = cc;
                    }
                    messages.push(tc_msg);
                }
            }
            AgentMessage::ToolResult { tool_call_id, output } => {
                let mut content = serde_json::json!({
                    "role": "tool",
                    "tool_call_id": tool_call_id,
                    "content": output,
                });
                if let Some(cc) = cache_control {
                    content["cache_control"] = cc;
                }
                messages.push(content);
            }
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentToolCall;

    #[test]
    fn test_cache_stats_recording() {
        let mut stats = CacheStats::default();
        stats.record(1000, 800, 200);
        assert_eq!(stats.total_input, 1000);
        assert_eq!(stats.cache_hits, 800);
        assert_eq!(stats.cache_writes, 200);
        assert!(stats.hit_ratio() > 0.7);
    }

    #[test]
    fn test_cache_stats_empty() {
        let stats = CacheStats::default();
        assert_eq!(stats.hit_ratio(), 0.0);
    }

    #[test]
    fn test_build_cached_messages_empty() {
        let msgs = build_cached_messages("system", &[], "");
        assert_eq!(msgs.len(), 1); // system prompt only
        assert!(msgs[0]["content"][0]["cache_control"]["type"] == "ephemeral");
    }

    #[test]
    fn test_build_cached_messages_last_not_cached() {
        let history = vec![
            AgentMessage::user("hello"),
            AgentMessage::assistant(Some("hi".into()), vec![]),
            AgentMessage::user("latest"), // last message — should NOT be cached
        ];
        let msgs = build_cached_messages("sys", &history, "");
        // system + user1 + assistant1 + user2(last) = 4 messages
        assert_eq!(msgs.len(), 4);
        // Last message should NOT have cache_control
        let last = &msgs[3];
        assert!(last.get("cache_control").is_none(), "last message should not be cached");
    }

    #[test]
    fn test_report_format() {
        let mut stats = CacheStats::default();
        stats.record(10000, 8000, 2000);
        let report = stats.report();
        assert!(report.contains("80%"));
        assert!(report.contains("Cache"));
    }
}
