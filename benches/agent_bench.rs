//! 🦀 Criterion benchmarks for the Tua Agent RS core.
//!
//! Benchmarks:
//!   1. `agent_harness_run` — AgentLoop::run with mock provider at 10/100/1000 events
//!   2. `tool_executor_cargo_check` — ToolExecutor dispatch for `cargo check`
//!   3. `sse_parser_throughput` — SSE byte-stream parser throughput
//!
//! Run:  cargo bench

use std::sync::Arc;
use std::time::Duration;

use criterion::{
    black_box, criterion_group, criterion_main, AxisScale, BatchSize, BenchmarkId, Criterion,
    PlotConfiguration, SamplingMode, Throughput,
};
use futures::channel::mpsc;
use futures::StreamExt;
use tokio::runtime::Runtime;

use tua_rs::agent::{AgentEvent, AgentLoop};
use tua_rs::providers::mock::{MockProvider, MockProviderBuilder};
use tua_rs::providers::openai_compatible::parse_sse;
use tua_rs::tools::rust_tools;

// ===========================================================================
//  Helper: build a single-threaded runtime shared across all benchmarks
// ===========================================================================

fn rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

// ===========================================================================
//  Benchmark 1: AgentHarness::run — mock provider throughput
// ===========================================================================

/// Build a mock provider that emits `count` text deltas followed by `Done`.
fn make_text_provider(count: usize) -> MockProvider {
    let mut builder = MockProviderBuilder::new().delay(Duration::ZERO);
    for i in 0..count {
        builder = builder.text_delta(format!("chunk_{i} "));
    }
    builder.done().build()
}

/// Build a mock provider that emits a mix of text, thinking, tool-call, and
/// tool-result events (simulating a realistic multi-round agent exchange).
fn make_mixed_provider(count: usize) -> MockProvider {
    let mut builder = MockProviderBuilder::new().delay(Duration::ZERO);
    for i in 0..count {
        match i % 5 {
            0 => {
                builder = builder.text_delta(format!("Step {i}: "));
            }
            1 => {
                builder = builder.thinking_delta(format!("reasoning {i}…"));
            }
            2 => {
                builder = builder.tool_call(
                    "cargo",
                    serde_json::json!({"subcommand": "check", "args": []}),
                );
            }
            3 => {
                builder = builder
                    .tool_result(format!("call_{i}"), format!("cargo check output chunk {i}"));
            }
            4 => {
                builder = builder.text_delta(format!("Result {i}\n"));
            }
            _ => unreachable!(),
        }
    }
    builder.done().build()
}

fn bench_agent_harness_run(c: &mut Criterion) {
    let mut group = c.benchmark_group("agent_harness_run");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    group.sampling_mode(SamplingMode::Auto);
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(10));

    for &count in &[10usize, 100, 1000] {
        // --- Pure text events ---
        group.bench_with_input(BenchmarkId::new("text_only", count), &count, |b, &count| {
            let runtime = rt();
            let provider = Arc::new(make_text_provider(count));
            let loop_ = Arc::new(AgentLoop::new(provider, "system prompt", vec![]));

            b.to_async(runtime).iter_custom(|iters| {
                let loop_ = Arc::clone(&loop_);
                async move {
                    let start = std::time::Instant::now();
                    for _ in 0..iters {
                        let events: Vec<AgentEvent> = black_box(loop_.run(vec![])).collect().await;
                        assert!(events.len() >= 2); // at least text + Done
                    }
                    start.elapsed()
                }
            });
        });

        // --- Mixed events (realistic multi-round simulation) ---
        group.bench_with_input(
            BenchmarkId::new("mixed_events", count),
            &count,
            |b, &count| {
                let runtime = rt();
                let provider = Arc::new(make_mixed_provider(count));
                let loop_ = Arc::new(AgentLoop::new(provider, "system prompt", vec![]));

                b.to_async(runtime).iter_custom(|iters| {
                    let loop_ = Arc::clone(&loop_);
                    async move {
                        let start = std::time::Instant::now();
                        for _ in 0..iters {
                            let events: Vec<AgentEvent> =
                                black_box(loop_.run(vec![])).collect().await;
                            assert!(events.len() >= 2);
                        }
                        start.elapsed()
                    }
                });
            },
        );
    }

    group.finish();
}

// ===========================================================================
//  Benchmark 2: ToolExecutor for `cargo check`
// ===========================================================================

fn bench_tool_executor_cargo_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("tool_executor_cargo_check");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    group.sampling_mode(SamplingMode::Auto);
    group.sample_size(15); // fewer samples because each runs a real subprocess
    group.measurement_time(Duration::from_secs(15));

    let tools = rust_tools();
    let cargo_tool = tools
        .iter()
        .find(|t| t.name == "cargo")
        .expect("cargo tool not found")
        .clone();

    let args = serde_json::json!({
        "subcommand": "check",
        "args": ["--color", "never"]
    });

    group.bench_with_input(BenchmarkId::new("execute", 1), &(), |b, _| {
        let runtime = rt();
        let cargo_tool = cargo_tool.clone();
        let args = args.clone();

        b.to_async(runtime).iter(|| async {
            let result = black_box(cargo_tool.execute(args.clone())).await;
            // Don't unwrap: cargo check may fail in CI but the benchmark
            // still measures dispatch + subprocess overhead.
            let _ = result;
        });
    });

    group.finish();
}

