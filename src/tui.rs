//! 🖥️ Terminal User Interface — multi-tab chat with command palette.
//!
//! Uses [`ratatui`] + [`crossterm`] for a terminal UI where the user can
//! converse with the Tua agent across multiple tabs, each with its own
//! profile and conversation history.
//!
//! ## Keybindings
//!
//! | Key          | Action              |
//! |--------------|---------------------|
//! | `Enter`      | Send message        |
//! | `Ctrl+T`     | New tab             |
//! | `Ctrl+W`     | Close active tab    |
//! | `Ctrl+P`     | Toggle command palette |
//! | `Ctrl+C`     | Quit                |
//! | `Esc`        | Close palette       |
//! | `PgUp`/`PgDn`| Scroll chat         |
//!
//! ## Command Palette Commands
//!
//! `/help`, `/profile`, `/model`, `/tools`, `/skills`, `/config`,
//! `/diff`, `/permissions`, `/sessions`, `/rollback`, `/undo`, `/clear`

use crate::agent::{AgentEvent, AgentLoop, AgentMessage, ModelProvider};
use crate::profiles::{self, RustProfile};
use crate::prompts::rust_system_prompt::RUST_SYSTEM_PROMPT;
use crate::providers::mock::MockProvider;
use crate::providers::openai_compatible::OpenAiCompatibleProvider;
use crate::providers::ProviderConfig;
use crate::tools::rust_tools;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Stdout, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Provider config (~/.tau/*.toml + *.json)
// ---------------------------------------------------------------------------

/// A provider loaded from `~/.tau/catalog.toml`.
///
/// Each provider has a `kind` (e.g. `"openai-compatible"`), a `base_url`,
/// the list of `models` it serves, and a `default_model`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInfo {
    /// Provider name, e.g. `"9router"` or `"openai-codex"`.
    pub name: String,
    /// Provider kind, e.g. `"openai-compatible"`.
    pub kind: String,
    /// Base URL for the API (e.g. `"http://127.0.0.1:20128/v1"`).
    pub base_url: String,
    /// Models served by this provider.
    pub models: Vec<String>,
    /// The provider's default model.
    pub default_model: String,
}

/// Deserialization shape for a single `[[providers]]` entry in `catalog.toml`.
///
/// Unknown fields (`display_name`, `api_key_env`, `context_windows`, …) are
/// ignored by serde, so the richer real-world catalog still parses.
#[derive(Debug, Deserialize)]
struct CatalogProvider {
    #[serde(default)]
    name: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    default_model: String,
}

/// Deserialization shape for the top-level `catalog.toml` document.
#[derive(Debug, Deserialize, Default)]
struct Catalog {
    #[serde(default)]
    providers: Vec<CatalogProvider>,
}

/// Deserialization shape for `~/.tau/providers.json`. We only need the
/// default provider name; the `provider_preferences` map is ignored.
#[derive(Debug, Deserialize, Default)]
struct ProvidersFile {
    #[serde(default)]
    default_provider: String,
}

/// Return the `~/.tau` config directory.
///
/// Resolves the home directory from `$HOME` (falling back to `$USERPROFILE`
/// on Windows, then `.`), matching the convention used by [`config`].
fn tau_config_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".tau")
}

/// Parse providers from `catalog.toml` content.
///
/// Returns an empty vec when the document is empty or malformed (the caller
/// falls back to the mock provider in that case).
fn parse_catalog(content: &str) -> Vec<ProviderInfo> {
    let Ok(catalog) = toml::from_str::<Catalog>(content) else {
        return Vec::new();
    };
    catalog
        .providers
        .into_iter()
        .map(|p| ProviderInfo {
            name: p.name,
            kind: p.kind,
            base_url: p.base_url,
            models: p.models,
            default_model: p.default_model,
        })
        .collect()
}

/// Parse the default provider name from `providers.json` content.
fn parse_default_provider(content: &str) -> Option<String> {
    let file: ProvidersFile = serde_json::from_str(content).ok()?;
    let name = file.default_provider;
    (!name.is_empty()).then_some(name)
}

/// Parse `credentials.json` into a `{ provider_name: api_key }` map.
fn parse_credentials(content: &str) -> HashMap<String, String> {
    serde_json::from_str(content).unwrap_or_default()
}

/// Load all providers from `~/.tau/catalog.toml`.
///
/// Returns an empty vec when the file is missing or unreadable — callers
/// should treat that as "no providers configured" and keep the mock.
pub fn load_provider_config() -> Vec<ProviderInfo> {
    let path = tau_config_dir().join("catalog.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_catalog(&content)
}

/// Load the default provider name from `~/.tau/providers.json`.
pub fn load_default_provider_name() -> Option<String> {
    let path = tau_config_dir().join("providers.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return None;
    };
    parse_default_provider(&content)
}

/// Load the API key for `provider` from `~/.tau/credentials.json`.
pub fn load_api_key(provider: &str) -> Option<String> {
    let path = tau_config_dir().join("credentials.json");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return None;
    };
    let creds = parse_credentials(&content);
    creds.get(provider).cloned().filter(|k| !k.is_empty())
}

/// Resolve the default `(provider, model)` pair from the catalog, honouring
/// the `default_provider` from `providers.json` when present.
///
/// Returns `("", "")` when there are no providers.
fn default_provider_model(providers: &[ProviderInfo]) -> (String, String) {
    let Some(first) = providers.first() else {
        return (String::new(), String::new());
    };
    let preferred = load_default_provider_name();
    let chosen = providers
        .iter()
        .find(|p| Some(&p.name) == preferred.as_ref())
        .unwrap_or(first);
    let model = if chosen.default_model.is_empty() {
        chosen.models.first().cloned().unwrap_or_default()
    } else {
        chosen.default_model.clone()
    };
    (chosen.name.clone(), model)
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// State for the provider/model picker overlay.
#[derive(Debug, Clone, Default)]
pub struct Picker {
    /// Selected index in the current list (providers or models).
    pub selected: usize,
    /// The provider chosen in the `ProviderPicker` step, carried into the
    /// `ModelPicker` step. `None` while picking a provider.
    pub provider: Option<ProviderInfo>,
}

impl Picker {
    /// Move the selection down, wrapping around a list of `len` items.
    pub fn select_next(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = (self.selected + 1) % len;
    }

    /// Move the selection up, wrapping around a list of `len` items.
    pub fn select_prev(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        self.selected = if self.selected == 0 {
            len - 1
        } else {
            self.selected - 1
        };
    }
}

/// A single conversation tab.
#[derive(Debug, Clone)]
pub struct Tab {
    /// Display name (e.g. "Chat 1", "Chat 2").
    pub name: String,
    /// The Rust coding profile active in this tab.
    pub profile: &'static RustProfile,
    /// Conversation history (user + assistant messages).
    pub messages: Vec<AgentMessage>,
    /// File edits tracked in this session.
    pub edits: Vec<FileEdit>,
    /// Vertical scroll offset for the messages area.
    pub scroll_offset: usize,
    /// Selected provider name for this tab (empty ⇒ mock provider).
    pub provider: String,
    /// Selected model for this tab.
    pub model: String,
    /// API key for this tab's provider (empty ⇒ no key / mock).
    pub api_key: String,
}

impl Tab {
    /// Create a new tab with the given name and profile.
    pub fn new(name: impl Into<String>, profile: &'static RustProfile) -> Self {
        Self {
            name: name.into(),
            profile,
            messages: Vec::new(),
            edits: Vec::new(),
            scroll_offset: 0,
            provider: String::new(),
            model: String::new(),
            api_key: String::new(),
        }
    }

    /// Create a new tab with an explicit provider/model/api-key.
    pub fn with_provider(
        name: impl Into<String>,
        profile: &'static RustProfile,
        provider: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            profile,
            messages: Vec::new(),
            edits: Vec::new(),
            scroll_offset: 0,
            provider: provider.into(),
            model: model.into(),
            api_key: api_key.into(),
        }
    }
}

/// A recorded file edit, capturing the before/after content for diff display.
#[derive(Debug, Clone)]
pub struct FileEdit {
    /// Path to the file that was edited.
    pub path: String,
    /// The original content before the edit.
    pub before: String,
    /// The new content after the edit.
    pub after: String,
    /// Timestamp when the edit was recorded.
    pub timestamp: String,
}

impl FileEdit {
    /// Create a new `FileEdit`.
    pub fn new(path: String, before: String, after: String) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        Self { path, before, after, timestamp }
    }

    /// Generate a simple diff string showing what changed.
    pub fn diff(&self) -> String {
        simple_diff(&self.before, &self.after)
    }
}

/// Mode the TUI is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal mode — typing input, navigating tabs.
    Normal,
    /// Command palette is open.
    CommandPalette,
    /// Provider picker popup is open (selecting a provider).
    ProviderPicker,
    /// Model picker popup is open (selecting a model for a chosen provider).
    ModelPicker,
}

/// Application state for the TUI.
#[derive(Debug)]
pub struct App {
    /// All open tabs.
    pub tabs: Vec<Tab>,
    /// Recorded file edits, indexed by tab index.
    pub edits: Vec<Vec<FileEdit>>,
    /// Index of the active (selected) tab.
    pub active_tab: usize,
    /// Current input buffer (what the user is typing).
    pub input_buffer: String,
    /// Global event log (displayed in status area).
    pub messages: Vec<String>,
    /// Current TUI mode.
    pub mode: AppMode,
    /// Command palette state.
    pub palette: CommandPalette,
    /// Available providers loaded from `~/.tau/catalog.toml`.
    pub providers: Vec<ProviderInfo>,
    /// Provider/model picker overlay state.
    pub picker: Picker,
    /// Whether the application should exit.
    pub should_quit: bool,
    /// Profile emoji display for the status bar.
    pub profile_emoji: String,
    /// Number of active tools.
    pub tools_count: usize,
    /// Estimated token count for the active tab's conversation.
    pub token_count: usize,
}

impl App {
    /// Create a new `App` with a single default tab.
    ///
    /// The default profile is `"rustacean"`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tua_rs::tui::App;
    ///
    /// let app = App::new();
    /// assert_eq!(app.tabs.len(), 1);
    /// assert_eq!(app.active_tab, 0);
    /// assert!(app.input_buffer.is_empty());
    /// assert!(app.messages.is_empty());
    /// ```
    pub fn new() -> Self {
        let profile = profiles::get_profile("rustacean").unwrap_or(&profiles::ALL_PROFILES[2]);

        // Load providers from ~/.tau. When none are found the tab keeps empty
        // provider/model fields and the agent loop falls back to MockProvider.
        let providers = load_provider_config();
        let (provider, model) = default_provider_model(&providers);
        let api_key = load_api_key(&provider).unwrap_or_default();
        let tab = Tab::with_provider("Chat 1", profile, provider, model, api_key);

        Self {
            tabs: vec![tab],
            edits: vec![Vec::new()],
            active_tab: 0,
            input_buffer: String::new(),
            messages: Vec::new(),
            mode: AppMode::Normal,
            palette: CommandPalette::new(),
            providers,
            picker: Picker::default(),
            should_quit: false,
            profile_emoji: profile.emoji.to_string(),
            tools_count: 14,
            token_count: 0,
        }
    }

    /// Create a new `App` from a previously saved session.
    ///
    /// The session's messages are loaded into the first tab, and the
    /// session metadata (profile, model) is used to configure the tab.
    ///
    /// # Examples
    ///
    /// ```
    /// use tua_rs::session::Session;
    /// use tua_rs::tui::App;
    ///
    /// let session = Session::new("rustacean", "deepseek/deepseek-v4-flash");
    /// let app = App::from_session(session, "rustacean");
    /// assert_eq!(app.tabs.len(), 1);
    /// assert_eq!(app.tabs[0].messages.len(), 0);
    /// ```
    pub fn from_session(session: crate::session::Session, profile_name: &str) -> Self {
        let profile = profiles::get_profile(profile_name)
            .or_else(|| profiles::get_profile(&session.meta.profile))
            .unwrap_or(&profiles::ALL_PROFILES[2]);

        let mut tab = Tab::new(
            format!("Session {}", &session.meta.id.to_string()[..8]),
            profile,
        );
        tab.messages = session.messages;

        // Apply the configured default provider/model to the resumed tab.
        let providers = load_provider_config();
        let (provider, model) = default_provider_model(&providers);
        let api_key = load_api_key(&provider).unwrap_or_default();
        tab.provider = provider;
        tab.model = model;
        tab.api_key = api_key;

        let mut app = Self {
            tabs: vec![tab],
            edits: vec![Vec::new()],
            active_tab: 0,
            input_buffer: String::new(),
            messages: vec![format!(
                "🔄 Resumed session {} with profile '{}'",
                session.meta.id, profile.name
            )],
            mode: AppMode::Normal,
            palette: CommandPalette::new(),
            providers,
            picker: Picker::default(),
            should_quit: false,
            profile_emoji: profile.emoji.to_string(),
            tools_count: 14,
            token_count: 0,
        };
        app.update_token_count();
        app
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
    }

    /// Switch to the previous tab (wraps around).
    pub fn prev_tab(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        self.active_tab = if self.active_tab == 0 {
            self.tabs.len() - 1
        } else {
            self.active_tab - 1
        };
    }

    /// Get a mutable reference to the active tab.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active_tab)
    }

    /// Get a reference to the active tab.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active_tab)
    }

    /// Add a new tab and switch to it.
    pub fn add_tab(&mut self) {
        let count = self.tabs.len() + 1;
        let profile = self
            .active_tab()
            .map(|t| t.profile)
            .unwrap_or(&profiles::ALL_PROFILES[2]);
        // Inherit the active tab's provider/model so the new tab keeps the
        // same backend (or mock) as the one it was opened from.
        let (provider, model, api_key) = self
            .active_tab()
            .map(|t| (t.provider.clone(), t.model.clone(), t.api_key.clone()))
            .unwrap_or_default();
        let tab = Tab::with_provider(format!("Chat {count}"), profile, provider, model, api_key);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.messages
            .push(format!("{} Created new tab 'Chat {count}'", profile.emoji));
    }

    /// Close the active tab. If it's the last tab, a new default tab is
    /// created instead.
    pub fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            // Replace with a fresh tab instead of closing the last one,
            // preserving the current provider/model configuration.
            let profile = self
                .active_tab()
                .map(|t| t.profile)
                .unwrap_or(&profiles::ALL_PROFILES[2]);
            let (provider, model, api_key) = self
                .active_tab()
                .map(|t| (t.provider.clone(), t.model.clone(), t.api_key.clone()))
                .unwrap_or_default();
            self.tabs[0] = Tab::with_provider("Chat 1", profile, provider, model, api_key);
            self.messages
                .push("🔄 Replaced last tab with a fresh one".into());
            return;
        }

        let removed = self.tabs.remove(self.active_tab);
        self.messages
            .push(format!("🗑️ Closed tab '{}'", removed.name));

        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
    }

    /// Send the current input buffer as a message in the active tab.
    ///
    /// Returns `true` if a message was sent, `false` if the buffer was empty.
    pub fn send_message(&mut self) -> bool {
        let text = std::mem::take(&mut self.input_buffer);
        if text.trim().is_empty() {
            self.input_buffer = text; // put it back
            return false;
        }

        let msg = AgentMessage::user(text.clone());
        if let Some(tab) = self.active_tab_mut() {
            tab.messages.push(msg);
            // Auto-scroll to bottom on new message.
            tab.scroll_offset = 0;
        }

        self.messages
            .push(format!("💬 Sent: {}...", truncate(&text, 40)));
        self.update_token_count();
        true
    }

    /// Scroll the active tab's messages up.
    pub fn scroll_up(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.scroll_offset = tab.scroll_offset.saturating_add(1);
        }
    }

    /// Scroll the active tab's messages down.
    pub fn scroll_down(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.scroll_offset = tab.scroll_offset.saturating_sub(1);
        }
    }

    /// Scroll to the bottom of the active tab's messages.
    pub fn scroll_to_bottom(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            tab.scroll_offset = 0;
        }
    }

    /// Estimate token count for the active tab's conversation
    /// (rough estimate: 1 token ≈ 4 characters).
    fn update_token_count(&mut self) {
        if let Some(tab) = self.active_tab() {
            let total_chars: usize =
                tab.messages
                    .iter()
                    .map(|m| match m {
                        AgentMessage::User { text }
                        | AgentMessage::ToolResult { output: text, .. } => text.len(),
                        AgentMessage::Assistant {
                            text: Some(text), ..
                        } => text.len(),
                        AgentMessage::Assistant { text: None, .. } => 0,
                    })
                    .sum();
            self.token_count = total_chars / 4;
        }
    }

    /// Record a file edit in the active tab.
    ///
    /// Captures the file path, before/after content, and the current
    /// local timestamp. The edit is pushed onto the active tab's edit
    /// history so it can be inspected with `/diff`.
    pub fn record_edit(&mut self, path: &str, before: &str, after: &str) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());

        let edit = FileEdit {
            path: path.to_string(),
            before: before.to_string(),
            after: after.to_string(),
            timestamp,
        };

        if let Some(tab) = self.active_tab_mut() {
            tab.edits.push(edit);
            let count = tab.edits.len();
            self.messages
                .push(format!("📝 Recorded edit #{count} for {path}"));
        }
    }

    /// Produce a simple line-based unified diff between two text strings.
    ///
    /// The output uses `-` and `+` prefixes for removed/added lines, and
    /// includes surrounding context lines for readability.
    fn simple_diff(&self, before: &str, after: &str) -> String {
        simple_diff(before, after)
    }

    /// Show a formatted diff from the active tab's edit history.
    ///
    /// Supported arguments:
    /// - `""` (empty) — summary of all edits
    /// - `"last"` — full unified diff of the most recent edit
    /// - `"<n>"` — full unified diff of edit #n (1-indexed)
    ///
    /// Returns a string suitable for display in the message panel.
    pub fn show_diff(&mut self, arg: &str) -> String {
        let Some(tab) = self.active_tab() else {
            return "⚠️  No active tab".into();
        };

        if tab.edits.is_empty() {
            return "📝 No edits recorded in this tab yet".into();
        }

        if arg.is_empty() {
            let mut lines: Vec<String> = Vec::with_capacity(tab.edits.len() + 2);
            lines.push("📂 Recorded edits:".into());
            lines.push("━━━━━━━━━━━━━━━━━━━━".into());
            for (i, edit) in tab.edits.iter().enumerate() {
                let added = edit
                    .after
                    .lines()
                    .count()
                    .saturating_sub(edit.before.lines().count());
                let removed = edit
                    .before
                    .lines()
                    .count()
                    .saturating_sub(edit.after.lines().count());
                let sign = if added > 0 { "+" } else { "" };
                lines.push(format!(
                    "  #{i}: {}  ({} lines, {}{} / -{})",
                    edit.path, edit.timestamp, sign, added, removed
                ));
            }
            return lines.join("\n");
        }

        let idx: Option<usize> = if arg == "last" {
            Some(tab.edits.len().saturating_sub(1))
        } else {
            arg.parse::<usize>().ok().and_then(|n| {
                if n == 0 { None } else { Some(n - 1) }
            })
        };

        let Some(idx) = idx else {
            return format!("⚠️  Invalid edit index: {arg}, expected <n> or 'last'");
        };

        let Some(edit) = tab.edits.get(idx) else {
            return format!(
                "⚠️  Edit #{idx} not found (have {} edits)",
                tab.edits.len()
            );
        };

        format!(
            "📝 Edit #{} — {}\n\n{}",
            idx + 1,
            edit.path,
            self.simple_diff(&edit.before, &edit.after),
        )
    }

    /// Execute a slash command from the palette.
    ///
    /// Returns a message describing the result.
    pub fn execute_command(&mut self, command: &str) -> String {
        match command {
            "/help" => {
                "📖 Commands: /help /profile /model /tools /skills /config /diff \
                 /permissions /sessions /rollback /undo /clear"
                    .into()
            }
            "/profile" => {
                if let Some(tab) = self.active_tab() {
                    format!(
                        "{} Active profile: {} — {}",
                        tab.profile.emoji, tab.profile.name, tab.profile.description
                    )
                } else {
                    "⚠️  No active tab".into()
                }
            }
            "/clear" => {
                if let Some(tab) = self.active_tab_mut() {
                    tab.messages.clear();
                    tab.scroll_offset = 0;
                }
                "🧹 Chat cleared".into()
            }
            "/model" => {
                if self.providers.is_empty() {
                    "⚠️  No providers found in ~/.tau/catalog.toml — using mock".into()
                } else {
                    self.picker = Picker::default();
                    self.mode = AppMode::ProviderPicker;
                    "🤖 Select a provider (↑↓ to navigate, Enter to select, Esc to cancel)".into()
                }
            }
            "/tools" => "🔧 14 Rust tools registered: cargo, rustc, rustfmt, clippy, rustup, \
                         audit, outdated, udeps, deny, bench, doc, test-doc, wasm-pack, rustc-explain"
                .into(),
            "/skills" => "📚 Available skills: async-rust, cargo-workspace, concurrency, \
                          error-handling, lifetimes, macros, ownership-borrowing, smart-pointers, \
                          testing, wasm"
                .into(),
            "/config" => "⚙️  Run `tua-rs config` in terminal for full configuration".into(),
            cmd if cmd.starts_with("/diff ") => self.show_diff(cmd.strip_prefix("/diff ").unwrap().trim()),
            "/diff" => self.show_diff(""),
            "/permissions" => "🔐 Permission mode: ask (default) — use --permission flag to change"
                .into(),
            "/sessions" => {
                format!(
                    "📋 Sessions: {} active tab(s), {} total messages",
                    self.tabs.len(),
                    self.tabs.iter().map(|t| t.messages.len()).sum::<usize>()
                )
            }
            "/resume" => {
                match crate::session::default_sessions_dir() {
                    Ok(dir) => {
                        match crate::session::list_sessions(&dir) {
                            Ok(summaries) if summaries.is_empty() => {
                                "📂 No saved sessions found in `~/.tua-rs/sessions/`".into()
                            }
                            Ok(summaries) => {
                                let mut lines: Vec<String> = Vec::with_capacity(summaries.len() + 1);
                                lines.push("📂 Saved sessions:".into());
                                for s in &summaries {
                                    lines.push(format!(
                                        "  {}  {}  {}  {} messages",
                                        &s.id.to_string()[..8],
                                        s.profile,
                                        s.created_at,
                                        s.message_count
                                    ));
                                }
                                lines.join("\n")
                            }
                            Err(e) => format!("⚠️  Could not list sessions: {e}"),
                        }
                    }
                    Err(e) => format!("⚠️  Could not access sessions directory: {e}"),
                }
            }
            "/rollback" => "⏪ Rollback: use the `checkpoint` tools to restore previous states".into(),
            "/undo" => "↩️ Undo: use the `edit` tools with rollback support".into(),
            _ => format!("❓ Unknown command: {command}"),
        }
    }

    // ── Live TUI run loop ────────────────────────────────────────────

    /// Signature identifying the active tab's provider configuration.
    ///
    /// Used to detect when the agent loop needs rebuilding (e.g. the user
    /// switched tabs or selected a new provider/model).
    fn active_signature(&self) -> String {
        self.active_tab()
            .map(|t| format!("{}|{}|{}", t.provider, t.model, t.api_key))
            .unwrap_or_default()
    }

    /// Build a fresh [`AgentLoop`] for the active tab's configured provider.
    ///
    /// Falls back to a [`MockProvider`] when no provider is configured, when
    /// the provider has no API key, or when the provider `kind` is not wired.
    fn build_agent_loop(&self) -> AgentLoop {
        let provider: Arc<dyn ModelProvider> = match self.active_tab() {
            Some(tab) => self.build_provider_for(tab),
            None => Arc::new(mock_greeting()),
        };
        AgentLoop::new(provider, RUST_SYSTEM_PROMPT.to_string(), rust_tools())
    }

    /// Build the concrete provider for a tab, with graceful mock fallbacks.
    fn build_provider_for(&self, tab: &Tab) -> Arc<dyn ModelProvider> {
        // Unknown provider name (or empty) ⇒ mock.
        let Some(info) = self.providers.iter().find(|p| p.name == tab.provider) else {
            return Arc::new(mock_greeting());
        };
        // No API key ⇒ mock with an actionable warning (never crash).
        if tab.api_key.is_empty() {
            return Arc::new(MockProvider::with_text(&format!(
                "⚠️ No API key for {} — add it to ~/.tau/credentials.json",
                tab.provider
            )));
        }
        match info.kind.as_str() {
            "openai-compatible" | "openai" => {
                let cfg = ProviderConfig::new(
                    "openai",
                    tab.api_key.clone(),
                    Some(info.base_url.clone()),
                    tab.model.clone(),
                );
                Arc::new(OpenAiCompatibleProvider::new(cfg))
            }
            other => Arc::new(MockProvider::with_text(&format!(
                "Provider kind `{other}` is not wired yet — using mock."
            ))),
        }
    }

    /// Run the TUI: enter raw mode, render frames, and pump both terminal
    /// key events and streaming agent events until the user quits
    /// (`Ctrl+C` or `Ctrl+Q`).
    ///
    /// This must be invoked from within a running tokio runtime (for
    /// example via `Runtime::block_on`), because the agent is driven by a
    /// background `tokio::spawn` task. The agent uses a [`MockProvider`]
    /// so it works fully offline with no API key.
    ///
    /// When the user presses `Enter` in normal mode, the current input is
    /// sent as a user message and a background task runs the agent loop,
    /// forwarding every [`AgentEvent`] into a channel that this render
    /// loop drains on each frame.
    pub fn run(&mut self) -> anyhow::Result<()> {
        // Build the agent loop for the active tab's provider/model. This is a
        // real [`OpenAiCompatibleProvider`] when `~/.tau/catalog.toml` +
        // `credentials.json` configure one, otherwise a [`MockProvider`].
        let mut agent_loop = self.build_agent_loop();
        let mut agent_signature = self.active_signature();

        // Bridge: agent background task → this synchronous render loop.
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<AgentEvent>();

        let (_guard, mut terminal) = TerminalGuard::enter()
            .map_err(|e| anyhow::anyhow!("failed to initialize terminal: {e}"))?;

        while !self.should_quit {
            // 1. Drain any agent events that arrived since the last frame.
            self.pump_agent_events(&mut event_rx);

            // 2. Render the current state.
            terminal.draw(|frame| render(frame, self))?;

            // 3. Poll for a terminal key event (non-blocking, 50 ms timeout
            //    so the loop stays responsive to streaming agent events).
            if crossterm::event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    // Rebuild the agent loop if the active tab's provider/model
                    // changed (switched tabs, or selected a new provider/model).
                    let signature = self.active_signature();
                    if signature != agent_signature {
                        agent_loop = self.build_agent_loop();
                        agent_signature = signature;
                    }
                    self.handle_tui_key(key, &agent_loop, &event_tx);
                }
            }
        }

        Ok(())
    }

    /// Dispatch a single key event, spawning the agent on `Enter`.
    fn handle_tui_key(
        &mut self,
        key: KeyEvent,
        agent_loop: &AgentLoop,
        event_tx: &tokio::sync::mpsc::UnboundedSender<AgentEvent>,
    ) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        // Enter in normal mode → send the message and spawn the agent.
        if self.mode == AppMode::Normal && key.code == KeyCode::Enter {
            if self.send_message() {
                let messages = self
                    .active_tab()
                    .map(|tab| tab.messages.clone())
                    .unwrap_or_default();
                let loop_clone = agent_loop.clone();
                let tx = event_tx.clone();
                tokio::spawn(async move {
                    let mut stream = loop_clone.run(messages);
                    while let Some(event) = stream.next().await {
                        if tx.send(event).is_err() {
                            // TUI exited and dropped the receiver — stop.
                            break;
                        }
                    }
                });
            }
            return;
        }

        // All other keys reuse the existing handler (tabs, palette, quit…).
        handle_key_event(self, key);
    }

    /// Drain every available agent event into the active tab.
    fn pump_agent_events(&mut self, rx: &mut tokio::sync::mpsc::UnboundedReceiver<AgentEvent>) {
        while let Ok(event) = rx.try_recv() {
            self.apply_agent_event(event);
        }
    }

    /// Apply a single streamed [`AgentEvent`] to the active tab's history.
    fn apply_agent_event(&mut self, event: AgentEvent) {
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        match event {
            AgentEvent::TextDelta(text) => append_assistant(tab, text),
            AgentEvent::ThinkingDelta(text) => append_assistant(tab, format!("\n💭 {text}")),
            AgentEvent::ToolCall(call) => {
                append_assistant(tab, format!("\n🔧 {}({})", call.name, call.arguments));
            }
            AgentEvent::ToolResult { output, .. } => {
                let marker = if output.to_lowercase().contains("error") {
                    "⚠️"
                } else {
                    "✅"
                };
                append_assistant(tab, format!("\n{marker} {}", truncate(&output, 120)));
            }
            AgentEvent::Error(msg) => append_assistant(tab, format!("\n❌ {msg}")),
            AgentEvent::Done => {}
        }
        tab.scroll_offset = 0;
        self.update_token_count();
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Command Palette
// ---------------------------------------------------------------------------

/// All available slash commands.
pub const SLASH_COMMANDS: &[&str] = &[
    "/help",
    "/profile",
    "/model",
    "/tools",
    "/skills",
    "/config",
    "/diff",
    "/permissions",
    "/sessions",
    "/resume",
    "/rollback",
    "/undo",
    "/clear",
];

/// State for the command palette overlay.
#[derive(Debug, Clone)]
pub struct CommandPalette {
    /// Whether the palette is currently visible.
    pub visible: bool,
    /// Current filter text the user has typed.
    pub filter: String,
    /// Index of the selected item in the filtered list.
    pub selected: usize,
}

impl CommandPalette {
    fn new() -> Self {
        Self {
            visible: false,
            filter: String::new(),
            selected: 0,
        }
    }

    /// Get the list of commands matching the current filter.
    pub fn filtered_commands(&self) -> Vec<&'static str> {
        if self.filter.is_empty() {
            SLASH_COMMANDS.to_vec()
        } else {
            let lower = self.filter.to_lowercase();
            SLASH_COMMANDS
                .iter()
                .copied()
                .filter(|cmd| cmd.contains(&lower))
                .collect()
        }
    }

    /// Select the next command in the filtered list.
    pub fn select_next(&mut self) {
        let filtered = self.filtered_commands();
        if filtered.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % filtered.len();
    }

    /// Select the previous command in the filtered list.
    pub fn select_prev(&mut self) {
        let filtered = self.filtered_commands();
        if filtered.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            filtered.len() - 1
        } else {
            self.selected - 1
        };
    }

    /// Get the currently selected command, if any.
    pub fn selected_command(&self) -> Option<&'static str> {
        let filtered = self.filtered_commands();
        filtered.get(self.selected).copied()
    }

    /// Push a character to the filter string.
    pub fn push_filter(&mut self, c: char) {
        self.filter.push(c);
        self.selected = 0;
    }

    /// Pop the last character from the filter string.
    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }
}

// ---------------------------------------------------------------------------
// Terminal helpers
// ---------------------------------------------------------------------------

use ratatui::backend::CrosstermBackend;

/// A guard that restores terminal settings when dropped.
pub struct TerminalGuard;

/// Return type for [`TerminalGuard::enter`].
type TuiInitResult =
    Result<(TerminalGuard, Terminal<CrosstermBackend<Stdout>>), Box<dyn std::error::Error>>;

impl TerminalGuard {
    /// Enter raw mode and set up the terminal for the TUI.
    pub fn enter() -> TuiInitResult {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
        )?;

        let backend = ratatui::backend::CrosstermBackend::new(std::io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok((TerminalGuard, terminal))
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::event::DisableMouseCapture,
        );
        let _ = std::io::stdout().flush();
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Run the TUI event loop. This function enters raw mode, renders the UI,
/// and processes key events until the user quits.
///
/// Returns an error if terminal initialisation fails.
pub fn run_tui(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let (_guard, mut terminal) = TerminalGuard::enter()?;

    while !app.should_quit {
        terminal.draw(|frame| render(frame, app))?;

        // Poll for events with a small timeout so we can redraw periodically.
        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                handle_key_event(app, key);
            }
        }
    }

    Ok(())
}

/// Render the full TUI layout.
fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    let constraints = if app.palette.visible {
        vec![
            Constraint::Length(1), // tab bar
            Constraint::Min(0),    // palette area (floating)
            Constraint::Min(1),    // chat area
            Constraint::Length(1), // input line
            Constraint::Length(1), // status bar
        ]
    } else {
        vec![
            Constraint::Length(1), // tab bar
            Constraint::Min(1),    // chat area
            Constraint::Length(1), // input line
            Constraint::Length(1), // status bar
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    render_tab_bar(frame, app, chunks[0]);
    if app.palette.visible {
        render_command_palette(frame, app, chunks[1]);
        render_chat_area(frame, app, chunks[2]);
    } else {
        render_chat_area(frame, app, chunks[1]);
    }
    render_input_line(
        frame,
        app,
        if app.palette.visible {
            chunks[3]
        } else {
            chunks[2]
        },
    );
    render_status_bar(
        frame,
        app,
        if app.palette.visible {
            chunks[4]
        } else {
            chunks[3]
        },
    );

    // Floating overlays drawn last so they sit on top of the base layout.
    match app.mode {
        AppMode::ProviderPicker => render_provider_picker(frame, app),
        AppMode::ModelPicker => render_model_picker(frame, app),
        _ => {}
    }
}

// ── Provider / Model Picker overlays ──────────────────────────────────

/// Render a popup list of available providers.
fn render_provider_picker(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 60, frame.area());
    let items: Vec<ListItem> = app
        .providers
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let prefix = if i == app.picker.selected {
                "▶ "
            } else {
                "  "
            };
            ListItem::new(format!("{}{}  [{}]", prefix, p.name, p.default_model))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Select Provider ")
                .borders(Borders::ALL),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(Clear, area);
    frame.render_widget(list, area);
}

/// Render a popup list of models for the currently selected provider.
fn render_model_picker(frame: &mut Frame, app: &App) {
    let area = centered_rect(50, 60, frame.area());
    let provider = match &app.picker.provider {
        Some(p) => p,
        None => return,
    };

    let items: Vec<ListItem> = provider
        .models
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let prefix = if i == app.picker.selected {
                "▶ "
            } else {
                "  "
            };
            let is_default = m == &provider.default_model;
            let tag = if is_default { " (default)" } else { "" };
            ListItem::new(format!("{}{}{}", prefix, m, tag))
        })
        .collect();

    let title = format!(" Select Model — {} ", provider.name);
    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(Clear, area);
    frame.render_widget(list, area);
}

/// Helper: return a rectangle centered in `r` with given width/height percentages.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Render the top tab bar with profile emojis.
fn render_tab_bar(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (i, tab) in app.tabs.iter().enumerate() {
        let is_active = i == app.active_tab;
        let style = if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        };

        let prefix = if is_active { " ▶ " } else { "   " };
        let text = format!("{}{} {} ", prefix, tab.profile.emoji, tab.name);

        spans.push(Span::styled(text, style));

        // Separator
        if i < app.tabs.len() - 1 {
            spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        }
    }

    // Fill the remaining space
    spans.push(Span::styled(
        " ".repeat(
            area.width
                .saturating_sub(spans.iter().map(|s| s.content.len()).sum::<usize>() as u16)
                as usize,
        ),
        Style::default().bg(Color::Reset),
    ));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

/// Render the main chat area with scrollable messages.
fn render_chat_area(frame: &mut Frame, app: &App, area: Rect) {
    let Some(tab) = app.tabs.get(app.active_tab) else {
        return;
    };

    // Build display lines from the active tab's messages.
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(tab.messages.len() * 2);

    for msg in &tab.messages {
        match msg {
            AgentMessage::User { text } => {
                let prefix = Span::styled(
                    "👤 ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                );
                let content = Span::styled(text.clone(), Style::default().fg(Color::White));
                lines.push(Line::from(vec![prefix, content]));
            }
            AgentMessage::Assistant {
                text: Some(text), ..
            } => {
                let prefix = Span::styled(
                    "🤖 ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
                let content = Span::styled(text.clone(), Style::default().fg(Color::Cyan));
                lines.push(Line::from(vec![prefix, content]));
            }
            AgentMessage::Assistant { text: None, .. } => {
                let prefix = Span::styled(
                    "🤖 ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                );
                let content = Span::styled(
                    "[thinking...]",
                    Style::default().fg(Color::DarkGray).italic(),
                );
                lines.push(Line::from(vec![prefix, content]));
            }
            AgentMessage::ToolResult { output, .. } => {
                let prefix = Span::styled(
                    "🔧 ",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                );
                let truncated = truncate(output, 120);
                let content = Span::styled(truncated, Style::default().fg(Color::Yellow).italic());
                lines.push(Line::from(vec![prefix, content]));
            }
        }
    }

    // Apply scroll offset
    let available_height = area.height.saturating_sub(1) as usize;
    let total_lines = lines.len();
    let scroll = tab
        .scroll_offset
        .min(total_lines.saturating_sub(available_height));
    let visible_lines: Vec<ListItem> = lines
        .iter()
        .skip(scroll)
        .take(available_height)
        .map(|line| ListItem::new(line.clone()))
        .collect();

    let list = List::new(visible_lines)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .title(format!(" 💬 {} ", tab.name))
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    frame.render_widget(list, area);
}

/// Render the command palette overlay.
fn render_command_palette(frame: &mut Frame, app: &App, area: Rect) {
    let filtered = app.palette.filtered_commands();

    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == app.palette.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(Span::styled(*cmd, style)))
        })
        .collect();

    let height = items.len().min(6) as u16 + 2; // max 6 items + borders
    let palette_area = Rect {
        x: area.x + 2,
        y: area.y,
        width: 30.min(area.width.saturating_sub(4)),
        height: height.min(area.height),
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" 🔍 {} ", app.palette.filter))
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_widget(list, palette_area);
}

/// Render the bottom input line.
fn render_input_line(frame: &mut Frame, app: &App, area: Rect) {
    let prefix = if app.palette.visible {
        "🔍 "
    } else {
        "💬 "
    };

    let input_text = if app.palette.visible {
        &app.palette.filter
    } else {
        &app.input_buffer
    };

    let display_text = format!("{prefix}{input_text}");
    let cursor_vis = if app.palette.visible {
        app.palette.filter.len()
    } else {
        app.input_buffer.len()
    };

    let paragraph = Paragraph::new(display_text)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .style(Style::default().bg(Color::Reset));

    frame.render_widget(paragraph, area);

    // Place cursor at the end of input
    let cursor_x = area.x + prefix.len() as u16 + cursor_vis as u16;
    let cursor_y = area.y;
    frame.set_cursor_position(ratatui::prelude::Position::new(cursor_x, cursor_y));
}

/// Render the status bar with profile, tools, and token count.
fn render_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let profile = app
        .tabs
        .get(app.active_tab)
        .map(|t| t.profile)
        .unwrap_or(&profiles::ALL_PROFILES[2]);

    let profile_text = format!(" {} {} ", profile.emoji, profile.name);
    let tools_text = format!(" 🔧 {} tools ", app.tools_count);
    let tokens_text = format!(" 📊 ~{} tokens ", app.token_count);
    let help_text = " Ctrl+P:palette  Ctrl+T:tab  Ctrl+W:close  Ctrl+C:quit ";

    // Pad the status bar
    let total_width = area.width as usize;
    let status_content = format!(
        "{:<width$}",
        format!("{profile_text}{tools_text}{tokens_text}{help_text}"),
        width = total_width
    );

    let paragraph = Paragraph::new(Text::styled(
        status_content,
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    frame.render_widget(paragraph, area);
}

// ---------------------------------------------------------------------------
// Key event handling
// ---------------------------------------------------------------------------

/// Handle a single key event, updating the app state.
fn handle_key_event(app: &mut App, key: KeyEvent) {
    // Global keybindings (work in all modes)
    if key.kind != KeyEventKind::Press {
        return;
    }

    // Ctrl+C or Ctrl+Q always quits
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('q')) {
        app.should_quit = true;
        return;
    }

    match app.mode {
        AppMode::CommandPalette => handle_palette_key(app, key),
        AppMode::ProviderPicker => handle_provider_picker_key(app, key),
        AppMode::ModelPicker => handle_model_picker_key(app, key),
        AppMode::Normal => handle_normal_key(app, key),
    }
}

/// Handle key events when in normal mode.
fn handle_normal_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Ctrl+T — new tab
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.add_tab();
        }
        // Ctrl+W — close tab
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.close_tab();
        }
        // Ctrl+P — toggle command palette
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.palette.visible = true;
            app.palette.filter.clear();
            app.palette.selected = 0;
            app.mode = AppMode::CommandPalette;
        }
        // Enter — send message
        KeyCode::Enter => {
            app.send_message();
        }
        // Tab — next tab
        KeyCode::Tab => {
            app.next_tab();
        }
        // BackTab (Shift+Tab) — previous tab
        KeyCode::BackTab => {
            app.prev_tab();
        }
        // PageUp — scroll up
        KeyCode::PageUp => {
            app.scroll_up();
        }
        // PageDown — scroll down
        KeyCode::PageDown => {
            app.scroll_down();
        }
        // End — scroll to bottom
        KeyCode::End => {
            app.scroll_to_bottom();
        }
        // Backspace — delete last character
        KeyCode::Backspace => {
            app.input_buffer.pop();
        }
        // Char — append to input buffer
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.input_buffer.push(c);
        }
        _ => {}
    }
}

/// Handle key events when the command palette is open.
fn handle_palette_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Esc — close palette
        KeyCode::Esc => {
            app.palette.visible = false;
            app.mode = AppMode::Normal;
        }
        // Enter — execute selected command
        KeyCode::Enter => {
            if let Some(cmd) = app.palette.selected_command() {
                let result = app.execute_command(cmd);
                app.messages.push(result);
            }
            app.palette.visible = false;
            // Only drop back to Normal if the command didn't open another
            // modal (e.g. /model switches to the ProviderPicker).
            if app.mode == AppMode::CommandPalette {
                app.mode = AppMode::Normal;
            }
        }
        // Up — previous command
        KeyCode::Up => {
            app.palette.select_prev();
        }
        // Down — next command
        KeyCode::Down => {
            app.palette.select_next();
        }
        // Backspace — delete filter char
        KeyCode::Backspace => {
            app.palette.pop_filter();
        }
        // Char — filter
        KeyCode::Char(c) => {
            app.palette.push_filter(c);
        }
        _ => {}
    }
}

/// Handle key events when the provider picker is open.
fn handle_provider_picker_key(app: &mut App, key: KeyEvent) {
    match key.code {
        // Esc — cancel
        KeyCode::Esc => {
            app.picker = Picker::default();
            app.mode = AppMode::Normal;
        }
        // Up / Down — navigate providers
        KeyCode::Up => app.picker.select_prev(app.providers.len()),
        KeyCode::Down => app.picker.select_next(app.providers.len()),
        // Enter — select provider, move on to model picker
        KeyCode::Enter => {
            if let Some(info) = app.providers.get(app.picker.selected).cloned() {
                app.picker.provider = Some(info);
                app.picker.selected = 0;
                app.mode = AppMode::ModelPicker;
            }
        }
        _ => {}
    }
}

/// Handle key events when the model picker is open.
fn handle_model_picker_key(app: &mut App, key: KeyEvent) {
    let model_count = app
        .picker
        .provider
        .as_ref()
        .map(|p| p.models.len())
        .unwrap_or(0);

    match key.code {
        // Esc — back to provider picker
        KeyCode::Esc => {
            app.picker.selected = 0;
            app.mode = AppMode::ProviderPicker;
        }
        // Up / Down — navigate models
        KeyCode::Up => app.picker.select_prev(model_count),
        KeyCode::Down => app.picker.select_next(model_count),
        // Enter — apply provider + model to the active tab
        KeyCode::Enter => {
            let Some(info) = app.picker.provider.clone() else {
                app.mode = AppMode::Normal;
                app.picker = Picker::default();
                return;
            };
            let Some(model) = info.models.get(app.picker.selected).cloned() else {
                return;
            };

            // Resolve the API key; warn and stay on mock if it's missing
            // rather than crashing on a real request.
            match load_api_key(&info.name) {
                Some(key) => {
                    if let Some(tab) = app.active_tab_mut() {
                        tab.provider = info.name.clone();
                        tab.model = model.clone();
                        tab.api_key = key;
                    }
                    app.messages
                        .push(format!("🤖 Switched to {} / {}", info.name, model));
                    app.mode = AppMode::Normal;
                    app.picker = Picker::default();
                }
                None => {
                    app.messages.push(format!(
                        "⚠️  No API key for {} — add it to ~/.tau/credentials.json",
                        info.name
                    ));
                    app.mode = AppMode::Normal;
                    app.picker = Picker::default();
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// The default mock greeting used when no real provider is configured.
fn mock_greeting() -> MockProvider {
    MockProvider::with_text(
        "🦀 Hello from Tua! I'm a mock agent streaming inside the TUI. \
         Type a message and press Enter to chat. Open the command palette \
         (Ctrl+P) and run /model to switch to a real provider.",
    )
}

/// Append `text` to the last assistant message in `tab`, creating a fresh
/// assistant message if the last message is not an assistant turn (or the
/// tab is empty).
fn append_assistant(tab: &mut Tab, text: String) {
    match tab.messages.last_mut() {
        Some(AgentMessage::Assistant { text: slot, .. }) => {
            if let Some(existing) = slot {
                existing.push_str(&text);
            } else {
                *slot = Some(text);
            }
        }
        _ => tab
            .messages
            .push(AgentMessage::assistant(Some(text), vec![])),
    }
}

/// Truncate a string to at most `max` characters, appending `...` if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

/// Produce a simple line-based unified diff between two text strings.
///
/// The output uses `-` and `+` prefixes for removed/added lines, and
/// includes surrounding context lines for readability.
fn simple_diff(before: &str, after: &str) -> String {
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();

    if before_lines == after_lines {
        return "  (no changes)".to_string();
    }

    let prefix_len = before_lines
        .iter()
        .zip(after_lines.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let suffix_len = before_lines
        .iter()
        .rev()
        .zip(after_lines.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = String::new();
    if prefix_len > 0 {
        result.push_str("@@ ... @@\n");
    }
    let ctx_start = prefix_len.saturating_sub(3);
    for line in &before_lines[ctx_start..prefix_len] {
        result.push_str(&format!("  {line}\n"));
    }
    for line in &before_lines[prefix_len..before_lines.len().saturating_sub(suffix_len)] {
        result.push_str(&format!("- {line}\n"));
    }
    for line in &after_lines[prefix_len..after_lines.len().saturating_sub(suffix_len)] {
        result.push_str(&format!("+ {line}\n"));
    }
    let after_end = after_lines.len().saturating_sub(suffix_len);
    for line in &after_lines[after_end..std::cmp::min(after_end + 3, after_lines.len())] {
        result.push_str(&format!("  {line}\n"));
    }
    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// `App::new()` creates an app with one tab using the rustacean profile.
    #[test]
    fn test_app_new_creates_single_tab() {
        let app = App::new();
        assert_eq!(app.tabs.len(), 1, "expected exactly 1 tab");
        assert_eq!(app.tabs[0].name, "Chat 1", "expected tab name 'Chat 1'");
        assert_eq!(
            app.tabs[0].profile.name, "rustacean",
            "expected rustacean profile"
        );
        assert_eq!(app.active_tab, 0, "active tab should be 0");
        assert!(app.input_buffer.is_empty(), "input buffer should be empty");
        assert!(app.messages.is_empty(), "messages list should be empty");
        assert_eq!(app.mode, AppMode::Normal, "initial mode should be Normal");
        assert!(!app.should_quit, "should not quit initially");
    }

    /// `App::default()` is equivalent to `App::new()`.
    #[test]
    fn test_app_default_equivalent_to_new() {
        let app1 = App::new();
        let app2 = App::default();
        assert_eq!(app1.tabs.len(), app2.tabs.len());
        assert_eq!(app1.active_tab, app2.active_tab);
        assert_eq!(app1.input_buffer, app2.input_buffer);
    }

    /// Adding a tab increments the tab count and switches to it.
    #[test]
    fn test_add_tab() {
        let mut app = App::new();
        app.add_tab();
        assert_eq!(app.tabs.len(), 2, "expected 2 tabs after adding one");
        assert_eq!(app.active_tab, 1, "should switch to the new tab");
        assert_eq!(
            app.tabs[1].name, "Chat 2",
            "new tab should be named 'Chat 2'"
        );
    }

    /// Adding multiple tabs works correctly.
    #[test]
    fn test_add_multiple_tabs() {
        let mut app = App::new();
        app.add_tab();
        app.add_tab();
        app.add_tab();
        assert_eq!(app.tabs.len(), 4);
        assert_eq!(app.active_tab, 3);
        assert_eq!(app.tabs[3].name, "Chat 4");
    }

    /// Closing a tab when there are multiple tabs removes it.
    #[test]
    fn test_close_tab_with_multiple() {
        let mut app = App::new();
        app.add_tab();
        assert_eq!(app.tabs.len(), 2);
        app.close_tab();
        assert_eq!(
            app.tabs.len(),
            1,
            "should have 1 tab after closing the second"
        );
        assert_eq!(app.active_tab, 0, "should fall back to tab 0");
    }

    /// Closing the last tab replaces it with a fresh one.
    #[test]
    fn test_close_last_tab_replaces() {
        let mut app = App::new();
        // Send a message first so the tab has content
        app.input_buffer = "Hello".into();
        app.send_message();
        assert_eq!(app.tabs[0].messages.len(), 1);

        // Close the only tab — it should be replaced with a fresh one
        app.close_tab();
        assert_eq!(app.tabs.len(), 1, "should still have 1 tab");
        assert_eq!(
            app.tabs[0].messages.len(),
            0,
            "replaced tab should have no messages"
        );
        assert_eq!(
            app.tabs[0].name, "Chat 1",
            "replaced tab should be named 'Chat 1'"
        );
    }

    /// `send_message` adds the input buffer as a user message and clears it.
    #[test]
    fn test_send_message() {
        let mut app = App::new();
        assert!(!app.send_message(), "empty send should return false");

        app.input_buffer = "Hello, Tua!".into();
        assert!(app.send_message(), "send should succeed");
        assert!(
            app.input_buffer.is_empty(),
            "input buffer should be cleared after send"
        );

        let tab = app.active_tab().unwrap();
        assert_eq!(tab.messages.len(), 1, "one message should be in the tab");
        match &tab.messages[0] {
            AgentMessage::User { text } => {
                assert_eq!(text, "Hello, Tua!");
            }
            _ => panic!("expected User message"),
        }
    }

    /// Sending multiple messages accumulates them in the tab.
    #[test]
    fn test_send_multiple_messages() {
        let mut app = App::new();
        app.input_buffer = "First".into();
        app.send_message();
        app.input_buffer = "Second".into();
        app.send_message();
        app.input_buffer = "Third".into();
        app.send_message();

        let tab = app.active_tab().unwrap();
        assert_eq!(tab.messages.len(), 3);
    }

    /// Tab switching works correctly.
    #[test]
    fn test_tab_switching() {
        let mut app = App::new();
        app.add_tab();
        app.add_tab();

        assert_eq!(app.active_tab, 2);
        app.next_tab();
        assert_eq!(app.active_tab, 0, "next should wrap to 0");
        app.prev_tab();
        assert_eq!(app.active_tab, 2, "prev should wrap to last tab");

        app.active_tab = 1;
        app.next_tab();
        assert_eq!(app.active_tab, 2);
        app.prev_tab();
        assert_eq!(app.active_tab, 1);
    }

    /// Scrolling adjusts the offset but does not go below 0.
    #[test]
    fn test_scroll() {
        let mut app = App::new();
        assert_eq!(app.tabs[0].scroll_offset, 0);

        app.scroll_up();
        assert_eq!(app.tabs[0].scroll_offset, 1);

        app.scroll_up();
        assert_eq!(app.tabs[0].scroll_offset, 2);

        app.scroll_down();
        assert_eq!(app.tabs[0].scroll_offset, 1);

        app.scroll_down();
        assert_eq!(app.tabs[0].scroll_offset, 0);

        app.scroll_down();
        assert_eq!(app.tabs[0].scroll_offset, 0, "cannot go below 0");
    }

    /// `scroll_to_bottom` resets the offset to 0.
    #[test]
    fn test_scroll_to_bottom() {
        let mut app = App::new();
        app.scroll_up();
        app.scroll_up();
        app.scroll_up();
        assert_eq!(app.tabs[0].scroll_offset, 3);

        app.scroll_to_bottom();
        assert_eq!(app.tabs[0].scroll_offset, 0);
    }

    /// The command palette starts hidden with an empty filter.
    #[test]
    fn test_command_palette_initial_state() {
        let app = App::new();
        assert!(!app.palette.visible);
        assert!(app.palette.filter.is_empty());
        assert_eq!(app.palette.selected, 0);
    }

    /// Filtering the command palette works.
    #[test]
    fn test_palette_filter() {
        let mut palette = CommandPalette::new();
        assert_eq!(palette.filtered_commands().len(), SLASH_COMMANDS.len());

        palette.push_filter('/');
        palette.push_filter('h');
        let filtered = palette.filtered_commands();
        assert!(filtered.contains(&"/help"));
        assert!(!filtered.contains(&"/clear"));

        palette.pop_filter();
        palette.pop_filter();
        assert!(palette.filter.is_empty());
        assert_eq!(palette.filtered_commands().len(), SLASH_COMMANDS.len());
    }

    /// Command palette selection wraps around.
    #[test]
    fn test_palette_selection_wraps() {
        let mut palette = CommandPalette::new();
        assert_eq!(palette.selected, 0);

        palette.select_prev();
        assert_eq!(
            palette.selected,
            SLASH_COMMANDS.len() - 1,
            "prev should wrap to last"
        );

        palette.select_next();
        assert_eq!(palette.selected, 0, "next from last should wrap to 0");
    }

    /// `execute_command` returns appropriate help text for known commands.
    #[test]
    fn test_execute_known_commands() {
        let mut app = App::new();

        let help_result = app.execute_command("/help");
        assert!(
            help_result.contains("/profile"),
            "/help should list commands"
        );

        let profile_result = app.execute_command("/profile");
        assert!(
            profile_result.contains("rustacean"),
            "/profile should show active profile"
        );

        let clear_result = app.execute_command("/clear");
        assert!(clear_result.contains("cleared"), "/clear should confirm");

        let unknown_result = app.execute_command("/nonexistent");
        assert!(
            unknown_result.contains("Unknown"),
            "unknown command should show error"
        );
    }

    /// Token count is roughly proportional to message length.
    #[test]
    fn test_token_count_update() {
        let mut app = App::new();

        // No messages → 0 tokens
        assert_eq!(app.token_count, 0);

        // Add a long message
        app.input_buffer = "Hello, Tua Agent! How are you today?".into();
        app.send_message();

        // ~38 chars / 4 ≈ 9 tokens
        assert!(
            app.token_count > 0,
            "token count should be > 0 after sending a message"
        );
    }

    /// All slash commands are listed.
    #[test]
    fn test_slash_commands_list() {
        assert_eq!(
            SLASH_COMMANDS.len(),
            13,
            "expected exactly 13 slash commands"
        );
        assert!(SLASH_COMMANDS.contains(&"/help"));
        assert!(SLASH_COMMANDS.contains(&"/clear"));
        assert!(SLASH_COMMANDS.contains(&"/profile"));
        assert!(SLASH_COMMANDS.contains(&"/resume"));
        assert!(SLASH_COMMANDS.contains(&"/undo"));
    }

    /// `truncate` shortens strings that exceed the limit.
    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 10), "hello w...");
        assert_eq!(truncate("hi", 2), "hi");
        assert_eq!(truncate("abc", 1), "...");
    }
    // -----------------------------------------------------------------------
    // Diff viewer tests
    // -----------------------------------------------------------------------

    /// Recording a file edit adds it to the active tab's edit history.
    #[test]
    fn test_record_file_edit() {
        let mut app = App::new();
        assert_eq!(app.tabs[0].edits.len(), 0);

        app.record_edit("src/main.rs", "old content", "new content");
        assert_eq!(app.tabs[0].edits.len(), 1);

        let edit = &app.tabs[0].edits[0];
        assert_eq!(edit.path, "src/main.rs");
        assert_eq!(edit.before, "old content");
        assert_eq!(edit.after, "new content");
        assert!(!edit.timestamp.is_empty(), "timestamp should be set");
    }

    /// `show_diff` returns a helpful message when no edits exist.
    #[test]
    fn test_diff_empty() {
        let mut app = App::new();
        let result = app.show_diff("");
        assert!(
            result.contains("No edits recorded"),
            "expected 'no edits' message"
        );
    }

    /// `show_diff` shows a summary when there is one edit.
    #[test]
    fn test_diff_single_edit() {
        let mut app = App::new();
        app.record_edit(
            "Cargo.toml",
            "[package] name = old",
            "[package] name = new",
        );

        let summary = app.show_diff("");
        assert!(summary.contains("Cargo.toml"), "summary should show path");
        assert!(summary.contains("#0"), "summary should show edit index");

        let detail = app.show_diff("0");
        assert!(
            detail.contains("Invalid"),
            "expected 'Invalid' for arg '0'"
        );

        let detail = app.show_diff("1");
        assert!(detail.contains("Cargo.toml"), "detail should show path");
        assert!(detail.contains("- [package] name = old"), "detail should show removed line");
        assert!(detail.contains("+ [package] name = new"), "detail should show added line");

        let last = app.show_diff("last");
        assert!(last.contains("Cargo.toml"), "last should show path");
        assert!(last.contains("- [package] name = old"), "last should show removed line");
    }

    /// `show_diff` with "last" argument returns the most recent edit.
    #[test]
    fn test_diff_last() {
        let mut app = App::new();
        app.record_edit("file1.rs", "a", "b");
        app.record_edit("file2.rs", "x", "y");

        let last = app.show_diff("last");
        assert!(last.contains("file2.rs"), "last should be file2.rs");
        assert!(last.contains("- x"), "last should show - x");
        assert!(last.contains("+ y"), "last should show + y");
    }


}
