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

use crate::agent::AgentMessage;
use crate::profiles::{self, RustProfile};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::{Frame, Terminal};
use std::io::{Stdout, Write};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

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
    pub edits: Vec<String>,
    /// Vertical scroll offset for the messages area.
    pub scroll_offset: usize,
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
        }
    }
}

/// Mode the TUI is currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    /// Normal mode — typing input, navigating tabs.
    Normal,
    /// Command palette is open.
    CommandPalette,
}

/// Application state for the TUI.
#[derive(Debug)]
pub struct App {
    /// All open tabs.
    pub tabs: Vec<Tab>,
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
        let tab = Tab::new("Chat 1", profile);

        Self {
            tabs: vec![tab],
            active_tab: 0,
            input_buffer: String::new(),
            messages: Vec::new(),
            mode: AppMode::Normal,
            palette: CommandPalette::new(),
            should_quit: false,
            profile_emoji: profile.emoji.to_string(),
            tools_count: 14,
            token_count: 0,
        }
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
        let tab = Tab::new(format!("Chat {count}"), profile);
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        self.messages
            .push(format!("{} Created new tab 'Chat {count}'", profile.emoji));
    }

    /// Close the active tab. If it's the last tab, a new default tab is
    /// created instead.
    pub fn close_tab(&mut self) {
        if self.tabs.len() <= 1 {
            // Replace with a fresh tab instead of closing the last one.
            let profile = self
                .active_tab()
                .map(|t| t.profile)
                .unwrap_or(&profiles::ALL_PROFILES[2]);
            self.tabs[0] = Tab::new("Chat 1", profile);
            self.messages.push("🔄 Replaced last tab with a fresh one".into());
            return;
        }

        let removed = self.tabs.remove(self.active_tab);
        self.messages.push(format!("🗑️ Closed tab '{}'", removed.name));

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
            let total_chars: usize = tab
                .messages
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
            "/model" => "🤖 Model: deepseek/deepseek-v4-flash (configurable in config)".into(),
            "/tools" => "🔧 14 Rust tools registered: cargo, rustc, rustfmt, clippy, rustup, \
                         audit, outdated, udeps, deny, bench, doc, test-doc, wasm-pack, rustc-explain"
                .into(),
            "/skills" => "📚 Available skills: async-rust, cargo-workspace, concurrency, \
                          error-handling, lifetimes, macros, ownership-borrowing, smart-pointers, \
                          testing, wasm"
                .into(),
            "/config" => "⚙️  Run `tua-rs config` in terminal for full configuration".into(),
            "/diff" => "📝 Diff: use the `diff` tool in agent conversation".into(),
            "/permissions" => "🔐 Permission mode: ask (default) — use --permission flag to change"
                .into(),
            "/sessions" => "📋 Sessions: all tabs are active sessions".into(),
            "/rollback" => "⏪ Rollback: use the `checkpoint` tools to restore previous states".into(),
            "/undo" => "↩️ Undo: use the `edit` tools with rollback support".into(),
            _ => format!("❓ Unknown command: {command}"),
        }
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

impl TerminalGuard {
    /// Enter raw mode and set up the terminal for the TUI.
    pub fn enter() -> Result<(TerminalGuard, Terminal<CrosstermBackend<Stdout>>), Box<dyn std::error::Error>> {
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
            Constraint::Length(1),   // tab bar
            Constraint::Min(0),      // palette area (floating)
            Constraint::Min(1),      // chat area
            Constraint::Length(1),   // input line
            Constraint::Length(1),   // status bar
        ]
    } else {
        vec![
            Constraint::Length(1),   // tab bar
            Constraint::Min(1),      // chat area
            Constraint::Length(1),   // input line
            Constraint::Length(1),   // status bar
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
    render_input_line(frame, app, if app.palette.visible { chunks[3] } else { chunks[2] });
    render_status_bar(frame, app, if app.palette.visible { chunks[4] } else { chunks[3] });
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
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
        };

        let prefix = if is_active { " ▶ " } else { "   " };
        let text = format!("{}{} {} ", prefix, tab.profile.emoji, tab.name);

        spans.push(Span::styled(text, style));

        // Separator
        if i < app.tabs.len() - 1 {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    // Fill the remaining space
    spans.push(Span::styled(
        " ".repeat(area.width.saturating_sub(
            spans.iter().map(|s| s.content.len()).sum::<usize>() as u16,
        ) as usize),
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
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                );
                let content = Span::styled(text.clone(), Style::default().fg(Color::White));
                lines.push(Line::from(vec![prefix, content]));
            }
            AgentMessage::Assistant {
                text: Some(text), ..
            } => {
                let prefix = Span::styled(
                    "🤖 ",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
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
    let scroll = tab.scroll_offset.min(
        total_lines.saturating_sub(available_height).max(0),
    );
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
                .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
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
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
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

    // Ctrl+C always quits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return;
    }

    match app.mode {
        AppMode::CommandPalette => handle_palette_key(app, key),
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
        KeyCode::Char(c) => {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                app.input_buffer.push(c);
            }
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
            app.mode = AppMode::Normal;
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

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Truncate a string to at most `max` characters, appending `...` if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
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
        assert_eq!(
            app.tabs[0].name, "Chat 1",
            "expected tab name 'Chat 1'"
        );
        assert_eq!(
            app.tabs[0].profile.name, "rustacean",
            "expected rustacean profile"
        );
        assert_eq!(app.active_tab, 0, "active tab should be 0");
        assert!(app.input_buffer.is_empty(), "input buffer should be empty");
        assert!(app.messages.is_empty(), "messages list should be empty");
        assert_eq!(
            app.mode,
            AppMode::Normal,
            "initial mode should be Normal"
        );
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
        assert_eq!(
            app.active_tab,
            2,
            "prev should wrap to last tab"
        );

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
        assert!(help_result.contains("/profile"), "/help should list commands");

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
        assert!(app.token_count > 0, "token count should be > 0 after sending a message");
    }

    /// All slash commands are listed.
    #[test]
    fn test_slash_commands_list() {
        assert_eq!(
            SLASH_COMMANDS.len(),
            12,
            "expected exactly 12 slash commands"
        );
        assert!(SLASH_COMMANDS.contains(&"/help"));
        assert!(SLASH_COMMANDS.contains(&"/clear"));
        assert!(SLASH_COMMANDS.contains(&"/profile"));
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
}
