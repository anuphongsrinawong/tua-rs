//! 🛠️ Interactive setup wizard for first-time Tua configuration.
//!
//! Walks the user through configuring providers, API keys, models, and
//! preferences using [`inquire`] for beautiful interactive prompts,
//! then writes the necessary config files.
//!
//! All paths use [`dirs::config_dir`] so this works on Linux, macOS, and Windows.

use inquire::{Confirm, Select, Text};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run the interactive setup wizard.
///
/// Returns `Ok(true)` if the user wants to launch the TUI afterward.
pub fn run() -> anyhow::Result<bool> {
    println!();
    println!("🦀  Tua Agent Setup Wizard");
    println!("════════════════════════════");
    println!();

    // ── Step 1: Provider ──────────────────────────────────────────────
    let provider_choice = Select::new(
        "📡  Choose your AI provider:",
        vec![
            "9Router (local proxy — recommended, free)",
            "DeepSeek (direct API)",
            "OpenAI",
            "Anthropic (Claude)",
            "Ollama (local)",
            "Custom OpenAI-compatible endpoint",
        ],
    )
    .with_starting_cursor(0)
    .prompt()
    .unwrap_or("9Router (local proxy — recommended, free)");

    let provider_idx = match provider_choice {
        s if s.starts_with("9Router") => 0,
        s if s.starts_with("DeepSeek") => 1,
        s if s.starts_with("OpenAI") => 2,
        s if s.starts_with("Anthropic") => 3,
        s if s.starts_with("Ollama") => 4,
        _ => 5,
    };

    let (provider_name, provider_kind, default_base_url, default_model, needs_key) =
        match provider_idx {
            0 => (
                "9router",
                "openai-compatible",
                "http://127.0.0.1:20128/v1",
                "ds/deepseek-v4-pro",
                true,
            ),
            1 => (
                "deepseek",
                "openai-compatible",
                "https://api.deepseek.com/v1",
                "deepseek-chat",
                true,
            ),
            2 => (
                "openai",
                "openai-compatible",
                "https://api.openai.com/v1",
                "gpt-4o",
                true,
            ),
            3 => (
                "anthropic",
                "openai-compatible",
                "https://api.anthropic.com/v1",
                "claude-sonnet-4-20250514",
                true,
            ),
            4 => (
                "ollama",
                "openai-compatible",
                "http://localhost:11434/v1",
                "llama3",
                false,
            ),
            _ => ("custom", "openai-compatible", "", "gpt-4o", true),
        };

    let mut base_url = default_base_url.to_string();
    if provider_idx == 5 {
        base_url = Text::new("Base URL:")
            .with_default("http://localhost:8080/v1")
            .prompt()
            .unwrap_or_else(|_| "http://localhost:8080/v1".into());
    }

    // ── Step 2: API Key ───────────────────────────────────────────────
    let api_key = if needs_key {
        let key = Text::new(&format!("API key for {provider_name}:"))
            .with_default("free")
            .prompt()
            .unwrap_or_else(|_| "free".into());
        if key.is_empty() {
            "free".to_string()
        } else {
            key
        }
    } else {
        "ollama".to_string()
    };

    // ── Step 3: Model ──────────────────────────────────────────────────
    let model = Text::new("Default model:")
        .with_default(default_model)
        .prompt()
        .unwrap_or_else(|_| default_model.into());

    // ── Step 4: TUI Theme ──────────────────────────────────────────────
    let theme_choice = Select::new("🎨  TUI Theme:", vec!["Dark  🌙", "Light ☀️"])
        .with_starting_cursor(0)
        .prompt()
        .unwrap_or("Dark  🌙");
    let theme_name = if theme_choice.starts_with("Light") {
        "light"
    } else {
        "dark"
    };

    // ── Step 5: Self-correction ────────────────────────────────────────
    let self_correct = Confirm::new("🔁  Enable self-correction (auto-fix Rust errors)?")
        .with_default(true)
        .prompt()
        .unwrap_or(true);

    // ── Step 6: Tool timeout ───────────────────────────────────────────
    let timeout = Text::new("⏱️  Tool timeout (seconds):")
        .with_default("30")
        .prompt()
        .unwrap_or_else(|_| "30".into());

    // ── Write config files ─────────────────────────────────────────────
    println!();
    println!("📝  Writing configuration...");
    println!();

    let tua_config_dir = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tua-rs");
    let tau_config_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".tau");

    fs::create_dir_all(&tua_config_dir)?;
    fs::create_dir_all(&tau_config_dir)?;

    // ~/.tua-rs/config.toml
    let tua_config_path = tua_config_dir.join("config.toml");
    if tua_config_path.exists() {
        let overwrite = Confirm::new(&format!(
            "  {} exists. Overwrite?",
            tua_config_path.display()
        ))
        .with_default(false)
        .prompt()
        .unwrap_or(false);
        if !overwrite {
            println!("  ⏭  Skipped");
        } else {
            write_tua_config(&tua_config_path, &model, self_correct, &timeout, theme_name)?;
            println!("  ✓ {}", tua_config_path.display());
        }
    } else {
        write_tua_config(&tua_config_path, &model, self_correct, &timeout, theme_name)?;
        println!("  ✓ {}", tua_config_path.display());
    }

    // ~/.tau/catalog.toml
    let catalog_path = tau_config_dir.join("catalog.toml");
    if catalog_path.exists() {
        println!("  ⏭  {} (already exists)", catalog_path.display());
    } else {
        write_catalog(
            &catalog_path,
            provider_name,
            provider_kind,
            &base_url,
            &model,
        )?;
        println!("  ✓ {}", catalog_path.display());
    }

    // ~/.tau/credentials.json
    let creds_path = tau_config_dir.join("credentials.json");
    let mut creds: HashMap<String, String> = if creds_path.exists() {
        let content = fs::read_to_string(&creds_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        HashMap::new()
    };
    creds.insert(provider_name.to_string(), api_key);
    fs::write(&creds_path, serde_json::to_string_pretty(&creds)?)?;
    println!("  ✓ {}", creds_path.display());

    // ~/.tau/providers.json
    let providers_path = tau_config_dir.join("providers.json");
    if !providers_path.exists() {
        let providers = serde_json::json!({
            "default_provider": provider_name,
            "provider_preferences": {
                provider_name: {
                    "default_model": model,
                    "headers": {},
                    "max_retries": 2,
                    "max_retry_delay_seconds": 1.0,
                    "timeout_seconds": 120.0
                }
            }
        });
        fs::write(&providers_path, serde_json::to_string_pretty(&providers)?)?;
        println!("  ✓ {}", providers_path.display());
    } else {
        println!("  ⏭  {} (already exists)", providers_path.display());
    }

    // ~/.tua-rs/theme.toml
    let theme_path = tua_config_dir.join("theme.toml");
    if !theme_path.exists() {
        write_default_theme(&theme_path, theme_name)?;
        println!("  ✓ {}", theme_path.display());
    }

    println!();
    println!("✅  Setup complete!");
    println!();
    println!("   Run: tua-rs tui");
    println!();

    // ── Launch TUI? ───────────────────────────────────────────────────
    let launch = Confirm::new("🚀  Launch TUI now?")
        .with_default(true)
        .prompt()
        .unwrap_or(true);
    Ok(launch)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_tua_config(
    path: &PathBuf,
    _model: &str,
    self_correct: bool,
    timeout: &str,
    theme: &str,
) -> anyhow::Result<()> {
    let content = format!(
        r#"# Tua Agent configuration
default_profile = "rustacean"
default_provider = "openai"
tool_timeout_secs = {timeout}
self_correction = {self_correct}
context_limit = 128000

[theme]
name = "{theme}"
"#
    );
    fs::write(path, content)?;
    Ok(())
}

fn write_catalog(
    path: &PathBuf,
    name: &str,
    kind: &str,
    base_url: &str,
    model: &str,
) -> anyhow::Result<()> {
    let content = format!(
        r#"schema_version = 1

[[providers]]
name = "{name}"
display_name = "{name} ({kind})"
kind = "{kind}"
base_url = "{base_url}"
models = ["{model}"]
default_model = "{model}"
"#
    );
    fs::write(path, content)?;
    Ok(())
}

fn write_default_theme(path: &PathBuf, theme_name: &str) -> anyhow::Result<()> {
    let (bg, fg, accent, user_msg, agent_msg, error, dim, border, input_bg, palette_bg) =
        if theme_name == "light" {
            (
                "#f5f5f5", "#1a1a2e", "#0066cc", "#006600", "#0044aa", "#cc0000", "#888888",
                "#cccccc", "#ffffff", "#e8e8f0",
            )
        } else {
            (
                "#1a1a2e", "#e0e0e0", "#00d4ff", "#00ff88", "#00d4ff", "#ff4444", "#666666",
                "#333355", "#0d0d1a", "#16213e",
            )
        };

    let content = format!(
        r#"# Tua TUI Theme
# Change `name` to switch: "dark", "light", or "custom"
# For custom themes, set name="custom" and edit colors below.

[theme]
name = "{theme_name}"

[theme.colors]
bg = "{bg}"
fg = "{fg}"
accent = "{accent}"
user_msg = "{user_msg}"
agent_msg = "{agent_msg}"
error = "{error}"
dim = "{dim}"
border = "{border}"
input_bg = "{input_bg}"
palette_bg = "{palette_bg}"
"#
    );
    fs::write(path, content)?;
    Ok(())
}
