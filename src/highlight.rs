//! 🖌️ Syntax highlighting for code blocks in the TUI.
//!
//! Uses [`syntect`] to parse Rust source code and produce
//! [`ratatui::text::Span`]s with appropriate colours from the
//! current TUI theme.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// Highlight Rust source code and return ratatui spans.
///
/// The returned spans use the TUI theme's accent colour for keywords
/// and the foreground colour for normal text, ensuring readability
/// in both dark and light modes.
pub fn highlight_rust(code: &str, accent: Color, fg: Color, dim: Color) -> Vec<Span<'static>> {
    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();

    let syntax = ps
        .find_syntax_by_extension("rs")
        .unwrap_or_else(|| ps.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);

    let mut spans: Vec<Span<'static>> = Vec::new();

    for line in LinesWithEndings::from(code) {
        let line = line.trim_end_matches(|c| c == '\n' || c == '\r');
        if line.is_empty() {
            spans.push(Span::raw("\n"));
            continue;
        }
        let ranges: Vec<(syntect::highlighting::Style, &str)> =
            h.highlight_line(line, &ps).unwrap_or_default();

        for (style, text) in &ranges {
            let fg_color = if style
                .font_style
                .contains(syntect::highlighting::FontStyle::BOLD)
            {
                accent
            } else if style.foreground.r == 0 && style.foreground.g == 0 && style.foreground.b == 0
            {
                dim
            } else {
                fg
            };
            let mut span_style = Style::default().fg(fg_color);
            if style
                .font_style
                .contains(syntect::highlighting::FontStyle::BOLD)
            {
                span_style = span_style.add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(text.to_string(), span_style));
        }
        spans.push(Span::raw("\n"));
    }

    spans
}

/// Detect if a message contains a code block and extract language + code.
pub fn extract_code_blocks(text: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut lang = String::new();
    let mut code = String::new();

    for line in text.lines() {
        if line.starts_with("```") && !in_block {
            in_block = true;
            lang = line.trim_start_matches("```").trim().to_string();
            code.clear();
        } else if line.starts_with("```") && in_block {
            in_block = false;
            blocks.push((std::mem::take(&mut lang), std::mem::take(&mut code)));
        } else if in_block {
            if !code.is_empty() {
                code.push('\n');
            }
            code.push_str(line);
        }
    }

    // Handle unclosed block
    if in_block && !code.is_empty() {
        blocks.push((lang, code));
    }

    blocks
}