// ===========================================================================
//  Benchmark 3: SSE parser throughput
// ===========================================================================

/// Generate a byte stream of `count` SSE chunks simulating realistic
/// OpenAI-compatible `/chat/completions` deltas.
fn make_sse_chunks(count: usize) -> Vec<bytes::Bytes> {
    let mut chunks = Vec::with_capacity(count + 1);

    for i in 0..count {
        let content = format!("token_{i}_");
        let json = serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": { "content": content },
                "finish_reason": null
            }]
        });
        let line = format!("data: {}\n", serde_json::to_string(&json).unwrap());
        chunks.push(bytes::Bytes::from(line));
    }

    // Terminator
    chunks.push(bytes::Bytes::from_static(b"data: [DONE]\n"));
    chunks
}

/// Generate SSE chunks that include tool-call deltas (mimicking function
/// calling chunks from the model).
fn make_sse_tool_call_chunks(count: usize) -> Vec<bytes::Bytes> {
    let mut chunks = Vec::with_capacity(count + 2);

    // First chunk: tool call header with id and name
    let first = serde_json::json!({
        "choices": [{
            "index": 0,
            "delta": {
                "tool_calls": [{
                    "index": 0,
                    "id": "call_bench_1",
                    "type": "function",
                    "function": { "name": "cargo", "arguments": "" }
                }]
            },
            "finish_reason": null
        }]
    });
    chunks.push(bytes::Bytes::from(format!(
        "data: {}\n",
        serde_json::to_string(&first).unwrap()
    )));

    // Middle chunks: incremental arguments
    for i in 0..count {
        let json = serde_json::json!({
            "choices": [{
                "index": 0,
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "function": { "arguments": format!("arg_part_{i}_") }
                    }]
                },
                "finish_reason": null
            }]
        });
        let line = format!("data: {}\n", serde_json::to_string(&json).unwrap());
        chunks.push(bytes::Bytes::from(line));
    }

    // Terminator
    chunks.push(bytes::Bytes::from_static(b"data: [DONE]\n"));
    chunks
}

fn bench_sse_parser_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("sse_parser_throughput");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));
    group.sampling_mode(SamplingMode::Auto);
    group.sample_size(30);
    group.measurement_time(Duration::from_secs(10));

    for &count in &[10usize, 100, 1000] {
        // --- Text delta chunks ---
        let chunks = make_sse_chunks(count);
        let total_bytes: usize = chunks.iter().map(|c| c.len()).sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));

        group.bench_with_input(
            BenchmarkId::new("text_deltas", count),
            &chunks,
            |b, chunks| {
                let runtime = rt();
                let owned_chunks: Vec<bytes::Bytes> = chunks.iter().map(|c| c.clone()).collect();

                b.to_async(runtime).iter_batched(
                    || owned_chunks.clone(),
                    |chunks| async move {
                        let (tx, mut rx) = mpsc::unbounded::<AgentEvent>();
                        let stream = futures::stream::iter(chunks.into_iter().map(Ok::<_, String>));

                        tokio::spawn(async move {
                            let _ = parse_sse(stream, tx).await;
                        });

                        let mut event_count = 0usize;
                        while rx.next().await.is_some() {
                            event_count += 1;
                        }
                        black_box(event_count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // --- Tool-call chunks ---
        let tool_chunks = make_sse_tool_call_chunks(count);
        let total_bytes: usize = tool_chunks.iter().map(|c| c.len()).sum();
        group.throughput(Throughput::Bytes(total_bytes as u64));

        group.bench_with_input(
            BenchmarkId::new("tool_call_deltas", count),
            &tool_chunks,
            |b, chunks| {
                let runtime = rt();
                let owned_chunks: Vec<bytes::Bytes> = chunks.iter().map(|c| c.clone()).collect();

                b.to_async(runtime).iter_batched(
                    || owned_chunks.clone(),
                    |chunks| async move {
                        let (tx, mut rx) = mpsc::unbounded::<AgentEvent>();
                        let stream = futures::stream::iter(chunks.into_iter().map(Ok::<_, String>));

                        tokio::spawn(async move {
                            let _ = parse_sse(stream, tx).await;
                        });

                        let mut event_count = 0usize;
                        while rx.next().await.is_some() {
                            event_count += 1;
                        }
                        black_box(event_count);
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ===========================================================================
//  Criterion harness
// ===========================================================================

criterion_group!(
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(2))
        .noise_threshold(0.05);
    targets = bench_agent_harness_run, bench_tool_executor_cargo_check, bench_sse_parser_throughput
);
criterion_main!(benches);
