//! OpenAI-compatible model provider.
//!
//! Implements [`ModelProvider`] for any API that follows the OpenAI
//! `/chat/completions` streaming format (ChatGPT, DeepSeek, Qwen, Grok,
//! Together AI, etc.).
//!
//! ## Configuration
//!
//! The [`ProviderConfig`] struct holds the three essential parameters:
//!
//! * `api_key` — Bearer token for authentication.
//! * `base_url` — API endpoint root (defaults to `https://api.openai.com/v1`).
//! * `model` — Model identifier (e.g. `"gpt-4o"`, `"deepseek-chat"`).

use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::agent::{
    AgentError, AgentEvent, AgentMessage, AgentResult, AgentTool, AgentToolCall, ModelProvider,
};

// ---------------------------------------------------------------------------
// Public configuration
// ---------------------------------------------------------------------------

/// Configuration for an OpenAI-compatible provider.
///
/// # Example (TOML)
///
/// ```toml
/// [provider]
/// api_key = "sk-..."
/// base_url = "https://api.openai.com/v1"
/// model = "gpt-4o"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// API key for Bearer authentication.
    pub api_key: String,
    /// API root URL (defaults to `https://api.openai.com/v1`).
    #[serde(default = "default_base_url")]
    pub base_url: String,
    /// Model identifier (e.g. `"gpt-4o"`, `"deepseek-chat"`).
    pub model: String,
}

fn default_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// An OpenAI-compatible provider backed by [`reqwest`].
///
/// Uses SSE (Server-Sent Events) over HTTP to stream token deltas and
/// tool-call fragments.  Thread-safe (`Send + Sync`).
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    /// Create a new provider from the given configuration.
    ///
    /// Sets up a `reqwest::Client` with a Bearer token, JSON content type,
    /// and a 120-second timeout.
    pub fn new(config: ProviderConfig) -> Self {
        let mut default_headers = HeaderMap::new();

        let mut auth_value = HeaderValue::from_str(&format!("Bearer {}", config.api_key))
            .expect("API key contains invalid header bytes");
        auth_value.set_sensitive(true);
        default_headers.insert(AUTHORIZATION, auth_value);

        default_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(default_headers)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest client");

        Self { config, client }
    }

    /// Full URL for the chat completions endpoint.
    fn chat_url(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        format!("{base}/chat/completions")
    }
}

// ---------------------------------------------------------------------------
// ModelProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for OpenAiCompatibleProvider {
    async fn stream_chat(
        &self,
        messages: Vec<AgentMessage>,
        system: String,
        tools: Vec<AgentTool>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>> {
        let url = self.chat_url();

        // Convert agent types → wire format.
        let wire_messages = agent_messages_to_wire(messages, system);
        let wire_tools = agent_tools_to_wire(&tools);

        debug!(
            url = %url,
            model = %self.config.model,
            messages = %wire_messages.len(),
            tools = %wire_tools.as_ref().map_or(0, |t| t.len()),
            "POST /chat/completions"
        );

        let body = ChatRequest {
            model: self.config.model.clone(),
            messages: wire_messages,
            stream: true,
            tools: wire_tools,
        };

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::StreamError(format!("HTTP request failed: {e}")))?;

        // Check for HTTP-level errors before attempting to stream.
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_text = response.text().await.unwrap_or_else(|_| "<no body>".into());
            warn!(status, body = %body_text, "API returned error");
            return Err(AgentError::StreamError(format!(
                "API error: {status} — {body_text}"
            )));
        }

        // Channel to bridge the async SSE reader with the returned Stream.
        let (tx, rx) = futures::channel::mpsc::unbounded();

        tokio::spawn(async move {
            let sse_result = parse_sse(response.bytes_stream(), tx.clone()).await;
            if let Err(err) = sse_result {
                warn!(error = %err, "SSE stream processing failed");
                let _ = tx.unbounded_send(AgentEvent::Error(err.to_string()));
            }
        });

        Ok(Box::pin(rx))
    }
}

// ---------------------------------------------------------------------------
// Type conversions: Agent → Wire
// ---------------------------------------------------------------------------

