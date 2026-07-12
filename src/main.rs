//! 🦀 Tua Agent RS v1.0.0

use clap::{Parser, Subcommand};
use tua_rs::{config, profiles};

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
    /// Generate test skeletons from a Rust source file
        /// Path to the Rust source file to analyze
        path: String,
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
                let mut app = tua_rs::tui::App::new();
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
            println!("🦀 Generating tests for: {}\n", path);
            if tests.is_empty() {
                println!("⚠️  No public functions found in '{}'", path);
            } else {
                for (i, test) in tests.iter().enumerate() {
                    if i > 0 {
                        println!();
                    }
                    println!("{test}");
                }
                println!("\n✅ Generated {} test(s)", tests.len());
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
                println!(
                    "🦀 Tua Agent RS v1.0.0 | {} | {}",
                    cli.profile, cli.provider
                );
                println!("💬 {}", prompt);
            } else {
                println!("🦀 Tua Agent RS v1.0.0");
                println!(
                    "Commands: profiles | config | check | test | review | sessions | tui | bench"
                );
            }
        }
    }
    Ok(())
}
