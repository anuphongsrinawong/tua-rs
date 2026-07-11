//! Anthropic provider.
//!
//! Implements [`ModelProvider`] for the Anthropic API. Uses the same SSE
//! streaming format as OpenAI-compatible APIs but authenticates with the
//! `x-api-key` header instead of `Authorization: Bearer`.
//!
//! # Configuration
//!
//! * `api_key` — sent as `x-api-key`.
//! * `base_url` — defaults to `https://api.anthropic.com/v1`.
//! * `model` — e.g. `"claude-sonnet-4-20250514"`, `"claude-3-haiku"`.

use std::pin::Pin;

use async_trait::async_trait;
use bytes::Bytes;
use futures::{Stream, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::agent::{
    AgentError, AgentEvent, AgentMessage, AgentResult, AgentTool, AgentToolCall, ModelProvider,
};

use super::ProviderConfig;

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// An Anthropic provider backed by [`reqwest`].
///
/// Uses SSE (Server-Sent Events) over HTTP to stream token deltas and
/// tool-call fragments. Thread-safe (`Send + Sync`).
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider from the given configuration.
    ///
    /// Sets up a `reqwest::Client` with the `x-api-key` header, JSON
    /// content type, anthropic-version header, and a 120-second timeout.
    pub fn new(config: ProviderConfig) -> Self {
        let mut default_headers = HeaderMap::new();

        // Anthropic uses `x-api-key` for auth.
        if !config.api_key.is_empty() {
            let mut auth_value = HeaderValue::from_str(&config.api_key)
                .expect("API key contains invalid header bytes");
            auth_value.set_sensitive(true);
            default_headers.insert("x-api-key", auth_value);
        }

        // Required by Anthropic API.
        default_headers.insert(
            "anthropic-version",
            HeaderValue::from_static("2023-06-01"),
        );

        default_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let client = reqwest::Client::builder()
            .default_headers(default_headers)
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to build reqwest client");

        Self { config, client }
    }

    /// Full URL for the messages endpoint.
    fn messages_url(&self) -> String {
        let base = self.config.resolved_base_url().trim_end_matches('/');
        format!("{base}/messages")
    }
}

// ---------------------------------------------------------------------------
// ModelProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for AnthropicProvider {
    async fn stream_chat(
        &self,
        messages: Vec<AgentMessage>,
        system: String,
        tools: Vec<AgentTool>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>> {
        let url = self.messages_url();

        // Convert agent types → Anthropic wire format.
        let (wire_system, wire_messages) = agent_messages_to_anthropic(messages, system);
        let wire_tools = agent_tools_to_anthropic(&tools);

        debug!(
            url = %url,
            model = %self.config.model,
            messages = %wire_messages.len(),
            tools = %wire_tools.as_ref().map_or(0, |t| t.len()),
            "POST /v1/messages"
        );

        let body = AnthropicMessageRequest {
            model: self.config.model.clone(),
            messages: wire_messages,
            system: wire_system,
            max_tokens: 8192,
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
            warn!(status, body = %body_text, "Anthropic API returned error");
            return Err(AgentError::StreamError(format!(
                "Anthropic API error: {status} — {body_text}"
            )));
        }

        // Channel to bridge the async SSE reader with the returned Stream.
        let (tx, rx) = futures::channel::mpsc::unbounded();

        tokio::spawn(async move {
            if let Err(err) = parse_anthropic_sse(response.bytes_stream(), tx.clone()).await {
                warn!(error = %err, "Anthropic SSE stream processing failed");
                let _ = tx.unbounded_send(AgentEvent::Error(err.to_string()));
            }
        });

        Ok(Box::pin(rx))
    }
}

// ---------------------------------------------------------------------------
// Anthropic-specific wire types
// ---------------------------------------------------------------------------

/// Request body for `POST /v1/messages`.
#[derive(Debug, Serialize)]
struct AnthropicMessageRequest {
    model: String,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    max_tokens: u32,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<AnthropicToolDef>>,
}

/// A single message in the Anthropic conversation format.
#[derive(Debug, Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

/// Anthropic content blocks can be text or tool_use/tool_result.
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum AnthropicContentBlock {
    Text {
        #[serde(rename = "type")]
        block_type: String,
        text: String,
    },
    ToolUse {
        #[serde(rename = "type")]
        block_type: String,
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        #[serde(rename = "type")]
        block_type: String,
        tool_use_id: String,
        content: String,
    },
}

