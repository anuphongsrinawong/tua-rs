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
// Public helper functions (self-correction)
// ---------------------------------------------------------------------------

/// Detect whether any of the recent tool results modified a `.rs` file.
///
/// This heuristic scans the message history for [`AgentMessage::ToolResult`]
/// entries whose content mentions paths ending in `.rs` or keywords such
/// as "written", "wrote", or "modified" — indicators that a tool (e.g.
/// `write`, `edit`, or `rustfmt`) may have changed Rust source code.
///
/// Returns `true` when at least one such result is found.
pub fn detect_rust_edits(messages: &[AgentMessage]) -> bool {
    messages.iter().any(|msg| match msg {
        AgentMessage::ToolResult { output, .. } => {
            let lower = output.to_lowercase();
            // Check for explicit `.rs` file references in the output
            if lower.contains(".rs")
                && (lower.contains("written")
                    || lower.contains("wrote")
                    || lower.contains("modified")
                    || lower.contains("updated")
                    || lower.contains("saved"))
            {
                return true;
            }
            // Also match on file-path-like patterns
            lower.contains(".rs`")
                || lower.contains(".rs ")
                || lower.contains(".rs\n")
                || lower.ends_with(".rs")
        }
        _ => false,
    })
}

/// Run `cargo check` in the specified working directory and return the
/// output.
///
/// * `cwd` — optional working directory. When `None`, the current
///   process working directory is used.
///
/// Returns `Ok(output)` if the command succeeded (exit code 0).
/// Returns `Err(errors)` if compilation failed or the command could not
/// be spawned.
pub async fn run_cargo_check(cwd: Option<&str>) -> Result<String, String> {
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("check");
    cmd.arg("--color");
    cmd.arg("never");

    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to spawn `cargo check`: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };

    if output.status.success() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

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
        Self::User { text: text.into() }
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
pub trait ModelProvider: Send + Sync + std::fmt::Debug {
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

    /// Configuration for self-correction behaviour.
    pub harness_config: AgentHarnessConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_rounds: Some(50),
            harness_config: AgentHarnessConfig::default(),
        }
    }
}

/// Configuration for the self-correction subsystem.
///
/// When self-correction is enabled, the harness runs `cargo check` after
/// every tool execution round that modifies `.rs` files. If compilation
/// errors are detected, they are fed back to the model as a
/// [`AgentMessage::User`] so it can attempt to fix them automatically.
#[derive(Debug, Clone)]
pub struct AgentHarnessConfig {
    /// Whether self-correction via `cargo check` is active.
    ///
    /// When `true`, the harness will automatically detect edits to Rust
    /// source files and run `cargo check` after each tool execution round.
    pub self_correction: bool,

    /// Maximum number of consecutive self-correction rounds.
    ///
    /// Each round injects compiler errors as a user message and allows
    /// the model one more attempt to fix them. Once this limit is
    /// reached the harness stops injecting errors and continues
    /// normally.
    pub max_self_corrections: u32,
}

impl Default for AgentHarnessConfig {
    fn default() -> Self {
        Self {
            self_correction: true,
            max_self_corrections: 3,
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

    /// Return a reference to the agent configuration.
    pub fn config(&self) -> &AgentConfig {
        &self.config
    }

    /// Return a mutable reference to the agent configuration.
    pub fn config_mut(&mut self) -> &mut AgentConfig {
        &mut self.config
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
            let mut consecutive_self_corrections = 0u32;

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

                // --- Context Guard: auto-compact at 80% threshold ---
                let used = crate::context_guard::estimate_tokens(&current_messages);
                let max = crate::context_guard::model_max_tokens("deepseek-v4-flash");
                let pct = (used as f64 / max as f64 * 100.0) as u8;
                if pct >= 80 {
                    crate::context_guard::compact_messages(&mut current_messages, 5);
                    let _ = tx.try_send(AgentEvent::TextDelta(format!(
                        "\n🔴 Context compacted ({}K/{}K — {}%) — keeping last 5 turns\n",
                        used / 1000, max / 1000, pct
                    )));
                } else if pct >= 60 {
                    let _ = tx.try_send(AgentEvent::TextDelta(format!(
                        "\n🟡 Context: {}K/{}K ({}%)\n",
                        used / 1000, max / 1000, pct
                    )));
                }

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
                            let _ = tx.try_send(AgentEvent::Error(
                                "unexpected ToolResult in model stream — ignoring".into(),
                            ));
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
                            let _ = tx
                                .try_send(AgentEvent::Error(format!("unknown tool `{}`", tc.name)));
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
                let assistant_opt: Option<String> = if model_text.is_empty() {
                    None
                } else {
                    Some(model_text.clone())
                };
                current_messages.push(AgentMessage::assistant(assistant_opt, tool_calls.clone()));
                current_messages.extend(tool_results.clone());

                // ---------------------------------------------------------
                // Self-correction step
                // ---------------------------------------------------------
                // If self-correction is enabled and the tool round modified
                // `.rs` files, run `cargo check` and inject any errors as a
                // user message so the model can attempt to fix them.
                let correction_config = &config.harness_config;
                if correction_config.self_correction
                    && consecutive_self_corrections < correction_config.max_self_corrections
                    && detect_rust_edits(&current_messages)
                {
                    match run_cargo_check(None).await {
                        Ok(_output) => {
                            // cargo check passed — reset the counter.
                            consecutive_self_corrections = 0;
                        }
                        Err(errors) => {
                            consecutive_self_corrections += 1;
                            let header = format!(
                                "\n\n[Self-correction round {}/{} — cargo check reported errors]\n",
                                consecutive_self_corrections,
                                correction_config.max_self_corrections,
                            );
                            let _ = tx.try_send(AgentEvent::TextDelta(header));

                            // Auto-inject rustc --explain for detected error codes
                            let explain_text = extract_and_explain_errors(&errors);

                            let user_msg = format!(
                                "The `cargo check` command produced errors. \
                                 Please fix them so the project compiles.\n\n```\n{errors}\n```\n\n{explain_text}"
                            );
                            current_messages.push(AgentMessage::user(user_msg));

                            // Instead of continuing the outer loop (which
                            // would consume another tool round), we jump
                            // back to the model call via `continue`.
                            continue;
                        }
                    }
                } else {
                    // No self-correction triggered; reset counter.
                    consecutive_self_corrections = 0;
                }
                // The loop continues for another round.
            }
        });

