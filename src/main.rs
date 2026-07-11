//! 🦀 Tua Agent RS v0.3.0 — Rust-native AI coding agent.
//!
//! Features: 14 tools, 8 profiles, self-correction, checkpointing, review, caching.

pub mod agent;
pub mod checkpoint;
pub mod config;
pub mod profiles;
pub mod prompts;
pub mod providers;
pub mod review;
pub mod skills;
pub mod tools;
pub mod tui;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tua-rs", version = "0.3.0")]
struct Cli {
    #[arg(short = 'p', long)]
    prompt: Option<String>,
    #[arg(long, default_value = "rustacean")]
    profile: String,
    #[arg(short = 'm', long, default_value = "deepseek/deepseek-v4-flash")]
    model: String,
    #[arg(long)]
    cwd: Option<String>,
    #[arg(long)]
    no_self_correct: bool,
    #[arg(long)]
    no_checkpoint: bool,
    #[arg(long)]
    no_review: bool,
    #[arg(long, default_value = "ask")]
    permission: String,
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
    Skills,
    Dashboard,
    Tui,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Profiles) => {
            println!("🦀 Tua Agent RS v0.3.0 — 8 Rust Coding Profiles\n");
            for p in profiles::ALL_PROFILES {
                println!("  {} {:15} — {}", p.emoji, p.name, p.description);
            }
        }
        Some(Commands::Config) => {
            let cfg = config::load()?;
            println!("Profile:       {}", cfg.default_profile);
            println!("Self-correct:  {}", cfg.self_correction);
            println!("Checkpoint:    {}", cfg.checkpoint_enabled);
            println!("Cache:         {}", cfg.prompt_caching);
            println!("Review:        {}", cfg.review_enabled);
            println!("Context limit: {} tokens", cfg.context_limit);
        }
        Some(Commands::Check) => {
            std::process::Command::new("cargo").arg("check").status()?;
        }
        Some(Commands::Test) => {
            std::process::Command::new("cargo").args(["test"]).status()?;
        }
        Some(Commands::Dashboard) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let app = tua_rs::dashboard::dashboard_router();
                let addr = "0.0.0.0:8765";
                println!("🦀 Tua Agent Dashboard — http://{addr}");
                let listener = tokio::net::TcpListener::bind(addr).await?;
                axum::serve(listener, app).await
            })?;
        }
        Some(Commands::Review) => {
            let findings = review::review_edits(&[], cli.cwd.as_deref());
            println!("{}", review::format_review(&findings));
        }
        Some(Commands::Skills) => {
            let prompt = skills::format_skills_for_prompt();
            println!("{}", prompt);
        }
        Some(Commands::Tui) => {
            let mut app = tui::App::new();
            if let Err(e) = tui::run_tui(&mut app) {
                eprintln!("❌ TUI error: {e}");
            }
        }
        None => {
            if let Some(ref prompt) = cli.prompt {
                println!("🦀  Tua Agent RS v0.3.0 — profile: {}", cli.profile);
                println!("🧠 self-correct={} checkpoint={} review={}", 
                    !cli.no_self_correct, !cli.no_checkpoint, !cli.no_review);
                println!("💬 {}", prompt);
                
                // Build system prompt
                let tools = tools::rust_tools();
                let tool_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
                let system = prompts::build_rust_system_prompt(&tool_names, &cli.profile);
                println!("📝 System prompt: {} chars", system.len());
                
                // Create provider
                let provider_config = providers::ProviderConfig {
                    api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                    base_url: "http://127.0.0.1:20128/v1".to_string(),
                    model: cli.model.clone(),
                };
                let _provider = providers::OpenAiCompatibleProvider::new(provider_config);
                
                println!("✅ Agent ready — connect provider and run loop");
            } else {
                println!("🦀  Tua Agent RS v0.3.0");
                println!("Commands: profiles | config | check | test | review | skills | dashboard | tui");
                println!("Flags: --no-self-correct --no-checkpoint --no-review");
                println!("Usage: tua-rs -p \"your Rust coding task\"");
            }
        }
    }
    Ok(())
}
