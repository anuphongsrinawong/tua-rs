//! 🎭 Mock provider — returns canned [`AgentEvent`] sequences for TUI testing.
//!
//! No API key, no HTTP calls, no real LLM.  Useful for developing and testing
//! the TUI without hitting a real model provider.
//!
//! # Examples
//!
//! ```rust
//! use tua_rs::agent::ModelProvider;
//! use tua_rs::providers::mock::MockProvider;
//!
//! let provider = MockProvider::with_text("Hello from mock!");
//! // Use with AgentLoop, TUI, or tests.
//! ```
//!
//! ```rust
//! use tua_rs::providers::mock::MockProviderBuilder;
//!
//! let provider = MockProviderBuilder::new()
//!     .text_delta("Hello,")
//!     .text_delta(" world!")
//!     .done()
//!     .build();
//! ```

use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use tokio::time::sleep;

use crate::agent::{
    AgentError, AgentEvent, AgentMessage, AgentResult, AgentTool, AgentToolCall, ModelProvider,
};

// ---------------------------------------------------------------------------
// MockProvider
// ---------------------------------------------------------------------------

/// A mock model provider that replays a pre-defined sequence of
/// [`AgentEvent`] values.
///
/// Each call to [`stream_chat`](MockProvider::stream_chat) returns the
/// same canned sequence, with an optional inter-event delay.
///
/// This is useful for **TUI testing**, **UI polish**, and **integration
/// tests** that must not depend on a live API.
///
/// # Warning
///
/// The mock does **not** simulate the full agent tool-call loop —
/// it only replays the events you give it.  Tool execution must be
/// exercised through the real [`AgentLoop`](crate::agent::AgentLoop) with
/// an [`EchoProvider`]-style harness if you need end-to-end tool testing.
#[derive(Debug, Clone)]
pub struct MockProvider {
    /// Canned events to replay.
    events: Vec<AgentEvent>,
    /// Delay between consecutive events.
    delay: Duration,
}

impl MockProvider {
    /// Create a mock provider that emits the given events.
    ///
    /// Uses a default delay of **50 ms** between events.
    pub fn new(events: Vec<AgentEvent>) -> Self {
        Self {
            events,
            delay: Duration::from_millis(50),
        }
    }

    /// Override the inter-event delay.
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Create a provider whose entire response is a single text string
    /// followed by [`Done`](AgentEvent::Done).
    ///
    /// This is the simplest way to test basic text rendering in the TUI.
    pub fn with_text(text: &str) -> Self {
        Self {
            events: vec![AgentEvent::TextDelta(text.to_string()), AgentEvent::Done],
            delay: Duration::from_millis(50),
        }
    }

    /// Create a provider that simulates a **tool call round**:
    ///
    /// 1. A short text prelude
    /// 2. A [`ToolCall`](AgentEvent::ToolCall) for the given tool
    /// 3. A [`ToolResult`](AgentEvent::ToolResult) with the given output
    /// 4. A closing text delta
    /// 5. [`Done`](AgentEvent::Done)
    pub fn with_tool_call(tool_name: &str, tool_result: &str) -> Self {
        Self {
            events: vec![
                AgentEvent::TextDelta(format!("Let me use {tool_name}… ")),
                AgentEvent::ToolCall(AgentToolCall {
                    id: "mock_call_1".to_string(),
                    name: tool_name.to_string(),
                    arguments: serde_json::json!({}),
                }),
                AgentEvent::ToolResult {
                    tool_call_id: "mock_call_1".to_string(),
                    output: tool_result.to_string(),
                },
                AgentEvent::TextDelta(format!("\n\nResult:\n```\n{tool_result}\n```")),
                AgentEvent::Done,
            ],
            delay: Duration::from_millis(80),
        }
    }

    /// Create a provider that simulates a **thinking/reasoning** phase
    /// before responding.
    pub fn with_thinking(thinking: &str, response: &str) -> Self {
        Self {
            events: vec![
                AgentEvent::ThinkingDelta(thinking.to_string()),
                AgentEvent::TextDelta(response.to_string()),
                AgentEvent::Done,
            ],
            delay: Duration::from_millis(30),
        }
    }

    /// Create a provider that always **errors** immediately.
    pub fn with_error(error: &str) -> Self {
        Self {
            events: vec![AgentEvent::Error(error.to_string()), AgentEvent::Done],
            delay: Duration::from_millis(10),
        }
    }
}

#[async_trait]
impl ModelProvider for MockProvider {
    async fn stream_chat(
        &self,
        _messages: Vec<AgentMessage>,
        _system: String,
        _tools: Vec<AgentTool>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>> {
        let events = self.events.clone();
        let delay = self.delay;

        // Spawn a task that replays events with the configured delay.
        let (tx, rx) = futures::channel::mpsc::unbounded();
        tokio::spawn(async move {
            for event in events {
                if !delay.is_zero() {
                    sleep(delay).await;
                }
                if tx.unbounded_send(event).is_err() {
                    break;
                }
            }
        });

        Ok(Box::pin(rx))
    }
}

// ---------------------------------------------------------------------------
// MockProviderBuilder
// ---------------------------------------------------------------------------

/// Builder for constructing a [`MockProvider`] with a fluent API.
///
/// # Example
///
/// ```rust
/// use tua_rs::providers::mock::MockProviderBuilder;
/// use tua_rs::agent::ModelProvider;
///
/// let provider = MockProviderBuilder::new()
///     .thinking_delta("Hmm, let me think…")
///     .text_delta("Here's the answer.")
///     .done()
///     .build();
/// ```
#[derive(Debug, Clone, Default)]
pub struct MockProviderBuilder {
    events: Vec<AgentEvent>,
    delay: Duration,
}

impl MockProviderBuilder {
    /// Start building a new mock provider.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the inter-event delay (default: 50 ms).
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Append a [`TextDelta`](AgentEvent::TextDelta) event.
    pub fn text_delta(mut self, text: impl Into<String>) -> Self {
        self.events.push(AgentEvent::TextDelta(text.into()));
        self
    }