/// Convert `AgentMessage`s + a system prompt into the OpenAI wire format.
fn agent_messages_to_wire(messages: Vec<AgentMessage>, system: String) -> Vec<WireMessage> {
    let mut out: Vec<WireMessage> = Vec::with_capacity(messages.len() + 1);

    // Prepend system prompt as a system-role message.
    if !system.is_empty() {
        out.push(WireMessage {
            role: "system".into(),
            content: Some(system),
            tool_calls: None,
            tool_call_id: None,
            name: None,
        });
    }

    for msg in messages {
        match msg {
            AgentMessage::User { text } => out.push(WireMessage {
                role: "user".into(),
                content: Some(text),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            }),
            AgentMessage::Assistant { text, tool_calls } => {
                let wire_calls = if tool_calls.is_empty() {
                    None
                } else {
                    Some(
                        tool_calls
                            .into_iter()
                            .map(|tc| WireToolCall {
                                id: tc.id,
                                call_type: "function".into(),
                                function: WireToolCallFunction {
                                    name: tc.name,
                                    arguments: tc.arguments.to_string(),
                                },
                            })
                            .collect(),
                    )
                };
                out.push(WireMessage {
                    role: "assistant".into(),
                    content: text,
                    tool_calls: wire_calls,
                    tool_call_id: None,
                    name: None,
                });
            }
            AgentMessage::ToolResult {
                tool_call_id,
                output,
            } => out.push(WireMessage {
                role: "tool".into(),
                content: Some(output),
                tool_calls: None,
                tool_call_id: Some(tool_call_id),
                name: None,
            }),
        }
    }

    out
}

/// Convert `AgentTool`s into the OpenAI tool definition format.
fn agent_tools_to_wire(tools: &[AgentTool]) -> Option<Vec<WireToolDefinition>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| WireToolDefinition {
                def_type: "function".into(),
                function: WireToolFunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Wire format types (requests)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<WireMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<WireToolDefinition>>,
}

#[derive(Debug, Serialize)]
struct WireMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<WireToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct WireToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: WireToolCallFunction,
}

#[derive(Debug, Serialize)]
struct WireToolCallFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct WireToolDefinition {
    #[serde(rename = "type")]
    def_type: String,
    function: WireToolFunctionDef,
}

#[derive(Debug, Serialize)]
struct WireToolFunctionDef {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Wire format types (response / SSE)
// ---------------------------------------------------------------------------

/// Top-level SSE chunk from `/chat/completions`.
#[derive(Debug, Deserialize)]
struct SseChunk {
    #[allow(dead_code)]
    id: Option<String>,
    #[allow(dead_code)]
    object: Option<String>,
    #[allow(dead_code)]
    created: Option<u64>,
    #[allow(dead_code)]
    model: Option<String>,
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    #[allow(dead_code)]
    index: u64,
    delta: Delta,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
    /// Used by DeepSeek, Qwen, and some OpenAI-compatible providers.
    #[serde(default)]
    reasoning_content: Option<String>,
    /// Used by Anthropic-style compatible APIs.
    #[serde(default)]
    thinking: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<DeltaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCall {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "type", default)]
    call_type: Option<String>,
    #[serde(default)]
    function: Option<DeltaToolCallFunction>,
}

#[derive(Debug, Deserialize)]
struct DeltaToolCallFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// ---------------------------------------------------------------------------
// SSE stream parser
// ---------------------------------------------------------------------------

