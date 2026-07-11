//! 🦀 Tua Agent RS v1.0.0

pub mod agent;
pub mod checkpoint;
pub mod config;
pub mod profiles;
pub mod prompts;
pub mod providers;
pub mod review;
pub mod session;
pub mod skills;
pub mod tools;

use clap::{Parser, Subcommand};

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
    Profiles, Config, Check, Test, Review, Sessions, Tui, Bench,
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
            println!("Profile: {} | Self-correct: {} | Review: {}", 
                cfg.default_profile, cfg.self_correction, cfg.review_enabled);
        }
        Some(Commands::Sessions) => println!("Session persistence: enabled"),
        Some(Commands::Tui) => println!("🦀 TUI mode — ratatui interface"),
        Some(Commands::Bench) => println!("🏃 Benchmarks: cargo bench"),
        _ => {
            if let Some(ref prompt) = cli.prompt {
                println!("🦀 Tua Agent RS v1.0.0 | {} | {}", cli.profile, cli.provider);
                println!("💬 {}", prompt);
            } else {
                println!("🦀 Tua Agent RS v1.0.0");
                println!("Commands: profiles | config | check | test | review | sessions | tui | bench");
            }
        }
    }
    Ok(())
}
