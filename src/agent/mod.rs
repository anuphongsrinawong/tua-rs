//! 🦀 Agent Harness — chat loop, tool execution, streaming events.
//!
//! This module provides the core building blocks for an agent that
//! interacts with an LLM via the [`ModelProvider`] trait, streaming
//! incremental events back to the caller while automatically executing
//! tool calls in a loop.

use futures::channel::mpsc;
use futures::future::BoxFuture;
use futures::Stream;
use futures::StreamExt;
use serde::Deserialize;
use serde::Serialize;
use std::pin::Pin;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors originating from the agent harness or tool execution.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    /// A tool was called but no matching registered tool was found.
    #[error("unknown tool `{tool_name}` — no tool with that name is registered")]
    UnknownTool {
        /// The name the model attempted to call.
        tool_name: String,
    },

    /// A known tool was called with arguments that could not be executed
    /// (e.g. the executor returned an error).
    #[error("tool `{tool_name}` failed: {message}")]
    ToolExecution {
        /// The tool that failed.
        tool_name: String,
        /// Human-readable error detail.
        message: String,
    },

    /// The agent hit the maximum number of consecutive tool-call rounds.
    #[error("agent reached the maximum tool-call round limit ({max})")]
    MaxRoundsExceeded {
        /// Configured maximum.
        max: usize,
    },

    /// The underlying stream produced an error that is not tool-related.
    #[error("stream error: {0}")]
    StreamError(String),

    /// A tool call could not be serialized/deserialized properly.
    #[error("invalid tool call arguments for `{tool_name}`: {message}")]
    InvalidToolCall {
        /// The tool that was called.
        tool_name: String,
        /// What went wrong.
        message: String,
    },
}

/// Convenience alias for `Result<T, AgentError>`.
pub type AgentResult<T> = Result<T, AgentError>;

// ---------------------------------------------------------------------------
// AgentMessage — conversation turns
// ---------------------------------------------------------------------------

/// A single message in the conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentMessage {
    /// A message from the human user.
    User {
        /// Message content.
        text: String,
    },

    /// A message from the assistant model.
    Assistant {
        /// The text content (if any).
        text: Option<String>,
        /// Tool calls issued by the model in this turn.
        tool_calls: Vec<AgentToolCall>,
    },

    /// The result of executing a tool.
    ToolResult {
        /// The ID of the tool call this result belongs to.
        tool_call_id: String,
        /// The output produced by the tool.
        output: String,
    },
}

impl AgentMessage {
    /// Build a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self::User {
            text: text.into(),
        }
    }

    /// Build an assistant message with optional text and tool calls.
    pub fn assistant(text: Option<String>, tool_calls: Vec<AgentToolCall>) -> Self {
        Self::Assistant { text, tool_calls }
    }

    /// Build a tool-result message.
    pub fn tool_result(tool_call_id: impl Into<String>, output: impl Into<String>) -> Self {
        Self::ToolResult {
            tool_call_id: tool_call_id.into(),
            output: output.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// AgentToolCall — a tool invocation emitted by the model
// ---------------------------------------------------------------------------

/// A tool call issued by the model, typically embedded inside an
/// [`AgentMessage::Assistant`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentToolCall {
    /// Unique identifier for this call (used to pair it with a result).
    pub id: String,
    /// The name of the tool to invoke.
    pub name: String,
    /// JSON arguments passed by the model.
    pub arguments: serde_json::Value,
}

// ---------------------------------------------------------------------------
// AgentTool — tool definition + executor
// ---------------------------------------------------------------------------

/// Asynchronous executor for a tool: given JSON arguments, returns a
/// JSON string result (or an error).
pub type ToolExecutor =
    Arc<dyn Fn(serde_json::Value) -> BoxFuture<'static, AgentResult<String>> + Send + Sync>;

/// A registered tool that the model may invoke.
///
/// Each tool has a **name**, a human-readable **description**, a JSON
/// **input_schema** describing valid arguments (following the JSON
/// Schema / OpenAPI convention), and an **executor** — an async
/// callable that carries out the tool's logic.
#[derive(Clone)]
pub struct AgentTool {
    /// Unique identifier for the tool (e.g. `"read_file"`).
    pub name: String,
    /// Natural-language description of what the tool does.
    pub description: String,
    /// JSON Schema document describing the expected arguments.
    pub input_schema: serde_json::Value,
    /// The async function that executes the tool.
    pub executor: ToolExecutor,
}

impl AgentTool {
    /// Create a new tool definition.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
        executor: ToolExecutor,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            executor,
        }
    }

    /// Invoke this tool with the given JSON arguments.
    pub async fn execute(&self, arguments: serde_json::Value) -> AgentResult<String> {
        (self.executor)(arguments).await
    }
}