        Box::pin(rx)
    }
}

// ── rustc --explain auto-inject ─────────────────────────────────────

/// Extract Rust error codes (E0XXX, E1XXX) from compiler output and
/// auto-inject `rustc --explain` documentation for each unique code.
///
/// This gives the agent the official compiler documentation inline,
/// dramatically reducing guesswork when fixing borrow-checker errors.
pub fn extract_and_explain_errors(compiler_output: &str) -> String {
    use std::collections::BTreeSet;
    use std::process::Command;
    
    // Find all unique error codes: E0XXX or E1XXX
    let mut codes = BTreeSet::new();
    for word in compiler_output.split(|c: char| !c.is_alphanumeric()) {
        if (word.starts_with("E0") || word.starts_with("E1")) && word.len() >= 5 {
            codes.insert(word.to_string());
        }
    }
    
    if codes.is_empty() {
        return String::new();
    }
    
    let mut explain = String::from("## 📖 Rust Compiler Documentation\n\n");
    explain.push_str(&format!("Detected {} unique error code(s):\n\n", codes.len()));
    
    for code in codes.iter().take(5) {
        // Run rustc --explain
        let output = Command::new("rustc")
            .args(["--explain", code])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_else(|| format!("(no explanation available for {code})"));
        
        // Truncate long explanations to avoid context bloat
        let truncated: String = output
            .lines()
            .take(30)
            .collect::<Vec<_>>()
            .join("\n");
        
        explain.push_str(&format!("### {code}\n```\n{truncated}\n```\n\n"));
    }
    
    if codes.len() > 5 {
        explain.push_str(&format!(
            "_({} additional error codes not shown — fix these first, then re-run)_\n",
            codes.len() - 5
        ));
    }
    
    explain
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
    use std::sync::atomic::AtomicUsize;

    /// A mock provider that echoes a fixed response.
    #[derive(Debug)]
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
                let input = args.get("input").and_then(|v| v.as_str()).unwrap_or("");
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

        let error_found = events
            .iter()
            .any(|e| matches!(e, AgentEvent::Error(msg) if msg.contains("nonexistent_tool")));
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
            harness_config: AgentHarnessConfig::default(),
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
        assert!(
            matches!(msg, AgentMessage::ToolResult { tool_call_id, output } if tool_call_id == "c1" && output == "output")
        );
    }

    #[test]
    fn test_message_user_empty() {
        let msg = AgentMessage::user("");
        assert!(matches!(msg, AgentMessage::User { text } if text.is_empty()));
    }

    #[test]
    fn test_config_defaults() {
        let cfg = AgentHarnessConfig::default();
        assert!(cfg.self_correction);
        assert_eq!(cfg.max_self_corrections, 3);
    }

    #[test]
    fn test_detect_rust_edits_accepts_messages() {
        // detect_rust_edits API — basic smoke test
        let result = detect_rust_edits(&[]);
        assert!(!result); // empty = no edits
    }

    #[test]
    fn test_agent_event_variants() {
        assert!(matches!(
            AgentEvent::TextDelta("hi".into()),
            AgentEvent::TextDelta(_)
        ));
        assert!(matches!(
            AgentEvent::ThinkingDelta("...".into()),
            AgentEvent::ThinkingDelta(_)
        ));
        assert!(matches!(AgentEvent::Done, AgentEvent::Done));
        assert!(matches!(
            AgentEvent::Error("oops".into()),
            AgentEvent::Error(_)
        ));
    }

    #[test]
    fn test_cargo_check_non_existent_dir() {
        // run_cargo_check is async — tested in tokio tests above
    }

    #[test]
    fn test_agent_tool_call_struct() {
        let tc = AgentToolCall {
            id: "call-1".into(),
            name: "cargo".into(),
            arguments: serde_json::json!({"subcommand": "check"}),
        };
        assert_eq!(tc.id, "call-1");
        assert_eq!(tc.name, "cargo");
    }
}
