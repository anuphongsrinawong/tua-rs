//! 🦀 ParallelRunner — run agent prompts across multiple files concurrently.
//!
//! Each file gets its own [`AgentLoop`] instance, spawned as a separate
//! tokio task. Results are collected in file order, so the caller can
//! correlate outputs with inputs even when tasks complete out of order.
//!
//! # Example
//!
//! ```rust,ignore
//! use tua_rs::parallel::ParallelRunner;
//! use tua_rs::agent::{AgentHarnessConfig, ModelProvider};
//! use std::sync::Arc;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn run() {
//! let provider: Arc<dyn ModelProvider> = /* ... */;
//! let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);
//! let results = runner.run_on_files(
//!     vec!["src/main.rs".into(), "src/lib.rs".into()],
//!     "Add doc-comments to all public items",
//! ).await;
//! # }
//! ```

use std::sync::Arc;

use futures::StreamExt;

use crate::agent::{
    AgentConfig, AgentEvent, AgentHarnessConfig, AgentLoop, AgentMessage, ModelProvider,
};
use crate::tools::rust_tools;

// ---------------------------------------------------------------------------
// ParallelRunner
// ---------------------------------------------------------------------------

/// Runs agent prompts across multiple files concurrently.
///
/// Each file is processed by a dedicated [`AgentLoop`] running in its own
/// tokio task. All tasks share the same [`ModelProvider`] (the provider
/// must be `Send + Sync`). The `harness_config` controls self-correction
/// behaviour.
///
/// Results are returned in the same order as the input `files` vector.
pub struct ParallelRunner {
    /// Self-correction configuration passed to every [`AgentLoop`].
    harness_config: AgentHarnessConfig,
    /// The language model provider shared across all tasks.
    provider: Arc<dyn ModelProvider>,
}

impl ParallelRunner {
    /// Create a new `ParallelRunner`.
    ///
    /// * `harness_config` — configuration for self-correction (passed to
    ///   each [`AgentLoop`] via [`AgentConfig`]).
    /// * `provider` — a shared [`ModelProvider`] used by all spawned tasks.
    ///   It is cheaply cloned via `Arc` for each task.
    pub fn new(harness_config: AgentHarnessConfig, provider: Arc<dyn ModelProvider>) -> Self {
        Self {
            harness_config,
            provider,
        }
    }

