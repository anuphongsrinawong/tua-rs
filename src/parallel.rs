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
// split_work — divide items across N workers as evenly as possible
// ---------------------------------------------------------------------------

/// Split a slice of items into `num_workers` chunks as evenly as possible.
///
/// Each worker receives either `base` or `base + 1` items, where
/// `base = items.len() / num_workers`. The first `remainder` workers
/// (where `remainder = items.len() % num_workers`) get one extra item.
///
/// This is a zero-cost abstraction — it only does arithmetic and slicing;
/// no allocations beyond the outer `Vec<&[T]>`.
///
/// # Panics
///
/// Panics if `num_workers == 0`.
///
/// # Example
///
/// ```
/// use tua_rs::parallel::split_work;
///
/// let items = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
/// let chunks = split_work(&items, 3);
/// assert_eq!(chunks.len(), 3);
/// assert_eq!(chunks[0], &[1, 2, 3, 4]); // base+1
/// assert_eq!(chunks[1], &[5, 6, 7]);    // base
/// assert_eq!(chunks[2], &[8, 9, 10]);   // base
/// ```
///
/// # Edge cases
///
/// ```
/// use tua_rs::parallel::split_work;
///
/// // Empty input
/// let empty: Vec<i32> = vec![];
/// let chunks = split_work(&empty, 3);
/// assert_eq!(chunks.len(), 3);
/// assert!(chunks.iter().all(|c| c.is_empty()));
///
/// // More workers than items
/// let items = vec!["a", "b"];
/// let chunks = split_work(&items, 5);
/// assert_eq!(chunks.len(), 5);
/// assert_eq!(chunks[0], &["a"]);
/// assert_eq!(chunks[1], &["b"]);
/// assert!(chunks[2..].iter().all(|c| c.is_empty()));
/// ```
pub fn split_work<T>(items: &[T], num_workers: usize) -> Vec<&[T]> {
    assert!(num_workers > 0, "num_workers must be > 0");

    let len = items.len();
    let base = len / num_workers;
    let remainder = len % num_workers;

    let mut chunks = Vec::with_capacity(num_workers);
    let mut start = 0usize;

    for i in 0..num_workers {
        // Workers 0..remainder get base+1 items, the rest get base items.
        let chunk_size = base + if i < remainder { 1 } else { 0 };
        let end = start + chunk_size;
        chunks.push(&items[start..end]);
        start = end;
    }

    chunks
}

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
                    let file_content = tokio::fs::read_to_string(&file)
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to read file `{file}`: {e}"))?;

                    // --- Build a system prompt that includes the file name ---
                    let system_prompt = format!(
                        "You are an expert Rust coding assistant.\n\
                         You are working on file: {file}\n\n\
                         {prompt}"
                    );

                    // --- Create the AgentLoop ---
                    let agent =
                        AgentLoop::with_config(provider, system_prompt, tools, agent_config);

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
                Err(join_err) => results.push(Err(anyhow::anyhow!("Task panicked: {join_err}"))),
            }
        }

        results
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentResult, AgentTool};
    use async_trait::async_trait;
    use futures::stream;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        ) -> AgentResult<Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send + 'static>>>
        {
            let events = self.response.clone();
            Ok(Box::pin(stream::iter(events)))
        }
    }

    /// A mock provider that tracks how many times `stream_chat` was called.
    #[derive(Debug)]
    struct CallCountingProvider {
        response: Vec<AgentEvent>,
        call_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ModelProvider for CallCountingProvider {
        async fn stream_chat(
            &self,
            _messages: Vec<AgentMessage>,
            _system: String,
            _tools: Vec<AgentTool>,
        ) -> AgentResult<Pin<Box<dyn futures::Stream<Item = AgentEvent> + Send + 'static>>>
        {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            let events = self.response.clone();
            Ok(Box::pin(stream::iter(events)))
        }
    }

    /// Helper: write a temporary file, run the runner, return results.
    async fn run_on_temp_file(content: &str, prompt: &str) -> Vec<anyhow::Result<String>> {
        let provider = Arc::new(MockProvider {
            response: vec![AgentEvent::TextDelta("Processed!".into()), AgentEvent::Done],
        });
        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let tmp = std::env::temp_dir().join(format!("__tua_parallel_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let file_path = tmp.join("test.rs");
        std::fs::write(&file_path, content).unwrap();
        let result = runner
            .run_on_files(vec![file_path.to_string_lossy().to_string()], prompt)
            .await;

        let _ = std::fs::remove_dir_all(&tmp);
        result
    }

    // =====================================================================
    // split_work tests (7 tests)
    // =====================================================================

    #[test]
    fn test_split_work_even_distribution() {
        let items: Vec<i32> = (1..=12).collect();
        let chunks = split_work(&items, 3);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &[1, 2, 3, 4]);
        assert_eq!(chunks[1], &[5, 6, 7, 8]);
        assert_eq!(chunks[2], &[9, 10, 11, 12]);
    }

    #[test]
    fn test_split_work_uneven_distribution() {
        // 10 items across 3 workers → 4, 3, 3
        let items: Vec<i32> = (1..=10).collect();
        let chunks = split_work(&items, 3);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &[1, 2, 3, 4]);
        assert_eq!(chunks[1], &[5, 6, 7]);
        assert_eq!(chunks[2], &[8, 9, 10]);
    }

    #[test]
    fn test_split_work_heavily_uneven() {
        // 5 items across 10 workers → 1, 1, 1, 1, 1, 0, 0, 0, 0, 0
        let items: Vec<i32> = (1..=5).collect();
        let chunks = split_work(&items, 10);

        assert_eq!(chunks.len(), 10);
        assert_eq!(chunks[0], &[1]);
        assert_eq!(chunks[1], &[2]);
        assert_eq!(chunks[2], &[3]);
        assert_eq!(chunks[3], &[4]);
        assert_eq!(chunks[4], &[5]);
        assert!(chunks[5..].iter().all(|c| c.is_empty()));
    }

    #[test]
    fn test_split_work_empty_input() {
        let empty: Vec<i32> = vec![];
        let chunks = split_work(&empty, 4);

        assert_eq!(chunks.len(), 4);
        assert!(chunks.iter().all(|c| c.is_empty()));
    }

    #[test]
    fn test_split_work_single_item() {
        let items = vec![42];
        let chunks = split_work(&items, 3);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &[42]);
        assert!(chunks[1].is_empty());
        assert!(chunks[2].is_empty());
    }

    #[test]
    fn test_split_work_single_worker() {
        let items: Vec<i32> = (1..=100).collect();
        let chunks = split_work(&items, 1);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 100);
        assert_eq!(chunks[0], &items[..]);
    }

    #[test]
    fn test_split_work_preserves_total() {
        for len in 0..=50 {
            for workers in 1..=10 {
                let items: Vec<i32> = (0..len).collect();
                let chunks = split_work(&items, workers);

                // Concatenate all chunks and verify they reproduce the original
                let reconstructed: Vec<i32> =
                    chunks.iter().flat_map(|c| c.iter()).copied().collect();
                assert_eq!(
                    reconstructed, items,
                    "failed for len={len}, workers={workers}"
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "num_workers must be > 0")]
    fn test_split_work_zero_workers_panics() {
        let items: Vec<i32> = vec![1, 2, 3];
        let _chunks = split_work(&items, 0);
    }

    // =====================================================================
    // ParallelRunner tests (7 tests)
    // =====================================================================

    #[tokio::test]
    async fn test_run_on_files_empty_input_returns_empty_results() {
        let provider = Arc::new(MockProvider {
            response: vec![AgentEvent::Done],
        });
        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let results = runner.run_on_files(vec![], "fix").await;
        assert!(results.is_empty(), "no files → no results");
    }

    #[tokio::test]
    async fn test_run_on_files_single_file_succeeds() {
        let results = run_on_temp_file("fn main() {}", "Add a doc comment").await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok(), "expected Ok, got: {:?}", results[0]);
        assert_eq!(results[0].as_ref().unwrap(), "Processed!");
    }

    #[tokio::test]
    async fn test_run_on_files_empty_file_content() {
        let results = run_on_temp_file("", "fix me").await;
        assert_eq!(results.len(), 1);
        assert!(results[0].is_ok());
        assert_eq!(results[0].as_ref().unwrap(), "Processed!");
    }

    #[tokio::test]
    async fn test_run_on_files_multiple_files_all_succeed() {
        let provider = Arc::new(MockProvider {
            response: vec![AgentEvent::TextDelta("ok".into()), AgentEvent::Done],
        });
        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let tmp = std::env::temp_dir().join(format!("__tua_parallel_multi_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let files: Vec<String> = (0..5)
            .map(|i| {
                let path = tmp.join(format!("file_{i}.rs"));
                std::fs::write(&path, format!("// file {i}")).unwrap();
                path.to_string_lossy().to_string()
            })
            .collect();

        let results = runner.run_on_files(files.clone(), "fix").await;
        assert_eq!(results.len(), 5);
        for (i, result) in results.iter().enumerate() {
            assert!(result.is_ok(), "file {i} should be Ok, got: {:?}", result);
            assert_eq!(result.as_ref().unwrap(), "ok");
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_run_on_files_mixed_success_and_failure() {
        let provider = Arc::new(MockProvider {
            response: vec![AgentEvent::TextDelta("ok".into()), AgentEvent::Done],
        });
        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let tmp = std::env::temp_dir().join(format!("__tua_parallel_mixed_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let real_file = tmp.join("real.rs");
        std::fs::write(&real_file, "fn main() {}").unwrap();

        let files = vec![
            real_file.to_string_lossy().to_string(), // exists → Ok
            "/tmp/__nonexistent_abc123__.rs".into(), // missing → Err
            real_file.to_string_lossy().to_string(), // exists → Ok
        ];

        let results = runner.run_on_files(files.clone(), "fix").await;
        assert_eq!(results.len(), 3);

        assert!(results[0].is_ok(), "first file should succeed");
        assert!(results[1].is_err(), "second file (missing) should fail");
        assert!(results[2].is_ok(), "third file should succeed");

        let err_msg = format!("{}", results[1].as_ref().unwrap_err());
        assert!(
            err_msg.contains("Failed to read"),
            "error should mention file read failure: {err_msg}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_run_on_files_results_preserve_file_order() {
        let provider = Arc::new(MockProvider {
            response: vec![AgentEvent::TextDelta("ok".into()), AgentEvent::Done],
        });

        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let tmp = std::env::temp_dir().join(format!("__tua_parallel_order_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let file_a = tmp.join("a.rs");
        let file_b = tmp.join("b.rs");
        let file_c = tmp.join("c.rs");
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
            assert!(result.is_ok(), "file {i} should be Ok, got: {:?}", result);
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_run_on_files_nonexistent_file_returns_error() {
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
    async fn test_run_on_files_each_file_gets_own_provider_call() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(CallCountingProvider {
            response: vec![AgentEvent::TextDelta("ok".into()), AgentEvent::Done],
            call_count: Arc::clone(&call_count),
        });

        let runner = ParallelRunner::new(AgentHarnessConfig::default(), provider);

        let tmp = std::env::temp_dir().join(format!("__tua_parallel_count_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let files: Vec<String> = (0..4)
            .map(|i| {
                let path = tmp.join(format!("count_{i}.rs"));
                std::fs::write(&path, format!("// count {i}")).unwrap();
                path.to_string_lossy().to_string()
            })
            .collect();

        let results = runner.run_on_files(files, "fix").await;
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(call_count.load(Ordering::SeqCst), 4);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // =====================================================================
    // Struct-trait tests
    // =====================================================================

    #[test]
    fn test_parallel_runner_is_send() {
        fn check_send<T: Send>() {}
        check_send::<ParallelRunner>();
    }

    #[test]
    fn test_parallel_runner_is_sync() {
        fn check_sync<T: Sync>() {}
        check_sync::<ParallelRunner>();
    }
}