/// Tool definition in Anthropic format.
#[derive(Debug, Serialize)]
struct AnthropicToolDef {
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_schema: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// SSE response types (Anthropic streaming format)
// ---------------------------------------------------------------------------

/// Top-level SSE event from Anthropic.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicSseEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        message: AnthropicMessageStart,
    },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: usize,
        content_block: AnthropicContentStart,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: usize,
        delta: AnthropicDelta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        index: usize,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[allow(dead_code)]
        delta: AnthropicMessageDeltaMeta,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageStart {
    #[allow(dead_code)]
    id: String,
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    role: String,
    #[allow(dead_code)]
    content: Vec<serde_json::Value>,
}

/// Start of a new content block.
#[derive(Debug, Deserialize)]
struct AnthropicContentStart {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    input: Option<serde_json::Value>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    signature: Option<String>,
}

/// A delta within a content block.
#[derive(Debug, Deserialize)]
struct AnthropicDelta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    partial_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnthropicMessageDeltaMeta {
    #[allow(dead_code)]
    stop_reason: Option<String>,
    #[allow(dead_code)]
    stop_sequence: Option<String>,
}

// ---------------------------------------------------------------------------
// Type conversions: Agent → Anthropic wire format
// ---------------------------------------------------------------------------

/// Convert agent messages + system prompt to Anthropic format.
///
/// Returns `(system_prompt, messages)` where `system_prompt` is `Some`
/// if any non-empty system text was provided.
fn agent_messages_to_anthropic(
    messages: Vec<AgentMessage>,
    system: String,
) -> (Option<String>, Vec<AnthropicMessage>) {
    let system_text = if system.is_empty() {
        None
    } else {
        Some(system)
    };

    let mut out: Vec<AnthropicMessage> = Vec::with_capacity(messages.len());

    for msg in messages {
        match msg {
            AgentMessage::User { text } => {
                out.push(AnthropicMessage {
                    role: "user".into(),
                    content: vec![AnthropicContentBlock::Text {
                        block_type: "text".into(),
                        text,
                    }],
                });
            }
            AgentMessage::Assistant { text, tool_calls } => {
                let mut content: Vec<AnthropicContentBlock> = Vec::new();

                // Add text block if present.
                if let Some(t) = text {
                    if !t.is_empty() {
                        content.push(AnthropicContentBlock::Text {
                            block_type: "text".into(),
                            text: t,
                        });
                    }
                }

                // Add tool_use blocks.
                for tc in tool_calls {
                    content.push(AnthropicContentBlock::ToolUse {
                        block_type: "tool_use".into(),
                        id: tc.id,
                        name: tc.name,
                        input: tc.arguments,
                    });
                }

                out.push(AnthropicMessage {
                    role: "assistant".into(),
                    content,
                });
            }
            AgentMessage::ToolResult {
                tool_call_id,
                output,
            } => {
                out.push(AnthropicMessage {
                    role: "user".into(),
                    content: vec![AnthropicContentBlock::ToolResult {
                        block_type: "tool_result".into(),
                        tool_use_id: tool_call_id,
                        content: output,
                    }],
                });
            }
        }
    }

    (system_text, out)
}

