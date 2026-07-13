//! 🎨 Rich terminal output helpers backed by [`rich_rs`].
//!
//! Provides styled terminal output: colored text, panels, and tables
//! for Tua's CLI and orchestrator output.

/// Print a styled success message.
pub fn success(msg: &str) {
    println!("{} {}", rich_rs::style("✔").green().bold(), msg);
}

/// Print a styled error message.
pub fn error(msg: &str) {
    eprintln!("{} {}", rich_rs::style("✖").red().bold(), msg);
}

/// Print a styled warning.
pub fn warn(msg: &str) {
    println!("{} {}", rich_rs::style("⚠").yellow(), msg);
}

/// Print a styled info message.
pub fn info(msg: &str) {
    println!("{} {}", rich_rs::style("ℹ").cyan(), msg);
}

/// Print a section header.
pub fn header(title: &str) {
    println!();
    println!("{}", rich_rs::style(title).bold().underline());
    println!();
}
