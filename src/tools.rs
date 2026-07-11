//! Rust tool definitions for the agent.
//!
//! Each tool has a name, description, JSON schema, and an async executor.
//! Based on Tua Agent v0.0.2 Python tools (14 tools).

use crate::agent::{AgentTool, ToolExecutor};
use std::sync::Arc;

/// Create the standard Rust tool set.
pub fn rust_tools() -> Vec<AgentTool> {
    vec![
        cargo_tool(),
        rustc_explain_tool(),
        clippy_tool(),
        rustfmt_tool(),
    ]
}

fn cargo_tool() -> AgentTool {
    AgentTool {
        name: "cargo".to_string(),
        description: "Run cargo commands: build, test, check, clippy, fmt, bench, doc".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "subcommand": {
                    "type": "string",
                    "enum": ["build", "test", "check", "clippy", "fmt", "bench", "doc"],
                    "description": "Cargo subcommand to run"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments"
                }
            },
            "required": ["subcommand"]
        }),
        executor: Arc::new(cargo_executor),
    }
}

fn rustc_explain_tool() -> AgentTool {
    AgentTool {
        name: "rustc_explain".to_string(),
        description: "Get official Rust compiler error explanation (e.g. E0502)".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "error_code": {
                    "type": "string",
                    "description": "Error code to explain (e.g. E0502, E0382)"
                }
            },
            "required": ["error_code"]
        }),
        executor: Arc::new(rustc_explain_executor),
    }
}

fn clippy_tool() -> AgentTool {
    AgentTool {
        name: "clippy".to_string(),
        description: "Run cargo clippy for Rust linting".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "deny_warnings": {"type": "boolean", "default": true},
                "fix": {"type": "boolean", "default": false}
            }
        }),
        executor: Arc::new(clippy_executor),
    }
}

fn rustfmt_tool() -> AgentTool {
    AgentTool {
        name: "rustfmt".to_string(),
        description: "Format Rust code according to style guidelines".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "check": {"type": "boolean", "description": "Only check, don't format"}
            }
        }),
        executor: Arc::new(rustfmt_executor),
    }
}

// ── Executors ────────────────────────────────────────────────────────────

async fn run_command(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run {}: {}", cmd, e))?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);
    
    if output.status.success() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

async fn cargo_executor(
    args: serde_json::Value,
) -> crate::agent::ToolResult {
    let sub = args["subcommand"].as_str().unwrap_or("check");
    let result = run_command("cargo", &[sub]).await;
    match result {
        Ok(out) => crate::agent::ToolResult { ok: true, content: out },
        Err(err) => crate::agent::ToolResult { ok: false, content: err },
    }
}

async fn rustc_explain_executor(
    args: serde_json::Value,
) -> crate::agent::ToolResult {
    let code = args["error_code"].as_str().unwrap_or("E0308");
    let result = run_command("rustc", &["--explain", code]).await;
    match result {
        Ok(out) => crate::agent::ToolResult { ok: true, content: out },
        Err(err) => crate::agent::ToolResult { ok: false, content: err },
    }
}

async fn clippy_executor(
    args: serde_json::Value,
) -> crate::agent::ToolResult {
    let mut cmd_args = vec!["clippy"];
    if args["deny_warnings"].as_bool().unwrap_or(true) {
        cmd_args.push("--");
        cmd_args.push("-D");
        cmd_args.push("warnings");
    }
    let result = run_command("cargo", &cmd_args).await;
    match result {
        Ok(out) => crate::agent::ToolResult { ok: true, content: out },
        Err(err) => crate::agent::ToolResult { ok: false, content: err },
    }
}

async fn rustfmt_executor(
    args: serde_json::Value,
) -> crate::agent::ToolResult {
    let mut cmd_args = vec!["fmt"];
    if args["check"].as_bool().unwrap_or(false) {
        cmd_args.push("--check");
    }
    let result = run_command("cargo", &cmd_args).await;
    match result {
        Ok(out) => crate::agent::ToolResult { ok: true, content: out },
        Err(err) => crate::agent::ToolResult { ok: false, content: err },
    }
}
