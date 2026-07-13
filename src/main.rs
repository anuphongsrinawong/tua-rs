//! 🦀 Tua Agent RS v1.0.0

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tua_rs::agent::{AgentEvent, AgentLoop, AgentMessage, ModelProvider};
use tua_rs::config;
use tua_rs::profiles;
use tua_rs::prompts::rust_system_prompt::RUST_SYSTEM_PROMPT;
use tua_rs::tools::rust_tools;
use tua_rs::tui;

#[derive(Parser)]
#[command(name = "tua-rs", version = "1.0.0")]
struct Cli {
    #[arg(short = 'p', long)]
    prompt: Option<String>,
    #[arg(long, default_value = "rustacean")]
    profile: String,
    #[arg(short = 'm', long, default_value = "deepseek/deepseek-v4-flash")]
    model: String,
    #[arg(long, default_value = "openai")]
    provider: String,
    #[arg(long)]
    resume: Option<String>,
    #[arg(long)]
    no_self_correct: bool,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Profiles,
    Config,
    Check,
    Test,
    Review,
    Sessions,
    Tui,
    Bench,
    /// Compile a crate to WebAssembly
    Wasm {
        /// Path to the crate directory
        path: String,
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },
    /// Complete Rust code by prefix
    Complete {
        /// Prefix to complete (e.g. Vec, impl, Option)
        prefix: String,
    },
    /// Orchestrate multiple agent workers
    Orchestrate {
        /// Task description
        task: String,
        /// Max parallel workers [default: 4]
        #[arg(long, default_value = "4")]
        parallel: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Commands::Profiles) => {
            for p in profiles::ALL_PROFILES {
                println!("  {} {:15} — {}", p.emoji, p.name, p.description);
            }
        }
        Some(Commands::Config) => {
            let cfg = config::load()?;
            println!(
                "Profile: {} | Self-correct: {} | Review: {}",
                cfg.default_profile, cfg.self_correction, cfg.review_enabled
            );
        }
        Some(Commands::Sessions) => println!("Session persistence: enabled"),
        Some(Commands::Tui) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let mut app = tui::App::new();
                app.run()
            })?;
        }
        Some(Commands::Bench) => println!("🏃 Benchmarks: cargo bench"),
        Some(Commands::Wasm { path, release }) => {
            let mode = if release { "release" } else { "debug" };
            println!("🦀 Compiling {} to WebAssembly ({mode})...", path);
            match tua_rs::wasm::compile_to_wasm(&path, release) {
                Ok(output) => println!("✅ Success:\n{output}"),
                Err(e) => eprintln!("❌ Failed:\n{e}"),
            }
        }
        Some(Commands::Orchestrate { task, parallel }) => {
            let result = tua_rs::orchestrator::plan_and_run(&task, parallel);
            if result.failed > 0 {
                eprintln!("⚠️  {} subtask(s) failed", result.failed);
            }
        }
        Some(Commands::Complete { prefix }) => {
            let completer = tua_rs::completion::CodeCompleter::new();
            let results = completer.complete(&prefix);
            if results.is_empty() {
                println!("No completions for '{}'", prefix);
            } else {
                for word in &results {
                    println!("{word}");
                }
            }
        }
        _ => {
            if let Some(ref prompt) = cli.prompt {
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(async {
                    run_cli_agent(&cli, prompt).await
                })?;
            } else {
                println!("🦀 Tua Agent RS v1.0.0");
                println!(
                    "Commands: profiles | config | check | test | review | sessions | tui | bench"
                );
                println!();
                println!("💡 Quick chat:  tua-rs --provider 9router --model ds/deepseek-v4-pro -p \"your task\"");
            }
        }
    }
    Ok(())
}

/// Run the agent in CLI mode: build provider, create AgentLoop, stream to stdout.
async fn run_cli_agent(cli: &Cli, prompt: &str) -> anyhow::Result<()> {
    println!(
        "🦀 Tua Agent RS v1.0.0 | {} | {}",
        cli.profile, cli.provider
    );

    // Build provider from config files (graceful fallback to Mock)
    let provider: Arc<dyn ModelProvider> = tui::load_provider(&cli.provider, &cli.model);

    let system = RUST_SYSTEM_PROMPT;
    let tools = rust_tools();
    let agent = AgentLoop::new(provider, system, tools);

    let messages = vec![AgentMessage::user(prompt)];
    let mut stream = agent.run(messages);

    use futures::StreamExt;
    let mut thinking = false;
    while let Some(event) = stream.next().await {
        match event {
            AgentEvent::ThinkingDelta(text) => {
                if !thinking {
                    eprint!("\n🤔 ");
                    thinking = true;
                }
                eprint!("{}", text);
            }
            AgentEvent::TextDelta(text) => {
                if thinking {
                    eprintln!();
                    thinking = false;
                }
                print!("{}", text);
            }
            AgentEvent::ToolCall(tc) => {
                if thinking {
                    eprintln!();
                    thinking = false;
                }
                eprintln!("\n🔧 {}({})", tc.name, tc.arguments);
            }
            AgentEvent::ToolResult { output, .. } => {
                // Show first 300 chars of tool results
                let preview: String = output.chars().take(300).collect();
                let suffix = if output.len() > 300 { "..." } else { "" };
                eprintln!("📋 {}{}", preview, suffix);
            }
            AgentEvent::Done => {
                if thinking {
                    eprintln!();
                }
                break;
            }
            AgentEvent::Error(e) => {
                if thinking {
                    eprintln!();
                }
                eprintln!("\n❌ Error: {e}");
                break;
            }
        }
    }
    println!(); // final newline
    Ok(())
}