/// Accumulator for a single tool call being built across streaming chunks.
#[derive(Debug, Clone, Default)]
struct ToolCallAccumulator {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

/// Parse an SSE byte stream from `/chat/completions` and emit
/// [`AgentEvent`] values through the sender.
///
/// The `byte_stream` parameter is the raw HTTP response body (bytes).
/// Lines are delimited by `\n`, and each `data:` line is parsed as JSON.
async fn parse_sse<S, E>(
    byte_stream: S,
    tx: futures::channel::mpsc::UnboundedSender<AgentEvent>,
) -> Result<(), String>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut byte_stream = byte_stream;
    let mut buf: Vec<u8> = Vec::new();
    let mut tool_accums: HashMap<usize, ToolCallAccumulator> = HashMap::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("SSE byte stream error: {e}"))?;
        buf.extend_from_slice(&chunk);

        // Process all complete lines from the buffer.
        loop {
            let nl_pos = match buf.iter().position(|&b| b == b'\n') {
                Some(pos) => pos,
                None => break,
            };

            let line: Vec<u8> = buf.drain(..=nl_pos).collect();
            let line = line
                .strip_suffix(b"\n")
                .unwrap_or(&line)
                .strip_suffix(b"\r")
                .unwrap_or(&line);

            if line.is_empty() || line.starts_with(b":") {
                // SSE comment or heartbeat — skip.
                continue;
            }

            if !line.starts_with(b"data: ") {
                continue;
            }

            let payload = &line[b"data: ".len()..];

            // Check for the stream-termination marker.
            if payload == b"[DONE]" {
                debug!("SSE stream finished ([DONE])");
                emit_remaining_tool_calls(&tx, &mut tool_accums);
                let _ = tx.unbounded_send(AgentEvent::Done);
                return Ok(());
            }

            // Parse the JSON chunk.
            let chunk: SseChunk = match serde_json::from_slice(payload) {
                Ok(c) => c,
                Err(e) => {
                    let text = String::from_utf8_lossy(payload).into_owned();
                    warn!(error = %e, payload = %text, "Failed to parse SSE chunk");
                    continue; // non-fatal
                }
            };

            trace!("SSE chunk: {} choices", chunk.choices.len());

            for choice in &chunk.choices {
                // --- Text delta ---
                if let Some(ref content) = choice.delta.content {
                    if !content.is_empty()
                        && tx
                            .unbounded_send(AgentEvent::TextDelta(content.clone()))
                            .is_err()
                    {
                        return Ok(());
                    }
                }

                // --- Thinking / reasoning delta ---
                // `reasoning_content` (DeepSeek, Qwen) preferred over
                // `thinking` (Anthropic compat).
                let thinking = choice
                    .delta
                    .reasoning_content
                    .as_deref()
                    .or(choice.delta.thinking.as_deref());
                if let Some(text) = thinking {
                    if !text.is_empty()
                        && tx
                            .unbounded_send(AgentEvent::ThinkingDelta(text.to_string()))
                            .is_err()
                    {
                        return Ok(());
                    }
                }

                // --- Tool call deltas (accumulate by index) ---
                if let Some(ref tool_calls) = choice.delta.tool_calls {
                    for tc in tool_calls {
                        let acc = tool_accums
                            .entry(tc.index)
                            .or_default();

                        if let Some(id) = tc.id.as_ref() {
                            acc.id = Some(id.clone());
                        }
                        if let Some(name) = tc.function.as_ref().and_then(|f| f.name.as_ref()) {
                            acc.name = Some(name.clone());
                        }
                        if let Some(args) = tc.function.as_ref().and_then(|f| f.arguments.as_ref())
                        {
                            acc.arguments.push_str(args);
                        }

                        // Emit a ToolCall event with the current accumulated
                        // arguments so the caller sees progress.
                        let call = agent_tool_call_from_accumulator(acc);
                        if tx.unbounded_send(AgentEvent::ToolCall(call)).is_err() {
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    // Stream ended without [DONE]; flush remaining tool calls and emit Done.
    emit_remaining_tool_calls(&tx, &mut tool_accums);
    let _ = tx.unbounded_send(AgentEvent::Done);

    Ok(())
}

/// Build an [`AgentToolCall`] from an accumulator's current state.
fn agent_tool_call_from_accumulator(acc: &ToolCallAccumulator) -> AgentToolCall {
    // The arguments string should be valid JSON by the end, but may be
    // incomplete mid-stream. Parse what we have and fall back to
    // `Value::Null` if the fragment is not valid JSON yet.
    let args: serde_json::Value = serde_json::from_str(&acc.arguments).unwrap_or_default();

    AgentToolCall {
        id: acc.id.clone().unwrap_or_default(),
        name: acc.name.clone().unwrap_or_default(),
        arguments: args,
    }
}

/// Emit any accumulated tool calls that have partial data remaining.
fn emit_remaining_tool_calls(
    tx: &futures::channel::mpsc::UnboundedSender<AgentEvent>,
    accums: &mut HashMap<usize, ToolCallAccumulator>,
) {
    if accums.is_empty() {
        return;
    }

    let mut indices: Vec<usize> = accums.keys().copied().collect();
    indices.sort_unstable();

    for idx in indices {
        if let Some(acc) = accums.remove(&idx) {
            let call = agent_tool_call_from_accumulator(&acc);
            let _ = tx.unbounded_send(AgentEvent::ToolCall(call));
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

    /// Collect all events from raw byte chunks.
    async fn run_parser(chunks: &[&[u8]]) -> Vec<AgentEvent> {
        let owned_chunks: Vec<Bytes> = chunks.iter().map(|c| Bytes::copy_from_slice(c)).collect();
        let (tx, rx) = futures::channel::mpsc::unbounded();
        tokio::spawn(async move {
            let stream = futures::stream::iter(owned_chunks.into_iter().map(Ok::<_, String>));
            let _ = parse_sse(stream, tx).await;
        });
        rx.collect().await
    }

    #[tokio::test]
    async fn test_done_termination() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], AgentEvent::TextDelta(s) if s == "Hi"));
        assert!(matches!(&events[1], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_content_deltas() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"}}]}\n",
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        assert_eq!(events.len(), 3);
        assert!(matches!(&events[0], AgentEvent::TextDelta(s) if s == "Hello"));
        assert!(matches!(&events[1], AgentEvent::TextDelta(s) if s == " world"));
        assert!(matches!(&events[2], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_reasoning_content() {
        let events = run_parser(&[b"data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"Let me think\"}}]}\n", b"data: [DONE]\n"]).await;

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], AgentEvent::ThinkingDelta(s) if s == "Let me think"));
        assert!(matches!(&events[1], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_thinking_field() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"thinking\":\"hmm...\"}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        assert!(matches!(&events[0], AgentEvent::ThinkingDelta(s) if s == "hmm..."));
    }

    /// `reasoning_content` should take priority over `thinking`.
    #[tokio::test]
    async fn test_reasoning_over_thinking() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"primary\",\"thinking\":\"fallback\"}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        assert!(matches!(&events[0], AgentEvent::ThinkingDelta(s) if s == "primary"));
    }

    #[tokio::test]
    async fn test_tool_call_accumulation() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]}}]}\n",
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"location\\\":\"}}]}}]}\n",
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"NYC\\\"}\"}}]}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        let tool_events: Vec<AgentToolCall> = events
            .iter()
            .filter_map(|e| {
                if let AgentEvent::ToolCall(tc) = e {
                    Some(tc.clone())
                } else {
                    None
                }
            })
            .collect();

        assert!(!tool_events.is_empty(), "should have tool call events");
        let last = tool_events.last().unwrap();
        assert_eq!(last.id, "call_1");
        assert_eq!(last.name, "get_weather");
        assert_eq!(last.arguments, serde_json::json!({"location": "NYC"}));
    }

    #[tokio::test]
    async fn test_multiple_tool_calls() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"function\",\"function\":{\"name\":\"fn_a\",\"arguments\":\"\"}},{\"index\":1,\"id\":\"call_b\",\"type\":\"function\",\"function\":{\"name\":\"fn_b\",\"arguments\":\"\"}}]}}]}\n",
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1}\"}},{\"index\":1,\"function\":{\"arguments\":\"{\\\"b\\\":2}\"}}]}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        let tool_events: Vec<AgentToolCall> = events
            .iter()
            .filter_map(|e| {
                if let AgentEvent::ToolCall(tc) = e {
                    Some(tc.clone())
                } else {
                    None
                }
            })
            .collect();

        let calls: HashMap<String, &AgentToolCall> =
            tool_events.iter().map(|tc| (tc.name.clone(), tc)).collect();

        assert_eq!(
            calls.get("fn_a").map(|c| &c.arguments),
            Some(&serde_json::json!({"a": 1}))
        );
        assert_eq!(
            calls.get("fn_b").map(|c| &c.arguments),
            Some(&serde_json::json!({"b": 2}))
        );
    }

    #[tokio::test]
    async fn test_skip_empty_content() {
        let events = run_parser(&[
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"\"}}]}\n",
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"real\"}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], AgentEvent::TextDelta(s) if s == "real"));
        assert!(matches!(&events[1], AgentEvent::Done));
    }

    #[tokio::test]
    async fn test_malformed_json_skipped() {
        let events = run_parser(&[
            b"data: {invalid json}\n",
            b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"}}]}\n",
            b"data: [DONE]\n",
        ])
        .await;

        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], AgentEvent::TextDelta(s) if s == "ok"));
        assert!(matches!(&events[1], AgentEvent::Done));
    }

    // ------------------------------------------------------------------
    // Conversion tests
    // ------------------------------------------------------------------

    #[test]
    fn test_agent_messages_to_wire_with_system() {
        let messages = vec![
            AgentMessage::user("hello"),
            AgentMessage::assistant(
                Some("world".into()),
                vec![AgentToolCall {
                    id: "c1".into(),
                    name: "greet".into(),
                    arguments: serde_json::json!({"name": "Rust"}),
                }],
            ),
            AgentMessage::tool_result("c1", "Hi Rust!"),
        ];

        let wire = agent_messages_to_wire(messages, "You are helpful.".into());
        assert_eq!(wire.len(), 4);

        assert_eq!(wire[0].role, "system");
        assert_eq!(wire[0].content.as_deref(), Some("You are helpful."));

        assert_eq!(wire[1].role, "user");
        assert_eq!(wire[1].content.as_deref(), Some("hello"));

        assert_eq!(wire[2].role, "assistant");
        assert_eq!(wire[2].content.as_deref(), Some("world"));
        let calls = wire[2].tool_calls.as_ref().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "c1");
        assert_eq!(calls[0].function.arguments, r#"{"name":"Rust"}"#);

        assert_eq!(wire[3].role, "tool");
        assert_eq!(wire[3].tool_call_id.as_deref(), Some("c1"));
        assert_eq!(wire[3].content.as_deref(), Some("Hi Rust!"));
    }

    #[test]
    fn test_agent_tools_to_wire_empty() {
        assert!(agent_tools_to_wire(&[]).is_none());
    }

    #[test]
    fn test_agent_tools_to_wire_non_empty() {
        let tool = AgentTool::new(
            "test_tool",
            "A test tool",
            serde_json::json!({"type": "object"}),
            std::sync::Arc::new(|_| Box::pin(async { Ok("ok".into()) })),
        );
        let wire = agent_tools_to_wire(&[tool]).unwrap();
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].function.name, "test_tool");
    }
}