/// Convert [`AgentTool`]s into Anthropic tool definition format.
fn agent_tools_to_anthropic(tools: &[AgentTool]) -> Option<Vec<AnthropicToolDef>> {
    if tools.is_empty() {
        return None;
    }
    Some(
        tools
            .iter()
            .map(|t| AnthropicToolDef {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: Some(t.input_schema.clone()),
            })
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// SSE stream parser (Anthropic format)
// ---------------------------------------------------------------------------

/// Accumulator for a single content block being built across streaming chunks.
#[derive(Debug, Clone, Default)]
struct AnthropicBlockAccumulator {
    block_type: Option<String>,
    id: Option<String>,
    name: Option<String>,
    text: String,
    thinking: String,
    partial_json: String,
}

/// Parse an Anthropic SSE byte stream and emit [`AgentEvent`] values.
async fn parse_anthropic_sse<S, E>(
    byte_stream: S,
    tx: futures::channel::mpsc::UnboundedSender<AgentEvent>,
) -> Result<(), String>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
    E: std::fmt::Display,
{
    let mut byte_stream = byte_stream;
    let mut buf: Vec<u8> = Vec::new();
    // Track blocks by index.
    let mut blocks: Vec<AnthropicBlockAccumulator> = Vec::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("SSE byte stream error: {e}"))?;
        buf.extend_from_slice(&chunk);

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
                continue;
            }

            if !line.starts_with(b"data: ") {
                continue;
            }

            let payload = &line[b"data: ".len()..];

            // Parse the event JSON.
            let event: AnthropicSseEvent = match serde_json::from_slice(payload) {
                Ok(e) => e,
                Err(e) => {
                    let text = String::from_utf8_lossy(payload).into_owned();
                    warn!(error = %e, payload = %text, "Failed to parse Anthropic SSE event");
                    continue;
                }
            };

            match event {
                AnthropicSseEvent::MessageStart { .. } => {
                    debug!("Anthropic message start");
                }
                AnthropicSseEvent::ContentBlockStart {
                    index,
                    content_block,
                } => {
                    // Ensure the blocks vec is large enough.
                    while blocks.len() <= index {
                        blocks.push(AnthropicBlockAccumulator::default());
                    }

                    let block = &mut blocks[index];
                    block.block_type = Some(content_block.block_type);

                    if let Some(id) = content_block.id {
                        block.id = Some(id);
                    }
                    if let Some(name) = content_block.name {
                        block.name = Some(name);
                    }
                }
                AnthropicSseEvent::ContentBlockDelta { index, delta } => {
                    if index >= blocks.len() {
                        continue;
                    }

                    let block = &mut blocks[index];

                    match delta.delta_type.as_str() {
                        "text_delta" => {
                            if let Some(text) = delta.text {
                                block.text.push_str(&text);
                                if tx
                                    .unbounded_send(AgentEvent::TextDelta(text))
                                    .is_err()
                                {
                                    return Ok(());
                                }
                            }
                        }
                        "thinking_delta" => {
                            if let Some(thinking) = delta.thinking {
                                block.thinking.push_str(&thinking);
                                if tx
                                    .unbounded_send(AgentEvent::ThinkingDelta(thinking))
                                    .is_err()
                                {
                                    return Ok(());
                                }
                            }
                        }
                        "input_json_delta" => {
                            if let Some(partial) = delta.partial_json {
                                block.partial_json.push_str(&partial);
                                // Try to parse the accumulated JSON and emit a tool call.
                                if let Some(id) = block.id.as_ref() {
                                    if let Some(name) = block.name.as_ref() {
                                        let args: serde_json::Value =
                                            serde_json::from_str(&block.partial_json)
                                                .unwrap_or(serde_json::Value::Object(
                                                    serde_json::Map::new(),
                                                ));
                                        let call = AgentToolCall {
                                            id: id.clone(),
                                            name: name.clone(),
                                            arguments: args,
                                        };
                                        if tx
                                            .unbounded_send(AgentEvent::ToolCall(call))
                                            .is_err()
                                        {
                                            return Ok(());
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                AnthropicSseEvent::ContentBlockStop { index } => {
                    // Flush any tool call from the completed block.
                    if let Some(block) = blocks.get(index) {
                        if block.block_type.as_deref() == Some("tool_use") {
                            if let (Some(id), Some(name)) = (block.id.as_ref(), block.name.as_ref()) {
                                let args: serde_json::Value =
                                    serde_json::from_str(&block.partial_json)
                                        .unwrap_or(serde_json::Value::Object(
                                            serde_json::Map::new(),
                                        ));
                                let call = AgentToolCall {
                                    id: id.clone(),
                                    name: name.clone(),
                                    arguments: args,
                                };
                                let _ = tx.unbounded_send(AgentEvent::ToolCall(call));
                            }
                        }
                    }
                }
                AnthropicSseEvent::MessageDelta { .. } => {
                    // Message metadata — ignore for streaming.
                }
                AnthropicSseEvent::MessageStop => {
                    debug!("Anthropic message stop");
                    let _ = tx.unbounded_send(AgentEvent::Done);
                    return Ok(());
                }
                AnthropicSseEvent::Ping => {
                    // Heartbeat — ignore.
                }
                AnthropicSseEvent::Unknown => {
                    trace!("Unknown Anthropic SSE event type");
                }
            }
        }
    }

    // Stream ended without explicit message_stop.
    let _ = tx.unbounded_send(AgentEvent::Done);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn make_provider() -> AnthropicProvider {
        let config = ProviderConfig::new("anthropic", "sk-ant-test", None, "claude-sonnet-4");
        AnthropicProvider::new(config)
    }

    #[test]
    fn test_anthropic_provider_creation() {
        let provider = make_provider();
        assert_eq!(
            provider.config.resolved_base_url(),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(provider.config.model, "claude-sonnet-4");
    }

    #[test]
    fn test_messages_url() {
        let provider = make_provider();
        assert_eq!(
            provider.messages_url(),
            "https://api.anthropic.com/v1/messages"
        );
    }

    #[test]
    fn test_messages_url_custom_base() {
        let config = ProviderConfig::new(
            "anthropic",
            "sk-ant-test",
            Some("http://localhost:8080/v1".into()),
            "claude",
        );
        let provider = AnthropicProvider::new(config);
        assert_eq!(provider.messages_url(), "http://localhost:8080/v1/messages");
    }

    #[test]
    fn test_agent_messages_to_anthropic_with_system() {
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

        let (system, wire) = agent_messages_to_anthropic(messages, "You are helpful.".into());
        assert_eq!(system.as_deref(), Some("You are helpful."));
        assert_eq!(wire.len(), 3);

        // User message
        assert_eq!(wire[0].role, "user");
        assert_eq!(wire[0].content.len(), 1);

        // Assistant message with text + tool_use
        assert_eq!(wire[1].role, "assistant");
        assert_eq!(wire[1].content.len(), 2);
        match &wire[1].content[0] {
            AnthropicContentBlock::Text { text, .. } => assert_eq!(text, "world"),
            _ => panic!("expected text block"),
        }
        match &wire[1].content[1] {
            AnthropicContentBlock::ToolUse { id, name, .. } => {
                assert_eq!(id, "c1");
                assert_eq!(name, "greet");
            }
            _ => panic!("expected tool_use block"),
        }

        // Tool result (wrapped as user message in Anthropic format)
        assert_eq!(wire[2].role, "user");
        match &wire[2].content[0] {
            AnthropicContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                assert_eq!(tool_use_id, "c1");
                assert_eq!(content, "Hi Rust!");
            }
            _ => panic!("expected tool_result block"),
        }
    }

    #[test]
    fn test_agent_messages_to_anthropic_no_system() {
        let messages = vec![AgentMessage::user("hello")];
        let (system, wire) = agent_messages_to_anthropic(messages, "".into());
        assert!(system.is_none());
        assert_eq!(wire.len(), 1);
    }

    #[test]
    fn test_agent_tools_to_anthropic_empty() {
        assert!(agent_tools_to_anthropic(&[]).is_none());
    }

    #[test]
    fn test_agent_tools_to_anthropic_non_empty() {
        let tool = AgentTool::new(
            "test_tool",
            "A test tool",
            serde_json::json!({"type": "object"}),
            std::sync::Arc::new(|_| Box::pin(async { Ok("ok".into()) })),
        );
        let wire = agent_tools_to_anthropic(&[tool]).unwrap();
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].name, "test_tool");
    }

    #[test]
    fn test_anthropic_assistant_without_text() {
        let messages = vec![AgentMessage::assistant(
            None,
            vec![AgentToolCall {
                id: "c1".into(),
                name: "tool".into(),
                arguments: serde_json::json!({}),
            }],
        )];

        let (_, wire) = agent_messages_to_anthropic(messages, "".into());
        assert_eq!(wire.len(), 1);
        assert_eq!(wire[0].content.len(), 1);
        match &wire[0].content[0] {
            AnthropicContentBlock::ToolUse { id, .. } => assert_eq!(id, "c1"),
            _ => panic!("expected tool_use block"),
        }
    }

    #[test]
    fn test_anthropic_assistant_empty_text() {
        let messages = vec![AgentMessage::assistant(
            Some("".into()),
            vec![],
        )];

        let (_, wire) = agent_messages_to_anthropic(messages, "".into());
        // Empty text without tool calls — still produces a message with no content?
        // Actually, our code skips empty text, so content will be empty.
        assert_eq!(wire[0].content.len(), 0);
    }

    #[test]
    fn test_content_block_start_tracking() {
        // Verify blocks are tracked by index.
        let mut blocks: Vec<AnthropicBlockAccumulator> = Vec::new();
        while blocks.len() <= 2 {
            blocks.push(AnthropicBlockAccumulator::default());
        }
        blocks[2].block_type = Some("text".into());
        assert_eq!(blocks[2].block_type.as_deref(), Some("text"));
    }

    #[test]
    fn test_input_json_accumulation() {
        let mut block = AnthropicBlockAccumulator::default();
        block.block_type = Some("tool_use".into());
        block.id = Some("call_1".into());
        block.name = Some("get_weather".into());

        block.partial_json.push_str("{\"location\": \"");
        block.partial_json.push_str("NYC\"}");

        let args: serde_json::Value =
            serde_json::from_str(&block.partial_json).unwrap();
        assert_eq!(args, serde_json::json!({"location": "NYC"}));
    }
}