impl std::fmt::Debug for AgentTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentTool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("input_schema", &self.input_schema)
            .field("executor", &"<fn>")
            .finish()
    }
}

// ---------------------------------------------------------------------------
// AgentEvent — streaming events
// ---------------------------------------------------------------------------

/// A single event yielded by the agent stream.
///
/// Events are surfaced incrementally — text and thinking deltas arrive
/// as the model generates, tool calls are emitted as soon as the model
/// requests them, and [`ToolResult`](AgentEvent::ToolResult) / [`Error`](AgentEvent::Error) /
/// [`Done`](AgentEvent::Done) are produced by the harness.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// A chunk of output text.
    TextDelta(String),
    /// A chunk of the model's internal reasoning / chain-of-thought.
    ThinkingDelta(String),
    /// The model has requested a tool invocation.
    ToolCall(AgentToolCall),
    /// The result of executing a tool.
    ToolResult {
        /// The ID of the originating tool call.
        tool_call_id: String,
        /// The tool's output.
        output: String,
    },
    /// A non-fatal error occurred (the loop may still continue).
    Error(String),
    /// The agent has finished processing.
    Done,
}

// ---------------------------------------------------------------------------
// ModelProvider trait
// ---------------------------------------------------------------------------

/// A provider that can stream chat completions from a language model.
///
/// The returned stream yields [`AgentEvent`] items: text deltas,
/// thinking deltas, tool calls, and finally a [`Done`](AgentEvent::Done).
/// Any critical failure should be returned as the outer [`AgentResult`];
/// non-fatal errors can be sent as [`AgentEvent::Error`] inside the
/// stream.
///
/// # Lifetime
///
/// The returned stream has a `'static` bound so it can be spawned
/// onto a separate task. Providers should clone any shared state
/// (e.g. HTTP clients) rather than borrowing from `self`.
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync {
    /// Start a streaming chat completion.
    ///
    /// * `messages` — the conversation history so far.
    /// * `system` — the system prompt.
    /// * `tools` — tools available to the model.
    async fn stream_chat(
        &self,
        messages: Vec<AgentMessage>,
        system: String,
        tools: Vec<AgentTool>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>>;
}

// ---------------------------------------------------------------------------
// AgentLoop — the main run-loop
// ---------------------------------------------------------------------------

/// Configuration for the agent execution loop.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of tool-call rounds before the agent is forced to stop.
    ///
    /// Each round is: model generates → tool calls → tool results fed back.
    /// `None` means no limit (use with caution).
    pub max_tool_rounds: Option<usize>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: Some(50),
        }
    }
}

/// The agent orchestration harness.
///
/// Wraps a [`ModelProvider`], a system prompt, and a set of tools,
/// and provides a [`run`](AgentLoop::run) method that executes the
/// chat loop, automatically invoking tools when the model requests them.
#[derive(Clone)]
pub struct AgentLoop {
    provider: Arc<dyn ModelProvider>,
    system: String,
    tools: Vec<AgentTool>,
    config: AgentConfig,
}

