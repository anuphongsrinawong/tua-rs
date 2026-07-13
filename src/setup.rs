//! 🛠️ Interactive setup wizard for first-time Tua configuration.
//!
//! Walks the user through configuring providers, API keys, models, and
//! preferences, then writes the necessary config files.
//!
//! All paths use [`dirs::config_dir`] so this works on Linux, macOS, and Windows.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, Write};
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

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    // ── Step 1: Provider ──────────────────────────────────────────────
    println!("📡  Choose your AI provider:");
    println!();
    println!("   1) 9Router (local proxy — recommended, free)");
    println!("   2) DeepSeek (direct API)");
    println!("   3) OpenAI");
    println!("   4) Anthropic (Claude)");
    println!("   5) Ollama (local)");
    println!("   6) Custom OpenAI-compatible endpoint");
    println!();

    let provider_num = ask_number(&mut reader, "Provider [1-6]", 1, 6, 1);

    let (provider_name, provider_kind, default_base_url, default_model, needs_key) =
        match provider_num {
            1 => ("9router", "openai-compatible", "http://127.0.0.1:20128/v1", "ds/deepseek-v4-pro", true),
            2 => ("deepseek", "openai-compatible", "https://api.deepseek.com/v1", "deepseek-chat", true),
            3 => ("openai", "openai-compatible", "https://api.openai.com/v1", "gpt-4o", true),
            4 => ("anthropic", "openai-compatible", "https://api.anthropic.com/v1", "claude-sonnet-4-20250514", true),
            5 => ("ollama", "openai-compatible", "http://localhost:11434/v1", "llama3", false),
            _ => ("custom", "openai-compatible", "", "gpt-4o", true),
        };

    let mut base_url = default_base_url.to_string();
    if provider_num == 6 {
        base_url = ask_string(&mut reader, "  Base URL", "http://localhost:8080/v1");
    }

    // ── Step 2: API Key ───────────────────────────────────────────────
    let api_key = if needs_key {
        let key = ask_string_optional(&mut reader, &format!("  API key for {provider_name}"), "free");
        if key.is_empty() { "free".to_string() } else { key }
    } else {
        "ollama".to_string()
    };

    // ── Step 3: Model ──────────────────────────────────────────────────
    let model = ask_string(&mut reader, "  Default model", default_model);

    // ── Step 4: TUI Theme ──────────────────────────────────────────────
    println!();
    println!("🎨  TUI Theme:");
    println!("   1) Dark  🌙 (default)");
    println!("   2) Light ☀️");
    let theme_num = ask_number(&mut reader, "Theme [1-2]", 1, 2, 1);
    let theme_name = if theme_num == 1 { "dark" } else { "light" };

    // ── Step 5: Self-correction ────────────────────────────────────────
    println!();
    let self_correct = ask_yes_no(&mut reader, "🔁  Enable self-correction (auto-fix Rust errors)?", true);

    // ── Step 6: Tool timeout ───────────────────────────────────────────
    let timeout = ask_string(&mut reader, "⏱️  Tool timeout (seconds)", "30");

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
        if !ask_yes_no(
            &mut reader,
            &format!("  {} exists. Overwrite?", tua_config_path.display()),
            false,
        ) {
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
        write_catalog(&catalog_path, provider_name, provider_kind, &base_url, &model)?;
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
    Ok(ask_yes_no(&mut reader, "🚀  Launch TUI now?", true))
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
                "#f5f5f5", "#1a1a2e", "#0066cc", "#006600", "#0044aa",
                "#cc0000", "#888888", "#cccccc", "#ffffff", "#e8e8f0",
            )
        } else {
            (
                "#1a1a2e", "#e0e0e0", "#00d4ff", "#00ff88", "#00d4ff",
                "#ff4444", "#666666", "#333355", "#0d0d1a", "#16213e",
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

// ── Input helpers ─────────────────────────────────────────────────────

fn ask_string(reader: &mut impl BufRead, prompt: &str, default: &str) -> String {
    print!("  {prompt} [{default}]: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    reader.read_line(&mut line).ok();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

fn ask_string_optional(reader: &mut impl BufRead, prompt: &str, default: &str) -> String {
    ask_string(reader, prompt, default)
}

fn ask_number(reader: &mut impl BufRead, prompt: &str, min: u32, max: u32, default: u32) -> u32 {
    loop {
        print!("  {prompt} [{default}]: ");
        io::stdout().flush().ok();
        let mut line = String::new();
        reader.read_line(&mut line).ok();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return default;
        }
        if let Ok(n) = trimmed.parse::<u32>() {
            if n >= min && n <= max {
                return n;
            }
        }
        println!("  ⚠️  Please enter {min}-{max}");
    }
}

fn ask_yes_no(reader: &mut impl BufRead, prompt: &str, default_yes: bool) -> bool {
    let default = if default_yes { "Y/n" } else { "y/N" };
    print!("  {prompt} [{default}]: ");
    io::stdout().flush().ok();
    let mut line = String::new();
    reader.read_line(&mut line).ok();
    let trimmed = line.trim().to_lowercase();
    if trimmed.is_empty() {
        return default_yes;
    }
    matches!(trimmed.as_str(), "y" | "yes")
}