    /// Append a [`ThinkingDelta`](AgentEvent::ThinkingDelta) event.
    pub fn thinking_delta(mut self, text: impl Into<String>) -> Self {
        self.events.push(AgentEvent::ThinkingDelta(text.into()));
        self
    }

    /// Append a [`ToolCall`](AgentEvent::ToolCall) event.
    pub fn tool_call(mut self, name: impl Into<String>, args: serde_json::Value) -> Self {
        self.events.push(AgentEvent::ToolCall(AgentToolCall {
            id: format!("call_{}", self.events.len()),
            name: name.into(),
            arguments: args,
        }));
        self
    }

    /// Append a [`ToolResult`](AgentEvent::ToolResult) event.
    pub fn tool_result(
        mut self,
        tool_call_id: impl Into<String>,
        output: impl Into<String>,
    ) -> Self {
        self.events.push(AgentEvent::ToolResult {
            tool_call_id: tool_call_id.into(),
            output: output.into(),
        });
        self
    }

    /// Append an [`Error`](AgentEvent::Error) event.
    pub fn error(mut self, msg: impl Into<String>) -> Self {
        self.events.push(AgentEvent::Error(msg.into()));
        self
    }

    /// Append a [`Done`](AgentEvent::Done) event.
    pub fn done(mut self) -> Self {
        self.events.push(AgentEvent::Done);
        self
    }

    /// Consume the builder and produce a [`MockProvider`].
    pub fn build(&self) -> MockProvider {
        MockProvider {
            events: self.events.clone(),
            delay: self.delay,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_text_response() {
        let provider = MockProvider::with_text("Hello, world!");
        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        let events: Vec<AgentEvent> = stream.collect().await;

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], AgentEvent::TextDelta(s) if s == "Hello, world!"));
        assert!(matches!(&events[1], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_mock_tool_call() {
        let provider = MockProvider::with_tool_call("cargo", "Compilation successful");
        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        let events: Vec<AgentEvent> = stream.collect().await;

        assert_eq!(events.len(), 5);
        assert!(matches!(&events[1], AgentEvent::ToolCall(tc) if tc.name == "cargo"));
        assert!(
            matches!(&events[2], AgentEvent::ToolResult { output, .. } if output == "Compilation successful")
        );
        assert!(matches!(&events[4], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_mock_thinking() {
        let provider = MockProvider::with_thinking("Let me analyze…", "The answer is 42.");
        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        let events: Vec<AgentEvent> = stream.collect().await;

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], AgentEvent::ThinkingDelta(s) if s == "Let me analyze…"));
        assert!(matches!(&events[1], AgentEvent::TextDelta(s) if s == "The answer is 42."));
        assert!(matches!(&events[2], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_mock_error() {
        let provider = MockProvider::with_error("Rate limit exceeded");
        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        let events: Vec<AgentEvent> = stream.collect().await;

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], AgentEvent::Error(s) if s == "Rate limit exceeded"));
        assert!(matches!(&events[1], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_builder_text() {
        let provider = MockProviderBuilder::new()
            .delay(Duration::from_millis(10))
            .text_delta("Hello,")
            .text_delta(" world!")
            .done()
            .build();

        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        let events: Vec<AgentEvent> = stream.collect().await;

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], AgentEvent::TextDelta(s) if s == "Hello,"));
        assert!(matches!(&events[1], AgentEvent::TextDelta(s) if s == " world!"));
        assert!(matches!(&events[2], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_builder_complex() {
        let provider = MockProviderBuilder::new()
            .delay(Duration::from_millis(5))
            .thinking_delta("Analyzing…")
            .text_delta("Result: ")
            .tool_call(
                "rustc",
                serde_json::json!({"action": "check", "target": "src/main.rs"}),
            )
            .tool_result("call_1", "No errors")
            .text_delta("\nDone!")
            .done()
            .build();

        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        let events: Vec<AgentEvent> = stream.collect().await;

        assert_eq!(events.len(), 7);
        assert!(matches!(&events.last(), Some(AgentEvent::Done)));
    }

    #[tokio::test]
    async fn test_delay_respected() {
        use std::time::Instant;
        let provider = MockProviderBuilder::new()
            .delay(Duration::from_millis(50))
            .text_delta("slow")
            .done()
            .build();

        let start = Instant::now();
        let mut stream = provider
            .stream_chat(vec![], String::new(), vec![])
            .await
            .unwrap();
        while stream.next().await.is_some() {}
        let elapsed = start.elapsed();

        // Two events → at least 1 delay gap = 50ms
        assert!(elapsed >= Duration::from_millis(40), "elapsed={elapsed:?}");
    }

    #[test]
    fn test_mock_provider_is_send_sync() {
        fn check_send<T: Send + Sync>() {}
        check_send::<MockProvider>();
    }
}
