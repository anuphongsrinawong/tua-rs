//! Rust tool definitions for the agent.
//!
//! Each tool has a name, description, JSON schema, and an async executor
//! that runs a subprocess via [`tokio::process::Command`].
//!
//! Based on Tua Agent v0.0.2 Python tools (14 tools).

use crate::agent::{AgentError, AgentResult, AgentTool, ToolExecutor};
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
        grep_tool(),
        cargo_add_tool(),
        cargo_expand_tool(),
        code_coverage_tool(),
        cargo_mutants_tool(),
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
            let sub_result: crate::agent::AgentResult<String> = args["subcommand"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| crate::agent::AgentError::InvalidToolCall {
                    tool_name: "cargo".into(),
                    message: "missing required field 'subcommand'".into(),
                });
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
                let sub = sub_result?;
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
            let action_result = args["action"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AgentError::InvalidToolCall {
                    tool_name: "rustc".into(),
                    message: "missing required field 'action'".into(),
                });
            let target = args["target"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let action = action_result?;
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
                            return Err(AgentError::InvalidToolCall {
                                tool_name: "rustc".into(),
                                message: "'check' action requires a 'target' file path".into(),
                            });
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
                    _ => {
                        return Err(AgentError::InvalidToolCall {
                            tool_name: "rustc".into(),
                            message: format!("unknown rustc action: {action}"),
                        })
                    }
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
            let action_result = args["action"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AgentError::InvalidToolCall {
                    tool_name: "rustup".into(),
                    message: "missing required field 'action'".into(),
                });
            let target = args["target"].as_str().map(|s| s.to_string());
            Box::pin(async move {
                let action = action_result?;
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
                .to_string();  // Optional — schema declares default "advisories"
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
            let code_result = args["error_code"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| AgentError::InvalidToolCall {
                    tool_name: "rustc_explain".into(),
                    message: "missing required field 'error_code'".into(),
                });
            Box::pin(async move {
                let code = code_result?;
                let mut cmd = tokio::process::Command::new("rustc");
                cmd.arg("--explain");
                cmd.arg(&code);
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

// ── Grep Search Tool ─────────────────────────────────────────────────

fn grep_tool() -> AgentTool {
    AgentTool::new(
        "grep",
        "Search Rust source code using regex patterns. Returns matching lines with file paths and line numbers. Useful for finding function definitions, trait implementations, error types, or any code pattern.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for (e.g., 'fn split', 'pub struct', 'impl.*Trait', 'E0308')"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file path to search in (defaults to src/)"
                },
                "glob": {
                    "type": "string",
                    "description": "File glob pattern to filter (e.g., '*.rs', '*.toml') — defaults to '*.rs'"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum results to return (default: 50)"
                }
            },
            "required": ["pattern"]
        }),
        make_executor("grep", |args| {
            let pattern = args["pattern"].as_str().unwrap_or("").to_string();
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("src");
            let glob = args.get("glob").and_then(|v| v.as_str()).unwrap_or("*.rs");
            let max = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

            // Use ripgrep (rg) if available, fall back to grep
            let has_rg = std::process::Command::new("rg")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            let mut cmd = if has_rg {
                let mut c = tokio::process::Command::new("rg");
                c.arg("--line-number")
                 .arg("--no-heading")
                 .arg("--color=never")
                 .arg("--max-count").arg(max.to_string())
                 .arg("--glob").arg(glob)
                 .arg(&pattern)
                 .arg(path);
                c
            } else {
                let mut c = tokio::process::Command::new("grep");
                c.arg("-rn")
                 .arg("--include").arg(glob)
                 .arg("-m").arg(max.to_string())
                 .arg(&pattern)
                 .arg(path);
                c
            };

            Box::pin(async move {
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

// ── Cargo Add Tool ───────────────────────────────────────────────────

fn cargo_add_tool() -> AgentTool {
    AgentTool::new(
        "cargo_add",
        "Add a new dependency to Cargo.toml. Runs `cargo add <crate>`. Supports features, version, and optional flags. Use this instead of manually editing Cargo.toml.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "crate_name": {
                    "type": "string",
                    "description": "Name of the crate to add (e.g., 'serde', 'tokio', 'thiserror')"
                },
                "features": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Features to enable (e.g., ['derive', 'json'])"
                },
                "version": {
                    "type": "string",
                    "description": "Specific version to pin (e.g., '1.0', '0.8') — omit for latest"
                },
                "optional": {
                    "type": "boolean",
                    "description": "Add as optional dependency"
                },
                "dev": {
                    "type": "boolean",
                    "description": "Add as dev-dependency"
                }
            },
            "required": ["crate_name"]
        }),
        make_executor("cargo_add", |args| {
            let crate_name = args["crate_name"].as_str().unwrap_or("").to_string();
            let features: Vec<String> = args.get("features")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let version = args.get("version").and_then(|v| v.as_str()).map(String::from);
            let optional = args.get("optional").and_then(|v| v.as_bool()).unwrap_or(false);
            let dev = args.get("dev").and_then(|v| v.as_bool()).unwrap_or(false);

            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("add").arg(&crate_name);
                if dev { cmd.arg("--dev"); }
                if optional { cmd.arg("--optional"); }
                if let Some(ref ver) = version { cmd.args(["--version", ver]); }
                if !features.is_empty() {
                    cmd.arg("--features").arg(features.join(","));
                }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

// ---------------------------------------------------------------------------
// Executor helpers
// ---------------------------------------------------------------------------

// ── Cargo Expand Tool ────────────────────────────────────────────────

fn cargo_expand_tool() -> AgentTool {
    AgentTool::new(
        "cargo_expand",
        "Expand Rust macros to see the actual generated code. Useful for debugging proc-macros, derive macros, or understanding what code #[tokio::main] generates. Requires `cargo-expand` installed.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "module": {
                    "type": "string",
                    "description": "Module or item to expand (e.g., 'my_module', 'my_module::my_function', 'src/main.rs')"
                },
                "themes": {
                    "type": "boolean",
                    "description": "Expand with theme/test expansion"
                }
            },
            "required": ["module"]
        }),
        make_executor("cargo_expand", |args| {
            let module = args["module"].as_str().unwrap_or("").to_string();
            let themes = args.get("themes").and_then(|v| v.as_bool()).unwrap_or(false);

            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("expand");
                if themes { cmd.arg("--themes"); }
                cmd.arg(&module);
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

// ── Code Coverage Tool ───────────────────────────────────────────────

fn code_coverage_tool() -> AgentTool {
    AgentTool::new(
        "coverage",
        "Measure Rust code coverage using cargo-llvm-cov. Shows which lines are tested and which are not. Use this after writing tests to verify coverage.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "format": {
                    "type": "string",
                    "enum": ["summary", "json", "html", "lcov"],
                    "description": "Output format: summary (text), json (structured), html (report), lcov (CI)"
                },
                "include_tests": {
                    "type": "boolean",
                    "description": "Include test code in coverage report"
                }
            },
            "required": []
        }),
        make_executor("coverage", |args| {
            let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("summary").to_string();
            let include_tests = args.get("include_tests").and_then(|v| v.as_bool()).unwrap_or(false);

            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("llvm-cov");
                match format.as_str() {
                    "json" => { cmd.arg("--json"); }
                    "html" => { cmd.arg("--html"); }
                    "lcov" => { cmd.arg("--lcov"); }
                    _ => { /* summary is default */ }
                }
                if !include_tests { cmd.arg("--ignore-filename-regex").arg("tests/"); }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

// ── Cargo Mutants Tool ───────────────────────────────────────────────

fn cargo_mutants_tool() -> AgentTool {
    AgentTool::new(
        "mutants",
        "Run mutation testing via cargo-mutants. Injects bugs into source code and checks if existing tests catch them. Surviving mutants = tests not rigorous enough. Requires `cargo-mutants` installed.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Specific function or module to mutation-test (e.g., 'session::save') — omit for all"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds per mutant (default: 60)"
                }
            },
            "required": []
        }),
        make_executor("mutants", |args| {
            let target = args.get("target").and_then(|v| v.as_str()).map(String::from);
            let timeout = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(60);

            Box::pin(async move {
                let mut cmd = tokio::process::Command::new("cargo");
                cmd.arg("mutants");
                cmd.arg("--timeout").arg(timeout.to_string());
                if let Some(ref t) = target { cmd.arg("--function").arg(t); }
                run_cmd_output(&mut cmd).await
            })
        }),
    )
}

/// Run a [`tokio::process::Command`] and capture stdout/stderr.
///
/// Returns `Ok(combined_output)` on success (exit code 0), or
/// `Err(combined_output)` on failure (non-zero exit or spawn error).
async fn run_cmd_output(cmd: &mut tokio::process::Command) -> AgentResult<String> {
    let output = cmd.output().await.map_err(|e| AgentError::ToolExecution {
        tool_name: "(subprocess)".into(),
        message: format!("Failed to spawn command: {e}"),
    })?;

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
        Err(AgentError::ToolExecution {
            tool_name: "(subprocess)".into(),
            message: combined,
        })
    }
}

/// Wrap an async function into a [`ToolExecutor`] (erased closure).
///
/// The inner function receives the parsed JSON arguments and returns
/// `AgentResult<String>` (success or typed error).
/// Errors are passed through directly without wrapping.
fn make_executor<F>(tool_name: &'static str, f: F) -> ToolExecutor
where
    F: Fn(
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = AgentResult<String>> + Send + 'static>,
        > + Send
        + Sync
        + 'static,
{
    let _ = tool_name;
    Arc::new(move |args| Box::pin(f(args)))
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
        assert_eq!(tools.len(), 19, "expected exactly 19 tools");
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
                    err_msg.contains("tool")
                        || err_msg.contains("E9999")
                        || err_msg.contains("rustc"),
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

    // ========================================================================
    // ▼  Data-validation tests (50+ pure static checks, no command execution)
    // ========================================================================

    // -------------------------------------------------------------------
    // Group 1: Global invariants — the full tool set
    // -------------------------------------------------------------------

    #[test]
    fn test_rust_tools_returns_exactly_19() {
        assert_eq!(rust_tools().len(), 19);
    }

    #[test]
    fn test_all_tool_names_are_unique() {
        let tools = rust_tools();
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "duplicate tool names");
    }

    #[test]
    fn test_all_tool_names_match_expected_set() {
        let tools = rust_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        names.sort();
        let expected = vec![
            "cargo",
            "cargo_add",
            "cargo_audit",
            "cargo_bench",
            "cargo_deny",
            "cargo_doc",
            "cargo_expand",
            "cargo_outdated",
            "cargo_test_doc",
            "cargo_udeps",
            "clippy",
            "coverage",
            "grep",
            "mutants",
            "rustc",
            "rustc_explain",
            "rustfmt",
            "rustup",
            "wasm_pack",
        ];
        assert_eq!(names, expected, "tool names do not match expected set");
    }

    // -------------------------------------------------------------------
    // Group 2: Every tool has a non-empty name and description
    // -------------------------------------------------------------------

    #[test]
    fn test_every_tool_has_non_empty_name() {
        for tool in &rust_tools() {
            assert!(
                !tool.name.is_empty(),
                "found tool with empty name at position"
            );
        }
    }

    #[test]
    fn test_every_tool_has_non_empty_description() {
        for tool in &rust_tools() {
            assert!(
                !tool.description.is_empty(),
                "tool '{}' has empty description",
                tool.name
            );
        }
    }

    #[test]
    fn test_every_description_mentions_relevant_context() {
        // Smoke check: each description should be substantial (≥10 chars).
        for tool in &rust_tools() {
            assert!(
                tool.description.len() >= 10,
                "tool '{}' description too short: '{}'",
                tool.name,
                tool.description
            );
        }
    }

    // -------------------------------------------------------------------
    // Group 3: Every schema is valid JSON Schema
    // -------------------------------------------------------------------

    #[test]
    fn test_every_schema_is_type_object() {
        for tool in &rust_tools() {
            let schema = &tool.input_schema;
            assert!(
                schema.is_object(),
                "tool '{}' schema is not a JSON object",
                tool.name
            );
            assert_eq!(
                schema["type"].as_str(),
                Some("object"),
                "tool '{}' schema type is not 'object'",
                tool.name
            );
        }
    }

    #[test]
    fn test_every_schema_has_properties_key() {
        for tool in &rust_tools() {
            let schema = &tool.input_schema;
            assert!(
                schema.get("properties").is_some(),
                "tool '{}' schema is missing 'properties'",
                tool.name
            );
            assert!(
                schema["properties"].is_object(),
                "tool '{}' properties is not an object",
                tool.name
            );
        }
    }

    #[test]
    fn test_every_required_field_references_existing_property() {
        for tool in &rust_tools() {
            let schema = &tool.input_schema;
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                let props = &schema["properties"];
                for field in required {
                    let field_name = field.as_str().unwrap_or("");
                    assert!(
                        props.get(field_name).is_some(),
                        "tool '{}' required field '{}' not found in properties",
                        tool.name,
                        field_name
                    );
                }
            }
        }
    }

    #[test]
    fn test_every_required_field_is_a_string() {
        for tool in &rust_tools() {
            let schema = &tool.input_schema;
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                for field in required {
                    assert!(
                        field.as_str().is_some(),
                        "tool '{}' has non-string entry in 'required'",
                        tool.name
                    );
                }
            }
        }
    }

    #[test]
    fn test_every_property_has_type_field() {
        for tool in &rust_tools() {
            let props = &tool.input_schema["properties"];
            let props_obj = props.as_object().unwrap();
            for (prop_name, prop_schema) in props_obj {
                assert!(
                    prop_schema.get("type").is_some(),
                    "tool '{}' property '{}' has no 'type'",
                    tool.name,
                    prop_name
                );
                let ty = prop_schema["type"].as_str().unwrap();
                assert!(
                    matches!(
                        ty,
                        "string" | "boolean" | "array" | "integer" | "number" | "object"
                    ),
                    "tool '{}' property '{}' has invalid type '{}'",
                    tool.name,
                    prop_name,
                    ty
                );
            }
        }
    }

    #[test]
    fn test_every_property_has_description() {
        for tool in &rust_tools() {
            let props = &tool.input_schema["properties"];
            let props_obj = props.as_object().unwrap();
            for (prop_name, prop_schema) in props_obj {
                let desc = prop_schema.get("description").and_then(|d| d.as_str());
                assert!(
                    desc.is_some() && !desc.unwrap().is_empty(),
                    "tool '{}' property '{}' has no description",
                    tool.name,
                    prop_name
                );
            }
        }
    }

    #[test]
    fn test_every_default_value_is_correct_type() {
        for tool in &rust_tools() {
            let props = &tool.input_schema["properties"];
            let props_obj = props.as_object().unwrap();
            for (prop_name, prop_schema) in props_obj {
                if let Some(default) = prop_schema.get("default") {
                    let ty = prop_schema["type"].as_str().unwrap();
                    match ty {
                        "boolean" => assert!(
                            default.is_boolean(),
                            "tool '{}' prop '{}' default is not boolean",
                            tool.name,
                            prop_name
                        ),
                        "string" => assert!(
                            default.is_string(),
                            "tool '{}' prop '{}' default is not string",
                            tool.name,
                            prop_name
                        ),
                        _ => {}
                    }
                }
            }
        }
    }

    #[test]
    fn test_array_properties_have_items_spec() {
        for tool in &rust_tools() {
            let props = &tool.input_schema["properties"];
            let props_obj = props.as_object().unwrap();
            for (prop_name, prop_schema) in props_obj {
                if prop_schema["type"].as_str() == Some("array") {
                    assert!(
                        prop_schema.get("items").is_some(),
                        "tool '{}' array prop '{}' has no 'items'",
                        tool.name,
                        prop_name
                    );
                    assert_eq!(
                        prop_schema["items"]["type"].as_str(),
                        Some("string"),
                        "tool '{}' array prop '{}' items type is not 'string'",
                        tool.name,
                        prop_name
                    );
                }
            }
        }
    }

    // -------------------------------------------------------------------
    // Group 4: Cargo tool schema — subcommand enum
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_schema_subcommand_is_required() {
        let tool = find_tool("cargo");
        let required: Vec<&str> = tool.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"subcommand"));
        assert_eq!(
            required.len(),
            1,
            "cargo should have exactly one required field"
        );
    }

    #[test]
    fn test_cargo_schema_subcommand_enum_values() {
        let tool = find_tool("cargo");
        let variants = get_enum_values(&tool, "subcommand");
        let expected = vec![
            "add", "bench", "build", "check", "clean", "clippy", "doc", "fix", "fmt", "remove",
            "run", "test", "tree", "update",
        ];
        assert_eq!(variants, expected, "cargo subcommand enum mismatch");
    }

    #[test]
    fn test_cargo_schema_args_is_array_of_strings() {
        let tool = find_tool("cargo");
        assert_eq!(tool.input_schema["properties"]["args"]["type"], "array");
        assert_eq!(
            tool.input_schema["properties"]["args"]["items"]["type"],
            "string"
        );
    }

    #[test]
    fn test_cargo_schema_cwd_is_string() {
        let tool = find_tool("cargo");
        assert_eq!(tool.input_schema["properties"]["cwd"]["type"], "string");
    }

    #[test]
    fn test_cargo_schema_subcommand_is_string_with_enum() {
        let tool = find_tool("cargo");
        let sub = &tool.input_schema["properties"]["subcommand"];
        assert_eq!(sub["type"], "string");
        assert!(sub.get("enum").is_some(), "subcommand missing enum");
        assert!(
            sub.get("description").is_some(),
            "subcommand missing description"
        );
    }

    // -------------------------------------------------------------------
    // Group 5: Rustc tool schema — action enum
    // -------------------------------------------------------------------

    #[test]
    fn test_rustc_schema_action_is_required() {
        let tool = find_tool("rustc");
        let required: Vec<&str> = tool.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"action"));
        assert_eq!(required.len(), 1);
    }

    #[test]
    fn test_rustc_schema_action_enum_values() {
        let tool = find_tool("rustc");
        let variants = get_enum_values(&tool, "action");
        let expected = vec!["check", "edition", "explain", "version"];
        assert_eq!(variants, expected, "rustc action enum mismatch");
    }

    #[test]
    fn test_rustc_schema_target_is_string() {
        let tool = find_tool("rustc");
        assert_eq!(tool.input_schema["properties"]["target"]["type"], "string");
    }

    #[test]
    fn test_rustc_schema_target_has_description() {
        let tool = find_tool("rustc");
        let desc = tool.input_schema["properties"]["target"]["description"]
            .as_str()
            .unwrap();
        assert!(!desc.is_empty());
    }

    // -------------------------------------------------------------------
    // Group 6: Rustfmt tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_rustfmt_has_no_required_fields() {
        let tool = find_tool("rustfmt");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty(),
            "rustfmt should have no required fields"
        );
    }

    #[test]
    fn test_rustfmt_check_is_boolean() {
        let tool = find_tool("rustfmt");
        let check = &tool.input_schema["properties"]["check"];
        assert_eq!(check["type"], "boolean");
        assert_eq!(check["default"], false);
        assert!(check.get("description").is_some());
    }

    #[test]
    fn test_rustfmt_files_is_array_of_strings() {
        let tool = find_tool("rustfmt");
        let files = &tool.input_schema["properties"]["files"];
        assert_eq!(files["type"], "array");
        assert_eq!(files["items"]["type"], "string");
        assert!(files.get("description").is_some());
    }

    // -------------------------------------------------------------------
    // Group 7: Clippy tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_clippy_has_no_required_fields() {
        let tool = find_tool("clippy");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_clippy_deny_warnings_is_boolean() {
        let tool = find_tool("clippy");
        let p = &tool.input_schema["properties"]["deny_warnings"];
        assert_eq!(p["type"], "boolean");
        assert_eq!(p["default"], true);
        assert!(p.get("description").is_some());
    }

    #[test]
    fn test_clippy_allow_is_array_of_strings() {
        let tool = find_tool("clippy");
        let p = &tool.input_schema["properties"]["allow"];
        assert_eq!(p["type"], "array");
        assert_eq!(p["items"]["type"], "string");
        assert!(p.get("description").is_some());
    }

    #[test]
    fn test_clippy_fix_is_boolean() {
        let tool = find_tool("clippy");
        let p = &tool.input_schema["properties"]["fix"];
        assert_eq!(p["type"], "boolean");
        assert_eq!(p["default"], false);
        assert!(p.get("description").is_some());
    }

    // -------------------------------------------------------------------
    // Group 8: Rustup tool schema — action enum
    // -------------------------------------------------------------------

    #[test]
    fn test_rustup_schema_action_is_required() {
        let tool = find_tool("rustup");
        let required: Vec<&str> = tool.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"action"));
        assert_eq!(required.len(), 1);
    }

    #[test]
    fn test_rustup_schema_action_enum_values() {
        let tool = find_tool("rustup");
        let variants = get_enum_values(&tool, "action");
        let expected = vec![
            "check",
            "component",
            "default",
            "show",
            "target",
            "toolchain",
            "update",
        ];
        assert_eq!(variants, expected, "rustup action enum mismatch");
    }

    #[test]
    fn test_rustup_schema_target_is_string() {
        let tool = find_tool("rustup");
        assert_eq!(tool.input_schema["properties"]["target"]["type"], "string");
        assert!(
            tool.input_schema["properties"]["target"]["description"]
                .as_str()
                .unwrap()
                .len()
                > 5
        );
    }

    // -------------------------------------------------------------------
    // Group 9: Cargo audit tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_audit_has_no_required_fields() {
        let tool = find_tool("cargo_audit");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_cargo_audit_fix_is_boolean() {
        let tool = find_tool("cargo_audit");
        let p = &tool.input_schema["properties"]["fix"];
        assert_eq!(p["type"], "boolean");
        assert_eq!(p["default"], false);
    }

    // -------------------------------------------------------------------
    // Group 10: Cargo outdated tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_outdated_has_no_required_fields() {
        let tool = find_tool("cargo_outdated");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_cargo_outdated_workspace_is_boolean() {
        let tool = find_tool("cargo_outdated");
        let p = &tool.input_schema["properties"]["workspace"];
        assert_eq!(p["type"], "boolean");
        assert_eq!(p["default"], true);
    }

    // -------------------------------------------------------------------
    // Group 11: Cargo udeps tool schema — empty properties
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_udeps_schema_has_empty_properties() {
        let tool = find_tool("cargo_udeps");
        let props = tool.input_schema["properties"].as_object().unwrap();
        assert!(
            props.is_empty(),
            "cargo_udeps should have no properties, got: {props:?}"
        );
    }

    #[test]
    fn test_cargo_udeps_has_no_required_field() {
        let tool = find_tool("cargo_udeps");
        assert!(tool.input_schema.get("required").is_none());
    }

    // -------------------------------------------------------------------
    // Group 12: Cargo deny tool schema — check enum
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_deny_has_no_required_fields() {
        let tool = find_tool("cargo_deny");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_cargo_deny_check_is_string_with_enum() {
        let tool = find_tool("cargo_deny");
        let p = &tool.input_schema["properties"]["check"];
        assert_eq!(p["type"], "string");
        let variants = get_enum_values(&tool, "check");
        let expected = vec!["advisories", "bans", "licenses", "sources"];
        assert_eq!(variants, expected, "cargo_deny check enum mismatch");
        assert_eq!(p["default"], "advisories");
    }

    // -------------------------------------------------------------------
    // Group 13: Cargo bench tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_bench_has_no_required_fields() {
        let tool = find_tool("cargo_bench");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_cargo_bench_args_is_array_of_strings() {
        let tool = find_tool("cargo_bench");
        let p = &tool.input_schema["properties"]["args"];
        assert_eq!(p["type"], "array");
        assert_eq!(p["items"]["type"], "string");
    }

    #[test]
    fn test_cargo_bench_cwd_is_string() {
        let tool = find_tool("cargo_bench");
        assert_eq!(tool.input_schema["properties"]["cwd"]["type"], "string");
    }

    // -------------------------------------------------------------------
    // Group 14: Cargo doc tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_doc_has_no_required_fields() {
        let tool = find_tool("cargo_doc");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_cargo_doc_open_is_boolean() {
        let tool = find_tool("cargo_doc");
        let p = &tool.input_schema["properties"]["open"];
        assert_eq!(p["type"], "boolean");
        assert_eq!(p["default"], false);
    }

    #[test]
    fn test_cargo_doc_args_is_array_of_strings() {
        let tool = find_tool("cargo_doc");
        let p = &tool.input_schema["properties"]["args"];
        assert_eq!(p["type"], "array");
        assert_eq!(p["items"]["type"], "string");
    }

    #[test]
    fn test_cargo_doc_cwd_is_string() {
        let tool = find_tool("cargo_doc");
        assert_eq!(tool.input_schema["properties"]["cwd"]["type"], "string");
    }

    // -------------------------------------------------------------------
    // Group 15: Cargo test_doc tool schema
    // -------------------------------------------------------------------

    #[test]
    fn test_cargo_test_doc_has_no_required_fields() {
        let tool = find_tool("cargo_test_doc");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_cargo_test_doc_package_is_string() {
        let tool = find_tool("cargo_test_doc");
        assert_eq!(tool.input_schema["properties"]["package"]["type"], "string");
    }

    #[test]
    fn test_cargo_test_doc_args_is_array_of_strings() {
        let tool = find_tool("cargo_test_doc");
        let p = &tool.input_schema["properties"]["args"];
        assert_eq!(p["type"], "array");
        assert_eq!(p["items"]["type"], "string");
    }

    #[test]
    fn test_cargo_test_doc_cwd_is_string() {
        let tool = find_tool("cargo_test_doc");
        assert_eq!(tool.input_schema["properties"]["cwd"]["type"], "string");
    }

    // -------------------------------------------------------------------
    // Group 16: Wasm-pack tool schema — subcommand & target enums
    // -------------------------------------------------------------------

    #[test]
    fn test_wasm_pack_has_no_required_fields() {
        let tool = find_tool("wasm_pack");
        assert!(
            tool.input_schema.get("required").is_none()
                || tool.input_schema["required"].as_array().unwrap().is_empty()
        );
    }

    #[test]
    fn test_wasm_pack_subcommand_enum_values() {
        let tool = find_tool("wasm_pack");
        let variants = get_enum_values(&tool, "subcommand");
        let expected = vec!["build", "new", "pack", "test"];
        assert_eq!(variants, expected, "wasm_pack subcommand enum mismatch");
    }

    #[test]
    fn test_wasm_pack_subcommand_is_string_with_default() {
        let tool = find_tool("wasm_pack");
        let p = &tool.input_schema["properties"]["subcommand"];
        assert_eq!(p["type"], "string");
        assert_eq!(p["default"], "build");
        assert!(p.get("enum").is_some());
    }

    #[test]
    fn test_wasm_pack_target_enum_values() {
        let tool = find_tool("wasm_pack");
        let variants = get_enum_values(&tool, "target");
        let expected = vec!["bundler", "deno", "nodejs", "web"];
        assert_eq!(variants, expected, "wasm_pack target enum mismatch");
    }

    #[test]
    fn test_wasm_pack_target_is_string() {
        let tool = find_tool("wasm_pack");
        let p = &tool.input_schema["properties"]["target"];
        assert_eq!(p["type"], "string");
        assert!(p.get("enum").is_some());
    }

    #[test]
    fn test_wasm_pack_args_is_array_of_strings() {
        let tool = find_tool("wasm_pack");
        let p = &tool.input_schema["properties"]["args"];
        assert_eq!(p["type"], "array");
        assert_eq!(p["items"]["type"], "string");
    }

    #[test]
    fn test_wasm_pack_cwd_is_string() {
        let tool = find_tool("wasm_pack");
        assert_eq!(tool.input_schema["properties"]["cwd"]["type"], "string");
    }

    // -------------------------------------------------------------------
    // Group 17: Rustc_explain tool schema — error_code
    // -------------------------------------------------------------------

    #[test]
    fn test_rustc_explain_error_code_is_required() {
        let tool = find_tool("rustc_explain");
        let required: Vec<&str> = tool.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(
            required.contains(&"error_code"),
            "error_code must be required"
        );
        assert_eq!(required.len(), 1, "only error_code should be required");
    }

    #[test]
    fn test_rustc_explain_error_code_is_string() {
        let tool = find_tool("rustc_explain");
        let p = &tool.input_schema["properties"]["error_code"];
        assert_eq!(p["type"], "string", "error_code type must be string");
        let desc = p["description"].as_str().unwrap();
        assert!(!desc.is_empty(), "error_code must have description");
        assert!(
            desc.contains("E0502") || desc.contains("error code"),
            "description should mention error codes"
        );
    }

    #[test]
    fn test_rustc_explain_only_has_error_code_property() {
        let tool = find_tool("rustc_explain");
        let props = tool.input_schema["properties"].as_object().unwrap();
        let keys: Vec<&str> = props.keys().map(|k| k.as_str()).collect();
        assert_eq!(
            keys,
            vec!["error_code"],
            "rustc_explain should have exactly one property"
        );
    }

    // -------------------------------------------------------------------
    // Group 18: Edge case — no tool has a property with a null type
    // -------------------------------------------------------------------

    /// Verify no tool property has a `null` value anywhere in its schema.
    #[test]
    fn test_no_null_values_in_any_schema() {
        fn has_null(val: &serde_json::Value) -> bool {
            match val {
                serde_json::Value::Null => true,
                serde_json::Value::Object(m) => m.values().any(has_null),
                serde_json::Value::Array(arr) => arr.iter().any(has_null),
                _ => false,
            }
        }
        for tool in &rust_tools() {
            assert!(
                !has_null(&tool.input_schema),
                "tool '{}' schema contains null value",
                tool.name
            );
        }
    }

    /// Every object-typed property value should be an object.
    #[test]
    fn test_no_primitive_properties_field() {
        for tool in &rust_tools() {
            let props = tool.input_schema["properties"].as_object().unwrap();
            for (name, prop) in props {
                assert!(
                    prop.is_object(),
                    "tool '{}' property '{}' is not an object (got: {})",
                    tool.name,
                    name,
                    prop
                );
            }
        }
    }

    /// All 14 tool names have expected prefix patterns.
    #[test]
    fn test_tool_name_naming_convention() {
        for tool in &rust_tools() {
            let name = &tool.name;
            // Tools either start with "cargo", "rust", "wasm", or "clippy"
            let valid_prefix = name.starts_with("cargo")
                || name.starts_with("rust")
                || name == "clippy"
                || name == "wasm_pack"
                || name == "grep"
                || name == "coverage"
                || name == "mutants";
            assert!(
                valid_prefix,
                "tool name '{}' does not follow naming convention",
                name
            );
        }
    }

    /// Each tool description has a minimum length of 20 characters.
    #[test]
    fn test_all_descriptions_minimum_length() {
        for tool in &rust_tools() {
            assert!(
                tool.description.len() >= 20,
                "tool '{}' description too short ({} chars): '{}'",
                tool.name,
                tool.description.len(),
                tool.description
            );
        }
    }

    /// Every enum-valued property has at least 2 options.
    #[test]
    fn test_all_enums_have_at_least_two_options() {
        for tool in &rust_tools() {
            let props = tool.input_schema["properties"].as_object().unwrap();
            for (prop_name, prop_schema) in props {
                if let Some(enum_arr) = prop_schema.get("enum").and_then(|e| e.as_array()) {
                    assert!(
                        enum_arr.len() >= 2,
                        "tool '{}' prop '{}' enum has only {} option(s)",
                        tool.name,
                        prop_name,
                        enum_arr.len()
                    );
                }
            }
        }
    }

    /// The `cargo` tool description mentions common subcommands.
    #[test]
    fn test_cargo_description_mentions_build() {
        let tool = find_tool("cargo");
        assert!(
            tool.description.contains("build")
                || tool.description.contains("test")
                || tool.description.contains("check"),
            "cargo description should mention key subcommands"
        );
    }

    /// The `rustc` tool description mentions compilation.
    #[test]
    fn test_rustc_description_mentions_compiler() {
        let tool = find_tool("rustc");
        assert!(
            tool.description.contains("syntax")
                || tool.description.contains("compiler")
                || tool.description.contains("compile"),
            "rustc description should reference compilation"
        );
    }

    /// The `clippy` tool description mentions linting.
    #[test]
    fn test_clippy_description_mentions_lint() {
        let tool = find_tool("clippy");
        assert!(
            tool.description.contains("lint"),
            "clippy description should mention linting"
        );
    }

    /// The `rustup` tool description mentions toolchain.
    #[test]
    fn test_rustup_description_mentions_toolchain() {
        let tool = find_tool("rustup");
        assert!(
            tool.description.contains("toolchain"),
            "rustup description should mention toolchain"
        );
    }

    /// The `cargo_audit` description mentions security.
    #[test]
    fn test_cargo_audit_description_mentions_security() {
        let tool = find_tool("cargo_audit");
        assert!(
            tool.description.contains("security") || tool.description.contains("vulnerabilit"),
            "cargo_audit description should mention security"
        );
    }

    /// No tool name is longer than 20 characters.
    #[test]
    fn test_all_tool_names_reasonable_length() {
        for tool in &rust_tools() {
            assert!(
                tool.name.len() <= 20,
                "tool name '{}' is too long ({} chars)",
                tool.name,
                tool.name.len()
            );
        }
    }

    // ========================================================================
    // Helper: find a tool by name (panics with clear message if missing)
    // ========================================================================

    fn find_tool(name: &str) -> AgentTool {
        let tools = rust_tools();
        tools
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("tool '{}' not found in rust_tools()", name))
    }

    /// Extract the sorted `"enum"` values from a tool's property as `Vec<&str>`.
    fn get_enum_values<'a>(tool: &'a AgentTool, property: &str) -> Vec<&'a str> {
        let mut variants: Vec<&str> = tool.input_schema["properties"][property]["enum"]
            .as_array()
            .unwrap_or_else(|| panic!("tool '{}' property '{}' missing enum", tool.name, property))
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        variants.sort();
        variants
    }
}