impl AgentLoop {
    /// Create a new agent loop.
    pub fn new(
        provider: Arc<dyn ModelProvider>,
        system: impl Into<String>,
        tools: Vec<AgentTool>,
    ) -> Self {
        Self {
            provider,
            system: system.into(),
            tools,
            config: AgentConfig::default(),
        }
    }

    /// Create a new agent loop with a custom [`AgentConfig`].
    pub fn with_config(
        provider: Arc<dyn ModelProvider>,
        system: impl Into<String>,
        tools: Vec<AgentTool>,
        config: AgentConfig,
    ) -> Self {
        Self {
            provider,
            system: system.into(),
            tools,
            config,
        }
    }

    /// Return a reference to the registered tools.
    pub fn tools(&self) -> &[AgentTool] {
        &self.tools
    }

    /// Return a reference to the system prompt.
    pub fn system(&self) -> &str {
        &self.system
    }

    /// Run the agent loop, consuming `messages` and returning a
    /// `Stream` of [`AgentEvent`]s.
    ///
    /// The stream is driven by an internal async task that:
    ///
    /// 1. Calls the [`ModelProvider::stream_chat`] and forwards text /
    ///    thinking deltas.
    /// 2. When the model emits [`AgentEvent::ToolCall`] events,
    ///    looks up the corresponding tool and executes it.
    /// 3. Feeds the tool result back into the conversation and
    ///    repeats from step 1.
    /// 4. Terminates when the model produces no more tool calls or
    ///    [`AgentEvent::Done`] is received.
    ///
    /// The returned stream is `Send` + `'static` so it can be
    /// collected or forwarded across await points.
    pub fn run(
        &self,
        messages: Vec<AgentMessage>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>> {
        let provider = Arc::clone(&self.provider);
        let system = self.system.clone();
        let tools = self.tools.clone();
        let config = self.config.clone();

        let (mut tx, rx) = mpsc::channel::<AgentEvent>(256);

        tokio::spawn(async move {
            let mut current_messages = messages;
            let mut tool_rounds = 0usize;

            loop {
                // --- Check round limit ---
                if let Some(max) = config.max_tool_rounds {
                    if tool_rounds >= max {
                        let _ = tx.try_send(AgentEvent::Error(format!(
                            "maximum tool-call rounds ({max}) reached",
                        )));
                        let _ = tx.try_send(AgentEvent::Done);
                        return;
                    }
                }
                tool_rounds += 1;

                // --- Call the model ---
                let stream = match provider
                    .stream_chat(current_messages.clone(), system.clone(), tools.clone())
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.try_send(AgentEvent::Error(e.to_string()));
                        let _ = tx.try_send(AgentEvent::Done);
                        return;
                    }
                };

                // --- Collect the model's response ---
                let mut model_text = String::new();
                let mut tool_calls: Vec<AgentToolCall> = Vec::new();

                let mut stream = Box::pin(stream);
                loop {
                    match stream.next().await {
                        Some(AgentEvent::TextDelta(chunk)) => {
                            model_text.push_str(&chunk);
                            let _ = tx.try_send(AgentEvent::TextDelta(chunk));
                        }
                        Some(AgentEvent::ThinkingDelta(chunk)) => {
                            let _ = tx.try_send(AgentEvent::ThinkingDelta(chunk));
                        }
                        Some(AgentEvent::ToolCall(tc)) => {
                            tool_calls.push(tc.clone());
                            let _ = tx.try_send(AgentEvent::ToolCall(tc));
                        }
                        Some(AgentEvent::Error(e)) => {
                            let _ = tx.try_send(AgentEvent::Error(e));
                        }
                        Some(AgentEvent::ToolResult { .. }) => {
                            // ToolResults inside the model stream should not appear;
                            // the harness manages those. Forward anyway.
                            let _ =
                                tx.try_send(AgentEvent::Error("unexpected ToolResult in model stream — ignoring".into()));
                        }
                        Some(AgentEvent::Done) | None => break,
                    }
                }

                // --- If no tool calls, we are done ---
                if tool_calls.is_empty() {
                    let _ = tx.try_send(AgentEvent::Done);
                    return;
                }

                // --- Execute each tool call ---
                let mut tool_results: Vec<AgentMessage> = Vec::new();
                for tc in &tool_calls {
                    // Look up the tool
                    let tool = match self_tool_by_name(&tools, &tc.name) {
                        Some(t) => t,
                        None => {
                            let _ = tx.try_send(AgentEvent::Error(format!(
                                "unknown tool `{}`",
                                tc.name
                            )));
                            // Insert a placeholder result so the model can recover.
                            tool_results.push(AgentMessage::tool_result(
                                &tc.id,
                                format!("[ERROR: unknown tool `{}`]", tc.name),
                            ));
                            continue;
                        }
                    };

                    // Execute
                    let output = match tool.execute(tc.arguments.clone()).await {
                        Ok(out) => out,
                        Err(e) => {
                            let msg = format!("tool `{}` execution failed: {e}", tc.name);
                            let _ = tx.try_send(AgentEvent::Error(msg.clone()));
                            tool_results.push(AgentMessage::tool_result(&tc.id, msg));
                            continue;
                        }
                    };

                    let _ = tx.try_send(AgentEvent::ToolResult {
                        tool_call_id: tc.id.clone(),
                        output: output.clone(),
                    });
                    tool_results.push(AgentMessage::tool_result(&tc.id, output));
                }

                // --- Build the assistant message and append tool results ---
                let assistant_text = if model_text.is_empty() {
                    None
                } else {
                    Some(model_text)
                };
                current_messages
                    .push(AgentMessage::assistant(assistant_text, tool_calls));
                current_messages.extend(tool_results);
                // The loop continues for another round.
            }
        });

