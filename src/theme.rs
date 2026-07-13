//! 🎨 TUI Theme system — dark/light mode with configurable colors.
//!
//! Themes are loaded from `~/.tua-rs/theme.toml` and fall back to
//! built-in dark/light presets when no config file exists.

use ratatui::style::Color;
use serde::Deserialize;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Theme struct
// ---------------------------------------------------------------------------

/// A named colour palette for the TUI.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub colors: ThemeColors,
}

/// Individual colour slots.
#[derive(Debug, Clone)]
pub struct ThemeColors {
    pub bg: Color,
    pub fg: Color,
    pub accent: Color,
    pub user_msg: Color,
    pub agent_msg: Color,
    pub error: Color,
    pub dim: Color,
    pub border: Color,
    pub input_bg: Color,
    pub palette_bg: Color,
}

// ---------------------------------------------------------------------------
// TOML deserialization shape
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ThemeFile {
    theme: ThemeSection,
}

#[derive(Debug, Deserialize)]
struct ThemeSection {
    #[serde(default)]
    name: String,
    #[serde(default)]
    colors: ColorsSection,
}

#[derive(Debug, Deserialize, Default)]
struct ColorsSection {
    #[serde(default)]
    bg: String,
    #[serde(default)]
    fg: String,
    #[serde(default)]
    accent: String,
    #[serde(default)]
    user_msg: String,
    #[serde(default)]
    agent_msg: String,
    #[serde(default)]
    error: String,
    #[serde(default)]
    dim: String,
    #[serde(default)]
    border: String,
    #[serde(default)]
    input_bg: String,
    #[serde(default)]
    palette_bg: String,
}

// ---------------------------------------------------------------------------
// Built-in themes
// ---------------------------------------------------------------------------

/// The default dark theme — deep navy background with cyan accents.
pub fn dark_theme() -> Theme {
    Theme {
        name: "dark".into(),
        colors: ThemeColors {
            bg: hex("#1a1a2e"),
            fg: hex("#e0e0e0"),
            accent: hex("#00d4ff"),
            user_msg: hex("#00ff88"),
            agent_msg: hex("#00d4ff"),
            error: hex("#ff4444"),
            dim: hex("#666666"),
            border: hex("#333355"),
            input_bg: hex("#0d0d1a"),
            palette_bg: hex("#16213e"),
        },
    }
}

/// A light theme — off-white background with dark text and blue accents.
pub fn light_theme() -> Theme {
    Theme {
        name: "light".into(),
        colors: ThemeColors {
            bg: hex("#f5f5f5"),
            fg: hex("#1a1a2e"),
            accent: hex("#0066cc"),
            user_msg: hex("#006600"),
            agent_msg: hex("#0044aa"),
            error: hex("#cc0000"),
            dim: hex("#888888"),
            border: hex("#cccccc"),
            input_bg: hex("#ffffff"),
            palette_bg: hex("#e8e8f0"),
        },
    }
}

// ---------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------

/// Load the theme from `~/.tua-rs/theme.toml`, falling back to the
/// built-in dark theme when the file is missing or unreadable.
pub fn load() -> Theme {
    let path = theme_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return dark_theme();
    };
    let Ok(file) = toml::from_str::<ThemeFile>(&content) else {
        return dark_theme();
    };
    match file.theme.name.as_str() {
        "light" => light_theme(),
        "custom" => Theme {
            name: "custom".into(),
            colors: ThemeColors {
                bg: parse_or(&file.theme.colors.bg, dark_theme().colors.bg),
                fg: parse_or(&file.theme.colors.fg, dark_theme().colors.fg),
                accent: parse_or(&file.theme.colors.accent, dark_theme().colors.accent),
                user_msg: parse_or(&file.theme.colors.user_msg, dark_theme().colors.user_msg),
                agent_msg: parse_or(&file.theme.colors.agent_msg, dark_theme().colors.agent_msg),
                error: parse_or(&file.theme.colors.error, dark_theme().colors.error),
                dim: parse_or(&file.theme.colors.dim, dark_theme().colors.dim),
                border: parse_or(&file.theme.colors.border, dark_theme().colors.border),
                input_bg: parse_or(&file.theme.colors.input_bg, dark_theme().colors.input_bg),
                palette_bg: parse_or(
                    &file.theme.colors.palette_bg,
                    dark_theme().colors.palette_bg,
                ),
            },
        },
        _ => dark_theme(),
    }
}

/// Load a specific named theme, used by `/theme dark` and `/theme light`.
pub fn load_named(name: &str) -> Theme {
    match name {
        "light" => light_theme(),
        _ => dark_theme(),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn theme_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tua-rs")
        .join("theme.toml")
}

fn hex(s: &str) -> Color {
    parse_hex(s).unwrap_or(Color::White)
}

fn parse_or(s: &str, fallback: Color) -> Color {
    if s.is_empty() {
        fallback
    } else {
        parse_hex(s).unwrap_or(fallback)
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}
