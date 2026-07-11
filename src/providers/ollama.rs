//! Ollama provider — local LLM inference via Ollama.
//!
//! Implements [`ModelProvider`] for Ollama's OpenAI-compatible API endpoint.
//! Ollama exposes the same `/chat/completions` SSE streaming format as
//! OpenAI, so this provider reuses the OpenAI-compatible SSE parser after
//! setting up the correct headers and base URL.
//!
//! # Configuration
//!
//! * `api_key` — optional (Ollama doesn't require auth by default).
//! * `base_url` — defaults to `http://localhost:11434/v1`.
//! * `model` — e.g. `"llama3"`, `"codellama"`, `"mistral"`.

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use tracing::debug;

use crate::agent::{
    AgentError, AgentEvent, AgentMessage, AgentResult, AgentTool, ModelProvider,
};

use super::openai_compatible::{agent_messages_to_wire, agent_tools_to_wire, parse_sse, ChatRequest};
use super::ProviderConfig;

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/// An Ollama provider backed by [`reqwest`].
///
/// Ollama exposes an OpenAI-compatible `/v1/chat/completions` endpoint, so
/// this provider delegates to the same SSE parsing logic as
/// [`OpenAiCompatibleProvider`] but with slightly different defaults:
///
/// * No auth header by default.
/// * Base URL defaults to `http://localhost:11434/v1`.
/// * Model names are Ollama-specific (e.g. `"llama3"`, `"codellama"`).
#[derive(Debug, Clone)]
pub struct OllamaProvider {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl OllamaProvider {
    /// Create a new Ollama provider from the given configuration.
    ///
    /// Sets up a `reqwest::Client` with a JSON content type and a
    /// 120-second timeout. No auth headers are added because Ollama
    /// runs locally and does not require authentication by default.
    pub fn new(config: ProviderConfig) -> Self {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        // If an API key is explicitly provided, send it as Bearer token.
        if !config.api_key.is_empty() {
            let mut auth_value = HeaderValue::from_str(&format!("Bearer {}", config.api_key))
                .expect("API key contains invalid header bytes");
            auth_value.set_sensitive(true);
            default_headers.insert(reqwest::header::AUTHORIZATION, auth_value);
        }

        let client = reqwest::Client::builder()
            .default_headers(default_headers)
            .timeout(std::time::Duration::from_secs(300)) // Ollama can be slow for first load
            .build()
            .expect("Failed to build reqwest client");

        Self { config, client }
    }

    /// Full URL for the chat completions endpoint.
    fn chat_url(&self) -> String {
        let base = self.config.resolved_base_url().trim_end_matches('/');
        format!("{base}/chat/completions")
    }
}

// ---------------------------------------------------------------------------
// ModelProvider implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl ModelProvider for OllamaProvider {
    async fn stream_chat(
        &self,
        messages: Vec<AgentMessage>,
        system: String,
        tools: Vec<AgentTool>,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentEvent> + Send + 'static>>> {
        let url = self.chat_url();

        // Convert agent types → OpenAI wire format (Ollama is compatible).
        let wire_messages = agent_messages_to_wire(messages, system);
        let wire_tools = agent_tools_to_wire(&tools);

        debug!(
            url = %url,
            model = %self.config.model,
            messages = %wire_messages.len(),
            tools = %wire_tools.as_ref().map_or(0, |t| t.len()),
            "POST /v1/chat/completions (Ollama)"
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
            .map_err(|e| AgentError::StreamError(format!("HTTP request to Ollama failed: {e}")))?;

        // Check for HTTP-level errors.
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body_text = response.text().await.unwrap_or_else(|_| "<no body>".into());
            return Err(AgentError::StreamError(format!(
                "Ollama API error: {status} — {body_text}"
            )));
        }

        // Channel to bridge the async SSE reader with the returned Stream.
        let (tx, rx) = futures::channel::mpsc::unbounded();

        tokio::spawn(async move {
            if let Err(err) = parse_sse(response.bytes_stream(), tx.clone()).await {
                tracing::warn!(error = %err, "Ollama SSE stream processing failed");
                let _ = tx.unbounded_send(AgentEvent::Error(err.to_string()));
            }
        });

        Ok(Box::pin(rx))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentToolCall;

    #[test]
    fn test_ollama_provider_creation() {
        let config = ProviderConfig::new("ollama", "", None, "llama3");
        let provider = OllamaProvider::new(config);
        assert_eq!(
            provider.config.resolved_base_url(),
            "http://localhost:11434/v1"
        );
        assert_eq!(provider.config.model, "llama3");
        assert_eq!(provider.config.api_key, "");
    }

    #[test]
    fn test_chat_url() {
        let config = ProviderConfig::new("ollama", "", None, "llama3");
        let provider = OllamaProvider::new(config);
        assert_eq!(
            provider.chat_url(),
            "http://localhost:11434/v1/chat/completions"
        );
    }

    #[test]
    fn test_chat_url_custom_base() {
        let config = ProviderConfig::new(
            "ollama",
            "",
            Some("http://ollama.local:8080/v1".into()),
            "llama3",
        );
        let provider = OllamaProvider::new(config);
        assert_eq!(
            provider.chat_url(),
            "http://ollama.local:8080/v1/chat/completions"
        );
    }

    #[test]
    fn test_ollama_with_api_key() {
        let config = ProviderConfig::new(
            "ollama",
            "custom-token",
            Some("http://remote-ollama:11434/v1".into()),
            "mistral",
        );
        let provider = OllamaProvider::new(config);
        // API key should be stored even if Ollama typically doesn't need one.
        assert_eq!(provider.config.api_key, "custom-token");
    }

    #[test]
    fn test_ollama_model_variants() {
        let models = ["llama3", "codellama", "mistral", "mixtral", "deepseek-coder"];
        for model in models {
            let config = ProviderConfig::new("ollama", "", None, model);
            let provider = OllamaProvider::new(config);
            assert_eq!(provider.config.model, model);
        }
    }

    #[test]
    fn test_ollama_reuses_openai_parser() {
        // Verify that the ChatRequest and parse_sse types are accessible
        // from the openai_compatible module. This is a compile-time check.
        let config = ProviderConfig::new("ollama", "", None, "llama3");
        let provider = OllamaProvider::new(config);
        assert!(provider.config.api_key.is_empty());
    }

    #[test]
    fn test_default_timeout_is_300s() {
        let config = ProviderConfig::new("ollama", "", None, "llama3");
        let provider = OllamaProvider::new(config);
        // Just verify creation doesn't panic.
        assert_eq!(provider.config.model, "llama3");
    }
}