        Box::pin(rx)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Find a tool by name in the given slice.
fn self_tool_by_name<'a>(tools: &'a [AgentTool], name: &str) -> Option<&'a AgentTool> {
    tools.iter().find(|t| t.name == name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A mock provider that echoes a fixed response.
    struct EchoProvider {
        response: Vec<AgentEvent>,
    }

    #[async_trait::async_trait]
    impl ModelProvider for EchoProvider {
        async fn stream_chat(
            &self,
            _messages: Vec<AgentMessage>,
            _system: String,
            _tools: Vec<AgentTool>,
        ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>> {
            let events = self.response.clone();
            Ok(Box::pin(futures::stream::iter(events)))
        }
    }

    #[tokio::test]
    async fn test_basic_text_response() {
        let provider = Arc::new(EchoProvider {
            response: vec![
                AgentEvent::TextDelta("Hello".into()),
                AgentEvent::TextDelta(", world!".into()),
                AgentEvent::Done,
            ],
        });

        let loop_ = AgentLoop::new(provider, "system prompt", vec![]);
        let events: Vec<AgentEvent> = loop_.run(vec![]).collect().await;

        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], AgentEvent::TextDelta(ref s) if s == "Hello"));
        assert!(matches!(events[1], AgentEvent::TextDelta(ref s) if s == ", world!"));
        assert!(matches!(events[2], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_tool_execution_round() {
        // A simple tool that uppercases its "input" argument
        let uppercase_exec: ToolExecutor = Arc::new(|args| {
            Box::pin(async move {
                let input = args
                    .get("input")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                Ok(input.to_uppercase())
            })
        });

        let tool = AgentTool::new(
            "uppercase",
            "Uppercases the input string",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "input": {"type": "string"}
                }
            }),
            uppercase_exec,
        );

        let provider = Arc::new(EchoProvider {
            response: vec![
                AgentEvent::TextDelta("I'll uppercase that.".into()),
                AgentEvent::ToolCall(AgentToolCall {
                    id: "call_1".into(),
                    name: "uppercase".into(),
                    arguments: serde_json::json!({"input": "hello world"}),
                }),
                AgentEvent::Done,
            ],
        });

        let loop_ = AgentLoop::new(provider, "system prompt", vec![tool]);
        let events: Vec<AgentEvent> = loop_.run(vec![]).collect().await;

        // We expect: TextDelta, ToolCall, ToolResult, Done
        let tool_result_found = events.iter().any(|e| match e {
            AgentEvent::ToolResult { output, .. } => output == "HELLO WORLD",
            _ => false,
        });
        assert!(
            tool_result_found,
            "expected ToolResult with 'HELLO WORLD', got: {events:#?}"
        );
        // The last event should be Done
        assert!(
            matches!(events.last(), Some(AgentEvent::Done)),
            "expected Done as last event"
        );
    }

    #[tokio::test]
    async fn test_unknown_tool_emits_error() {
        let provider = Arc::new(EchoProvider {
            response: vec![
                AgentEvent::ToolCall(AgentToolCall {
                    id: "call_404".into(),
                    name: "nonexistent_tool".into(),
                    arguments: serde_json::json!({}),
                }),
                AgentEvent::Done,
            ],
        });

        let loop_ = AgentLoop::new(provider, "system prompt", vec![]);
        let events: Vec<AgentEvent> = loop_.run(vec![]).collect().await;

        let error_found = events.iter().any(|e| matches!(e, AgentEvent::Error(msg) if msg.contains("nonexistent_tool")));
        assert!(error_found, "expected Error about unknown tool");
        assert!(matches!(events.last(), Some(AgentEvent::Done)));
    }

    #[tokio::test]
    async fn test_max_rounds_limit() {
        let counter = Arc::new(AtomicUsize::new(0));

        // A tool that triggers another round by making the model call it again
        let counter_clone = Arc::clone(&counter);
        let loopback_exec: ToolExecutor = Arc::new(move |_args| {
            let _ = Arc::clone(&counter_clone);
            Box::pin(async move { Ok("done".into()) })
        });

        let tool = AgentTool::new(
            "loopback",
            "Triggers another round",
            serde_json::json!({"type": "object", "properties": {}}),
            loopback_exec,
        );

        // Each model response produces a tool call, so the loop will keep going
        let response = vec![
            AgentEvent::ToolCall(AgentToolCall {
                id: "call_lp".into(),
                name: "loopback".into(),
                arguments: serde_json::json!({}),
            }),
            AgentEvent::Done,
        ];
        let provider = Arc::new(EchoProvider { response });

        let config = AgentConfig {
            max_tool_rounds: Some(3),
        };
        let loop_ = AgentLoop::with_config(provider, "system prompt", vec![tool], config);

        // With max_tool_rounds=3 and each round producing a tool call,
        // we expect it to hit the limit.
        let events: Vec<AgentEvent> = loop_.run(vec![]).collect().await;

        let error_found = events
            .iter()
            .any(|e| matches!(e, AgentEvent::Error(msg) if msg.contains("maximum tool-call")));
        assert!(
            error_found,
            "expected Error about max rounds, got: {events:#?}"
        );
        assert!(matches!(events.last(), Some(AgentEvent::Done)));
    }

    #[tokio::test]
    async fn test_message_constructors() {
        let msg = AgentMessage::user("hello");
        assert!(matches!(msg, AgentMessage::User { text } if text == "hello"));

        let msg = AgentMessage::assistant(
            Some("response".into()),
            vec![AgentToolCall {
                id: "c1".into(),
                name: "tool".into(),
                arguments: serde_json::json!({}),
            }],
        );
        assert!(matches!(msg, AgentMessage::Assistant { text: Some(t), .. } if t == "response"));

        let msg = AgentMessage::tool_result("c1", "output");
        assert!(matches!(msg, AgentMessage::ToolResult { tool_call_id, output } if tool_call_id == "c1" && output == "output"));
    }
}
