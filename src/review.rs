//! Multi-agent review (#19) — background clippy + code analysis.
//!
//! After the agent edits `.rs` files, this module runs `cargo clippy` in the
//! background and parses structured findings (errors, warnings, info).

use std::process::Command;

/// A single finding from the code reviewer.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewFinding {
    pub severity: String, // "error" | "warning" | "info"
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// Run `cargo clippy` on the project and parse findings.
pub fn review_edits(_files: &[String], cwd: Option<&str>) -> Vec<ReviewFinding> {
    let mut cmd = Command::new("cargo");
    cmd.args(["clippy", "--message-format=short"]);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }

    let output = match cmd.output() {
        Ok(o) => o,
        Err(_) => {
            return vec![ReviewFinding {
                severity: "info".into(),
                file: "".into(),
                line: 0,
                message: "cargo clippy not available — skipping review".into(),
            }]
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_clippy_output(&stderr)
}

/// Parse clippy short-format output lines like:
/// `src/main.rs:7:5: error: expected one of ..., found \`x\``  
/// `src/lib.rs:7:5: warning: ...`
fn parse_clippy_output(output: &str) -> Vec<ReviewFinding> {
    let mut findings = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse pattern: "file:line:col: severity: message"
        let parts: Vec<&str> = line.splitn(5, ':').collect();
        if parts.len() < 5 {
            continue;
        }

        let file = parts[0].trim().to_string();
        let line_num: u32 = parts[1].trim().parse().unwrap_or(0);
        // parts[2] is column — skip
        let severity = parts[3].trim().to_string();
        let message = parts[4].trim().to_string();

        if severity == "error" || severity == "warning" || severity == "info" {
            findings.push(ReviewFinding {
                severity,
                file,
                line: line_num,
                message,
            });
        }
    }

    findings
}

/// Format review findings for display.
pub fn format_review(findings: &[ReviewFinding]) -> String {
    if findings.is_empty() {
        return String::from("✅ Review: no issues found");
    }

    let error_count = findings.iter().filter(|f| f.severity == "error").count();
    let warn_count = findings.iter().filter(|f| f.severity == "warning").count();

    let mut out = format!(
        "🔍 Review: {} errors, {} warnings\n",
        error_count, warn_count
    );

    for f in findings.iter().take(10) {
        let icon = match f.severity.as_str() {
            "error" => "❌",
            "warning" => "⚠️",
            _ => "ℹ️",
        };
        out.push_str(&format!(
            "  {} {}:{} — {}\n",
            icon, f.file, f.line, f.message
        ));
    }

    if findings.len() > 10 {
        out.push_str(&format!("  ... and {} more\n", findings.len() - 10));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_clippy_error_line() {
        let line = "src/main.rs:7:5: error: expected one of `!`, found `x`";
        let findings = parse_clippy_output(line);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "error");
        assert_eq!(findings[0].file, "src/main.rs");
        assert_eq!(findings[0].line, 7);
    }

    #[test]
    fn test_parse_clippy_warning_line() {
        let line = "src/lib.rs:42:9: warning: unused variable: `x`";
        let findings = parse_clippy_output(line);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, "warning");
        assert_eq!(findings[0].line, 42);
    }

    #[test]
    fn test_parse_multiple_lines() {
        let output =
            "src/a.rs:1:1: error: E001\nsrc/b.rs:2:2: warning: W002\nsrc/c.rs:3:3: info: I003\n";
        let findings = parse_clippy_output(output);
        assert_eq!(findings.len(), 3);
    }

    #[test]
    fn test_format_empty() {
        let result = format_review(&[]);
        assert!(result.contains("no issues"));
    }

    #[test]
    fn test_format_with_findings() {
        let findings = vec![ReviewFinding {
            severity: "error".into(),
            file: "main.rs".into(),
            line: 10,
            message: "bad code".into(),
        }];
        let result = format_review(&findings);
        assert!(result.contains("❌"));
        assert!(result.contains("bad code"));
    }

    #[test]
    fn test_review_edits_no_cargo() {
        // Should not panic even if cargo isn't available in test context
        let findings = review_edits(&[], Some("/nonexistent"));
        // Either empty or has the "not available" info message
        assert!(findings.is_empty() || findings[0].severity == "info");
    }
}
