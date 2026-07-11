//! Rust tool definitions for the agent.
//!
//! Each tool has a name, description, JSON schema, and an async executor
//! that runs a subprocess via [`tokio::process::Command`].
//!
//! Based on Tua Agent v0.0.2 Python tools (14 tools).

use crate::agent::{AgentError, AgentTool, ToolExecutor};
use std::sync::Arc;

/// Create the standard Rust tool set (14 tools matching Python tua-agent).
pub fn rust_tools() -> Vec<AgentTool> {
    vec![
        cargo_tool(),
        rustc_tool(),
        rustfmt_tool(),
        clippy_tool(),
        rustup_tool(),
        cargo_audit_tool(),
        cargo_outdated_tool(),
        cargo_udeps_tool(),
        cargo_deny_tool(),
        cargo_bench_tool(),
        cargo_doc_tool(),
        cargo_test_doc_tool(),
        wasm_pack_tool(),
        rustc_explain_tool(),
    ]
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

fn cargo_tool() -> AgentTool {
    AgentTool::new(
        "cargo",
        "Run cargo commands: build, test, check, clippy, fmt, bench, doc, update, tree, add, remove, run, clean, fix",
        serde_json::json!({
            "type": "object",
            "properties": {
                "subcommand": {
                    "type": "string",
                    "enum": ["build", "test", "check", "clippy", "fmt", "bench", "doc", "update", "tree", "add", "remove", "run", "clean", "fix"],
                    "description": "Cargo subcommand to run"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments passed to the cargo subcommand"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (defaults to project root)"
                }
            },
            "required": ["subcommand"]
        }),
        make_executor("cargo", |args| {
            let sub = args["subcommand"]
                .as_str()
                .unwrap_or("check")
                .to_string();
            let extra: Vec<String> = args["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = args["cwd"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg(&sub);
                if !extra.is_empty() {
                    cmd.args(&extra);
                }
                if let Some(dir) = cwd {
                    cmd.current_dir(&dir);
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn rustc_tool() -> AgentTool {
    AgentTool::new(
        "rustc",
        "Run the Rust compiler directly — check syntax, explain errors, verify editions",
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["explain", "check", "version", "edition"],
                    "description": "Action: explain (error code), check (compile single file), version"
                },
                "target": {
                    "type": "string",
                    "description": "For explain: error code (e.g., E0502). For check: path to .rs file."
                }
            },
            "required": ["action"]
        }),
        make_executor("rustc", |args| {
            let action = args["action"].as_str().unwrap_or("version").to_string();
            let target = args["target"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("rustc");
                match action.as_str() {
                    "explain" => {
                        let code = target.as_deref().unwrap_or("E0308");
                        cmd.arg("--explain").arg(code);
                    }
                    "check" => {
                        if let Some(path) = target {
                            cmd.arg(path);
                        } else {
                            return Err("rustc check requires a 'target' file path".into());
                        }
                    }
                    "version" => {
                        cmd.arg("--version");
                    }
                    "edition" => {
                        cmd.arg("--edition").arg("2021");
                        if let Some(path) = target {
                            cmd.arg(path);
                        }
                    }
                    _ => return Err(format!("unknown rustc action: {action}")),
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn rustfmt_tool() -> AgentTool {
    AgentTool::new(
        "rustfmt",
        "Format Rust code according to style guidelines (rustfmt)",
        serde_json::json!({
            "type": "object",
            "properties": {
                "check": {
                    "type": "boolean",
                    "description": "Check formatting without modifying files",
                    "default": false
                },
                "files": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Specific files to format (defaults to all .rs files)"
                }
            }
        }),
        make_executor("rustfmt", |args| {
            let check = args["check"].as_bool().unwrap_or(false);
            let files: Vec<String> = args["files"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Box::pin(async move {
                if !files.is_empty() {
                    let mut cmd = tokio::process::Command::new("rustfmt");
                    if check {
                        cmd.arg("--check");
                    }
                    cmd.args(&files);
                    run_cmd_output(&mut cmd).await
                } else {
                    // Default: run `cargo fmt`
                    let mut cmd = tokio::process::Command::new("cargo");
                    cmd.arg("fmt");
                    if check {
                        cmd.arg("--check");
                    }
                    run_cmd_output(&mut cmd).await
                }
            })
        }),
    )
}

fn clippy_tool() -> AgentTool {
    AgentTool::new(
        "clippy",
        "Run Clippy — Rust linter with 550+ lint rules for catching mistakes and improving code",
        serde_json::json!({
            "type": "object",
            "properties": {
                "deny_warnings": {
                    "type": "boolean",
                    "description": "Treat all warnings as errors (-D warnings)",
                    "default": true
                },
                "allow": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Lint names to allow (e.g., clippy::too_many_arguments)"
                },
                "fix": {
                    "type": "boolean",
                    "description": "Auto-fix suggestions where possible (--fix)",
                    "default": false
                }
            }
        }),
        make_executor("clippy", |args| {
            let deny_warnings = args["deny_warnings"].as_bool().unwrap_or(true);
            let fix = args["fix"].as_bool().unwrap_or(false);
            let allow: Vec<String> = args["allow"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("clippy");
                if fix {
                    cmd.arg("--fix");
                }
                if deny_warnings {
                    cmd.arg("--");
                    cmd.arg("-D");
                    cmd.arg("warnings");
                }
                if !allow.is_empty() {
                    if !deny_warnings {
                        cmd.arg("--");
                    }
                    for lint in &allow {
                        cmd.arg("-A");
                        cmd.arg(lint);
                    }
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn rustup_tool() -> AgentTool {
    AgentTool::new(
        "rustup",
        "Manage Rust toolchains — check, install, update, switch targets and components",
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["show", "update", "check", "target", "component", "default", "toolchain"],
                    "description": "rustup action"
                },
                "target": {
                    "type": "string",
                    "description": "Target name, component name, or toolchain version"
                }
            },
            "required": ["action"]
        }),
        make_executor("rustup", |args| {
            let action = args["action"].as_str().unwrap_or("show").to_string();
            let target = args["target"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("rustup");
                cmd.arg(&action);
                if let Some(t) = target {
                    cmd.arg(&t);
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_audit_tool() -> AgentTool {
    AgentTool::new(
        "cargo_audit",
        "Run cargo audit to check dependencies for known security vulnerabilities (RustSec advisory database)",
        serde_json::json!({
            "type": "object",
            "properties": {
                "fix": {
                    "type": "boolean",
                    "description": "Attempt to fix vulnerabilities automatically",
                    "default": false
                }
            }
        }),
        make_executor("cargo_audit", |args| {
            let fix = args["fix"].as_bool().unwrap_or(false);
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("audit");
                if fix {
                    cmd.arg("fix");
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_outdated_tool() -> AgentTool {
    AgentTool::new(
        "cargo_outdated",
        "Run cargo outdated to display dependencies that have newer versions available",
        serde_json::json!({
            "type": "object",
            "properties": {
                "workspace": {
                    "type": "boolean",
                    "description": "Check every crate in the workspace",
                    "default": true
                }
            }
        }),
        make_executor("cargo_outdated", |args| {
            let workspace = args["workspace"].as_bool().unwrap_or(true);
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("outdated");
                if workspace {
                    cmd.arg("--workspace");
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_udeps_tool() -> AgentTool {
    AgentTool::new(
        "cargo_udeps",
        "Run cargo udeps to detect unused dependencies in Cargo.toml",
        serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        make_executor("cargo_udeps", |_args| {
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("udeps");
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_deny_tool() -> AgentTool {
    AgentTool::new(
        "cargo_deny",
        "Run cargo deny to check for license violations, security advisories, banned crates, and disallowed sources",
        serde_json::json!({
            "type": "object",
            "properties": {
                "check": {
                    "type": "string",
                    "enum": ["advisories", "bans", "licenses", "sources"],
                    "description": "What to check: advisories, bans, licenses, or sources",
                    "default": "advisories"
                }
            }
        }),
        make_executor("cargo_deny", |args| {
            let check = args["check"]
                .as_str()
                .unwrap_or("advisories")
                .to_string();
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("deny");
                cmd.arg("check");
                cmd.arg(&check);
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_bench_tool() -> AgentTool {
    AgentTool::new(
        "cargo_bench",
        "Run cargo bench to execute criterion/other benchmarks and capture timing output",
        serde_json::json!({
            "type": "object",
            "properties": {
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments (e.g. a benchmark name filter, --bench <name>, -- --save-baseline)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (defaults to project root)"
                }
            }
        }),
        make_executor("cargo_bench", |args| {
            let extra: Vec<String> = args["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = args["cwd"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("bench");
                if !extra.is_empty() {
                    cmd.args(&extra);
                }
                if let Some(dir) = cwd {
                    cmd.current_dir(&dir);
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_doc_tool() -> AgentTool {
    AgentTool::new(
        "cargo_doc",
        "Run cargo doc to build API documentation and report whether it builds successfully",
        serde_json::json!({
            "type": "object",
            "properties": {
                "open": {
                    "type": "boolean",
                    "description": "Open the generated docs in a browser (--open)",
                    "default": false
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments (e.g. --no-deps, --document-private-items)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (defaults to project root)"
                }
            }
        }),
        make_executor("cargo_doc", |args| {
            let open = args["open"].as_bool().unwrap_or(false);
            let extra: Vec<String> = args["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = args["cwd"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("doc");
                if open {
                    cmd.arg("--open");
                }
                if !extra.is_empty() {
                    cmd.args(&extra);
                }
                if let Some(dir) = cwd {
                    cmd.current_dir(&dir);
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn cargo_test_doc_tool() -> AgentTool {
    AgentTool::new(
        "cargo_test_doc",
        "Run cargo test --doc to compile and execute doc-tests embedded in rustdoc comments",
        serde_json::json!({
            "type": "object",
            "properties": {
                "package": {
                    "type": "string",
                    "description": "Restrict doc-tests to a specific package (-p <name>)"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments passed to `cargo test --doc`"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (defaults to project root)"
                }
            }
        }),
        make_executor("cargo_test_doc", |args| {
            let package = args["package"].as_str().map(|s| s.to_string());
            let extra: Vec<String> = args["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = args["cwd"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("test");
                cmd.arg("--doc");
                if let Some(pkg) = package {
                    cmd.arg("-p");
                    cmd.arg(&pkg);
                }
                if !extra.is_empty() {
                    cmd.args(&extra);
                }
                if let Some(dir) = cwd {
                    cmd.current_dir(&dir);
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn wasm_pack_tool() -> AgentTool {
    AgentTool::new(
        "wasm_pack",
        "Run wasm-pack to build, test, pack, or scaffold Rust code targeting WebAssembly (wasm32-unknown-unknown)",
        serde_json::json!({
            "type": "object",
            "properties": {
                "subcommand": {
                    "type": "string",
                    "enum": ["build", "test", "pack", "new"],
                    "description": "wasm-pack subcommand",
                    "default": "build"
                },
                "target": {
                    "type": "string",
                    "enum": ["web", "bundler", "nodejs", "deno"],
                    "description": "Build target for `wasm-pack build` (web, bundler, nodejs, deno)"
                },
                "args": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Additional arguments (e.g. --release, --dev)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (defaults to project root)"
                }
            }
        }),
        make_executor("wasm_pack", |args| {
            let subcommand = args["subcommand"]
                .as_str()
                .unwrap_or("build")
                .to_string();
            let target = args["target"].as_str().map(|s| s.to_string());
            let extra: Vec<String> = args["args"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let cwd = args["cwd"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("wasm-pack");
                cmd.arg(&subcommand);
                if let Some(t) = target {
                    cmd.arg("--target");
                    cmd.arg(&t);
                }
                if !extra.is_empty() {
                    cmd.args(&extra);
                }
                if let Some(dir) = cwd {
                    cmd.current_dir(&dir);
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

fn rustc_explain_tool() -> AgentTool {
    AgentTool::new(
        "rustc_explain",
        "Get detailed Rust compiler error explanation: rustc --explain <error_code> (e.g., E0502, E0382)",
        serde_json::json!({
            "type": "object",
            "properties": {
                "error_code": {
                    "type": "string",
                    "description": "Rust compiler error code to explain (e.g., E0502, E0382)"
                }
            },
            "required": ["error_code"]
        }),
        make_executor("rustc_explain", |args| {
            let code = args["error_code"]
                .as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "E0308".to_string());
            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("rustc");
                cmd.arg("--explain");
                cmd.arg(&code);
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

// ---------------------------------------------------------------------------
// Executor helpers
// ---------------------------------------------------------------------------

/// Run a [`tokio::process::Command`] and capture stdout/stderr.
///
/// Returns `Ok(combined_output)` on success (exit code 0), or
/// `Err(combined_output)` on failure (non-zero exit or spawn error).
async fn run_cmd_output(cmd: &mut tokio::process::Command) -> Result<String, String> {
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Failed to spawn command: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let combined = if stderr.is_empty() {
        stdout
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };

    if output.status.success() {
        Ok(combined)
    } else {
        Err(combined)
    }
}

/// Wrap an async function into a [`ToolExecutor`] (erased closure).
///
/// The inner function receives the parsed JSON arguments and returns
/// `Result<String, String>` (success content or error message).
/// Errors are wrapped in [`AgentError::ToolExecution`].
fn make_executor<F>(tool_name: &'static str, f: F) -> ToolExecutor
where
    F: Fn(
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'static>,
        > + Send
        + Sync
        + 'static,
{
    let name = tool_name.to_string();
    Arc::new(move |args| {
        let fut = f(args);
        let name = name.clone();
        Box::pin(async move {
            match fut.await {
                Ok(content) => Ok(content),
                Err(msg) => Err(AgentError::ToolExecution {
                    tool_name: name,
                    message: msg,
                }),
            }
        })
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_count() {
        let tools = rust_tools();
        assert_eq!(tools.len(), 14, "expected exactly 14 tools");
    }

    #[tokio::test]
    #[ignore = "requires Cargo.toml in working directory (passes via cargo test --lib)"]
    async fn test_cargo_check_executes_successfully() {
        let tools = rust_tools();
        let cargo = tools.iter().find(|t| t.name == "cargo").unwrap();
        let args = serde_json::json!({
            "subcommand": "check"
        });
        let result = cargo.execute(args).await;
        assert!(result.is_ok(), "cargo check failed: {:?}", result);
    }

    #[tokio::test]
    #[ignore = "requires Cargo.toml in working directory (passes via cargo test --lib)"]
    async fn test_cargo_check_captures_output() {
        let tools = rust_tools();
        let cargo = tools.iter().find(|t| t.name == "cargo").unwrap();
        let args = serde_json::json!({
            "subcommand": "check"
        });
        let result = cargo.execute(args).await.unwrap();
        // Cargo output varies: it may say "Checking", "Compiling", "Finished",
        // or "Blocking waiting for file lock" if another process holds the lock.
        assert!(!result.is_empty(), "cargo check output should not be empty");
        let has_expected_content = result.contains("Checking")
            || result.contains("Finished")
            || result.contains("Compiling")
            || result.contains("Blocking")
            || result.contains("Fresh");
        assert!(
            has_expected_content,
            "output should mention compilation or build: {result}"
        );
    }

    #[tokio::test]
    async fn test_rustc_explain_executes_successfully() {
        let tools = rust_tools();
        let tool = tools.iter().find(|t| t.name == "rustc_explain").unwrap();
        let args = serde_json::json!({
            "error_code": "E0502"
        });
        let result = tool.execute(args).await;
        assert!(result.is_ok(), "rustc --explain E0502 failed: {:?}", result);
    }

    #[tokio::test]
    async fn test_rustc_explain_contains_error_explanation() {
        let tools = rust_tools();
        let tool = tools.iter().find(|t| t.name == "rustc_explain").unwrap();
        let args = serde_json::json!({
            "error_code": "E0502"
        });
        let output = tool.execute(args).await.unwrap();
        assert!(
            output.contains("borrow") || output.contains("mutable"),
            "E0502 explanation should mention borrow/mutable, got: {output}"
        );
    }

    #[tokio::test]
    async fn test_rustc_explain_with_unknown_code() {
        let tools = rust_tools();
        let tool = tools.iter().find(|t| t.name == "rustc_explain").unwrap();
        let args = serde_json::json!({
            "error_code": "E9999"
        });
        let result = tool.execute(args).await;
        // rustc --explain with an unknown code may still succeed (printing help text)
        // or fail with an error. Either way, the output should be informative.
        match result {
            Ok(output) => {
                // rustc may print "error[E9999]: unknown error code" or help text
                assert!(
                    !output.is_empty(),
                    "output for unknown code should not be empty"
                );
            }
            Err(err) => {
                let err_msg = err.to_string();
                assert!(
                    err_msg.contains("tool") || err_msg.contains("E9999") || err_msg.contains("rustc"),
                    "error should mention the tool, got: {err_msg}"
                );
            }
        }
    }

    #[test]
    fn test_all_tools_have_unique_names() {
        let tools = rust_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            tools.len(),
            "duplicate tool names found: {names:?}"
        );
    }

    #[test]
    fn test_cargo_tool_schema() {
        let tools = rust_tools();
        let cargo = tools.iter().find(|t| t.name == "cargo").unwrap();
        assert!(cargo.input_schema["properties"]["subcommand"].is_object());
        assert_eq!(
            cargo.input_schema["required"][0].as_str().unwrap(),
            "subcommand"
        );
    }

    #[test]
    fn test_rustc_explain_tool_schema() {
        let tools = rust_tools();
        let tool = tools.iter().find(|t| t.name == "rustc_explain").unwrap();
        assert_eq!(
            tool.input_schema["required"][0].as_str().unwrap(),
            "error_code"
        );
        assert!(tool.description.contains("E0502"));
    }

    #[test]
    fn test_wasm_pack_tool_schema() {
        let tools = rust_tools();
        let tool = tools.iter().find(|t| t.name == "wasm_pack").unwrap();
        let sub_enum = &tool.input_schema["properties"]["subcommand"]["enum"];
        let variants: Vec<&str> = sub_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(variants.contains(&"build"));
        assert!(variants.contains(&"test"));
        assert!(variants.contains(&"pack"));
        assert!(variants.contains(&"new"));
    }

    #[test]
    fn test_cargo_deny_tool_schema() {
        let tools = rust_tools();
        let tool = tools.iter().find(|t| t.name == "cargo_deny").unwrap();
        let check_enum = &tool.input_schema["properties"]["check"]["enum"];
        let variants: Vec<&str> = check_enum
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(variants.contains(&"advisories"));
        assert!(variants.contains(&"bans"));
        assert!(variants.contains(&"licenses"));
        assert!(variants.contains(&"sources"));
    }
}