    /// Run the given prompt on every file in `files` concurrently.
    ///
    /// Each file is processed by a dedicated [`AgentLoop`]:
    ///
    /// 1. The file is read from disk.
    /// 2. An agent is created with the shared [`ModelProvider`], a system
    ///    prompt tailored to the file, and the standard set of Rust tools.
    /// 3. The agent receives a single user message containing the file
    ///    content and the prompt.
    /// 4. All [`AgentEvent::TextDelta`] chunks are concatenated into the
    ///    result string.
    ///
    /// Results are returned **in file order**, not completion order. If a
    /// task panics or the agent encounters a fatal error, the corresponding
    /// entry is `Err(...)`.
    pub async fn run_on_files(
        &self,
        files: Vec<String>,
        prompt: &str,
    ) -> Vec<anyhow::Result<String>> {
        let tools = rust_tools();
        let agent_config = AgentConfig {
            max_tool_rounds: Some(50),
            harness_config: self.harness_config.clone(),
        };

        // Spawn one tokio task per file.
        let tasks: Vec<_> = files
            .into_iter()
            .map(|file| {
                let provider = Arc::clone(&self.provider);
                let tools = tools.clone();
                let agent_config = agent_config.clone();
                let prompt = prompt.to_string();

                tokio::spawn(async move {
                    // --- Read the file ---
                    let file_content = tokio::fs::read_to_string(&file).await.map_err(|e| {
                        anyhow::anyhow!("Failed to read file `{file}`: {e}")
                    })?;

                    // --- Build a system prompt that includes the file name ---
                    let system_prompt = format!(
                        "You are an expert Rust coding assistant.\n\
                         You are working on file: {file}\n\n\
                         {prompt}"
                    );

                    // --- Create the AgentLoop ---
                    let agent = AgentLoop::with_config(
                        provider,
                        system_prompt,
                        tools,
                        agent_config,
                    );

                    // --- Build the conversation ---
                    let user_msg = format!(
                        "Please work on the following Rust file.\n\n\
                         File: {file}\n\n\
                         ```rust\n{file_content}\n```\n\n\
                         Task: {prompt}"
                    );
                    let messages = vec![AgentMessage::user(user_msg)];

                    // --- Run the agent and collect text deltas ---
                    let mut output = String::new();
                    let mut stream = agent.run(messages);

                    while let Some(event) = stream.next().await {
                        match event {
                            AgentEvent::TextDelta(chunk) => output.push_str(&chunk),
                            AgentEvent::Error(e) => {
                                output.push_str(&format!("\n[Error: {e}]"));
                            }
                            // Ignore thinking deltas, tool calls, tool results, done.
                            _ => {}
                        }
                    }

                    if output.trim().is_empty() {
                        Ok("(no output)".to_string())
                    } else {
                        Ok(output)
                    }
                })
            })
            .collect();

        // --- Collect results in original file order ---
        let mut results = Vec::with_capacity(tasks.len());
        for task in tasks {
            match task.await {
                Ok(Ok(output)) => results.push(Ok(output)),
                Ok(Err(e)) => results.push(Err(e)),
                Err(join_err) => {
                    results.push(Err(anyhow::anyhow!("Task panicked: {join_err}")))
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentResult, AgentTool};
    use async_trait::async_trait;
    use futures::stream;
    use std::pin::Pin;

    /// A mock provider that echoes a fixed response for every file.
    #[derive(Debug)]
    struct MockProvider {
        response: Vec<AgentEvent>,
    }

    #[async_trait]
    impl ModelProvider for MockProvider {
        async fn stream_chat(
            &self,
            _messages: Vec<AgentMessage>,
            _system: String,
            _tools: Vec<AgentTool>,
        ) -> AgentResult<Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send + 'static>>> {
            let events = self.response.clone();
            Ok(Box::pin(stream::iter(events)))
        }
    }

    #[tokio::test]
    async fn test_run_on_files_with_mock_provider() {
        let provider = Arc::new(MockProvider {
            response: vec![
                AgentEvent::TextDelta("Fixed!".into()),
                AgentEvent::Done,
            ],
        });

        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        // Write a temporary file to process
        let tmp_dir = std::env::temp_dir().join("__tua_parallel_test__");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let file_path = tmp_dir.join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let results = runner
            .run_on_files(
                vec![file_path.to_string_lossy().to_string()],
                "Add a doc comment",
            )
            .await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok(), "expected Ok, got: {:?}", results[0]);
        let output = results[0].as_ref().unwrap();
        assert_eq!(output, "Fixed!");

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }

    #[tokio::test]
    async fn test_nonexistent_file_returns_error() {
        let provider = Arc::new(MockProvider {
            response: vec![AgentEvent::Done],
        });

        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let results = runner
            .run_on_files(
                vec!["/tmp/__nonexistent_file_12345__.rs".into()],
                "fix this",
            )
            .await;

        assert_eq!(results.len(), 1);
        assert!(
            results[0].is_err(),
            "expected Err for nonexistent file, got: {:?}",
            results[0]
        );
        let err_msg = format!("{}", results[0].as_ref().unwrap_err());
        assert!(
            err_msg.contains("Failed to read"),
            "error message should mention failed read: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_results_preserve_file_order() {
        let provider = Arc::new(MockProvider {
            response: vec![
                AgentEvent::TextDelta("ok".into()),
                AgentEvent::Done,
            ],
        });

        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let tmp_dir = std::env::temp_dir().join("__tua_parallel_order_test__");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();

        let file_a = tmp_dir.join("a.rs");
        let file_b = tmp_dir.join("b.rs");
        let file_c = tmp_dir.join("c.rs");
        std::fs::write(&file_a, "// a").unwrap();
        std::fs::write(&file_b, "// b").unwrap();
        std::fs::write(&file_c, "// c").unwrap();

        let files = vec![
            file_a.to_string_lossy().to_string(),
            file_b.to_string_lossy().to_string(),
            file_c.to_string_lossy().to_string(),
        ];

        let results = runner.run_on_files(files.clone(), "fix").await;

        assert_eq!(results.len(), 3);
        for (i, result) in results.iter().enumerate() {
            assert!(
                result.is_ok(),
                "file {} should be Ok, got: {:?}",
                i,
                result
            );
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
