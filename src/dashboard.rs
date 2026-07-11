//! Web dashboard for Tua Agent RS — project health at a glance.
//!
//! Serves an HTML dashboard at `/` with dark theme, plus JSON API endpoints
//! at `/api/health` and `/api/status`. Uses [`tokio::process::Command`] to
//! gather live project metadata (git info, LOC, build status, clippy stats).

use axum::{
    extract::State,
    response::{Html, Json},
    routing::get,
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

// ---------------------------------------------------------------------------
// Dashboard state
// ---------------------------------------------------------------------------

/// Holds cached project metrics, refreshed on each request.
#[derive(Debug, Clone)]
pub struct DashboardState {
    inner: Arc<Mutex<DashInner>>,
}

#[derive(Debug, Clone)]
struct DashInner {
    /// Project root directory (resolved from `CARGO_MANIFEST_DIR` or cwd).
    project_dir: String,
    #[allow(dead_code)]
    /// Timestamp of last refresh.
    refreshed_at: Instant,
}

impl DashboardState {
    /// Create a new dashboard state, probing the current project root.
    ///
    /// Uses the `CARGO_MANIFEST_DIR` env var set by Cargo, falling back to
    /// the current working directory.
    pub fn new() -> Self {
        let dir = std::env::var("CARGO_MANIFEST_DIR")
            .or_else(|_| std::env::current_dir().map(|p| p.to_string_lossy().to_string()))
            .unwrap_or_else(|_| ".".to_string());

        Self {
            inner: Arc::new(Mutex::new(DashInner {
                project_dir: dir,
                refreshed_at: Instant::now(),
            })),
        }
    }
}

impl Default for DashboardState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// Response for `GET /api/health`.
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
}

/// Response for `GET /api/status`.
#[derive(Debug, Serialize)]
struct StatusResponse {
    project: ProjectInfo,
    build: BuildStatus,
    quality: QualityMetrics,
    tools: Vec<ToolInfo>,
}

#[derive(Debug, Serialize)]
struct ProjectInfo {
    name: String,
    version: String,
    profile: String,
    rust_version: String,
    git_branch: String,
    git_commit: String,
    git_date: String,
    lines_of_code: u64,
    workspace_members: Vec<String>,
}

#[derive(Debug, Serialize)]
struct BuildStatus {
    last_check: String,
    success: bool,
    error_count: u32,
    warning_count: u32,
}

#[derive(Debug, Serialize)]
struct QualityMetrics {
    clippy_warnings: u32,
    clippy_errors: u32,
    total_files: u32,
    test_count: u32,
    doc_test_count: u32,
}

#[derive(Debug, Serialize)]
struct ToolInfo {
    name: String,
    description: String,
}

// ---------------------------------------------------------------------------
// HTML template
// ---------------------------------------------------------------------------

/// Render the full HTML dashboard page with inline dark-theme styles.
fn render_html(project: &ProjectInfo, build: &BuildStatus, quality: &QualityMetrics) -> String {
    let build_badge_class = if build.success { "success" } else { "failure" };
    let build_badge_text = if build.success {
        "✅ PASS"
    } else {
        "❌ FAIL"
    };

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>🦀 Tua Agent — Dashboard</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ background: #0d1117; color: #c9d1d9; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; padding: 2rem; }}
  .container {{ max-width: 960px; margin: 0 auto; }}
  h1 {{ font-size: 1.75rem; margin-bottom: 0.5rem; }}
  h1 span {{ color: #58a6ff; }}
  .subtitle {{ color: #8b949e; margin-bottom: 2rem; }}
  .grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; margin-bottom: 2rem; }}
  @media (max-width: 600px) {{ .grid {{ grid-template-columns: 1fr; }} }}
  .card {{ background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 1.25rem; }}
  .card h2 {{ font-size: 1rem; color: #58a6ff; margin-bottom: 0.75rem; border-bottom: 1px solid #21262d; padding-bottom: 0.5rem; }}
  .card table {{ width: 100%; border-collapse: collapse; }}
  .card td {{ padding: 0.35rem 0; font-size: 0.875rem; }}
  .card td:first-child {{ color: #8b949e; width: 40%; }}
  .card td:last-child {{ color: #c9d1d9; font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace; }}
  .badge {{ display: inline-block; padding: 0.2rem 0.6rem; border-radius: 12px; font-size: 0.75rem; font-weight: 600; }}
  .badge.success {{ background: #1b3a1f; color: #3fb950; border: 1px solid #2ea043; }}
  .badge.failure {{ background: #3d1a1a; color: #f85149; border: 1px solid #da3633; }}
  .full-width {{ grid-column: 1 / -1; }}
  .tools-list {{ display: flex; flex-wrap: wrap; gap: 0.5rem; margin-top: 0.5rem; }}
  .tool-tag {{ background: #1f2937; color: #79c0ff; padding: 0.2rem 0.6rem; border-radius: 4px; font-size: 0.8rem; border: 1px solid #30363d; }}
  .footer {{ margin-top: 2rem; text-align: center; color: #484f58; font-size: 0.8rem; }}
  .footer a {{ color: #58a6ff; text-decoration: none; }}
  .footer a:hover {{ text-decoration: underline; }}
  .loc-bar {{ background: #21262d; height: 12px; border-radius: 6px; overflow: hidden; margin-top: 0.5rem; }}
  .loc-bar-fill {{ height: 100%; background: linear-gradient(90deg, #58a6ff, #3fb950); border-radius: 6px; width: 0%; }}
  .api-endpoints {{ margin-top: 1rem; }}
  .api-endpoints code {{ background: #21262d; padding: 0.15rem 0.4rem; border-radius: 3px; font-size: 0.8rem; color: #79c0ff; }}
  .metric-value {{ font-size: 1.5rem; font-weight: 700; color: #f0f6fc; }}
  .metric-label {{ font-size: 0.75rem; color: #8b949e; }}
</style>
</head>
<body>
<div class="container">
  <h1>🦀 <span>Tua Agent</span> Dashboard</h1>
  <p class="subtitle">Project health &amp; quality metrics at a glance</p>

  <div class="grid">
    <!-- Project Info -->
    <div class="card">
      <h2>📦 Project</h2>
      <table>
        <tr><td>Name</td><td>{name}</td></tr>
        <tr><td>Version</td><td>{version}</td></tr>
        <tr><td>Profile</td><td>{profile}</td></tr>
        <tr><td>Rust</td><td>{rust_version}</td></tr>
        <tr><td>Branch</td><td>{git_branch}</td></tr>
        <tr><td>Commit</td><td><code>{git_commit_short}</code></td></tr>
        <tr><td>Date</td><td>{git_date}</td></tr>
        <tr><td>Members</td><td>{workspace_members}</td></tr>
      </table>
    </div>

    <!-- Build Status -->
    <div class="card">
      <h2>🔨 Build</h2>
      <div style="margin-bottom: 1rem;">
        <span class="badge {build_badge_class}">{build_badge_text}</span>
        <span style="color: #8b949e; font-size: 0.8rem; margin-left: 0.5rem;">{last_check}</span>
      </div>
      <table>
        <tr><td>Errors</td><td>{build_errors}</td></tr>
        <tr><td>Warnings</td><td>{build_warnings}</td></tr>
      </table>
    </div>

    <!-- Lines of Code -->
    <div class="card">
      <h2>📊 Lines of Code</h2>
      <div class="metric-value">{loc}</div>
      <div class="metric-label">total lines across {total_files} source files</div>
      <div class="loc-bar"><div class="loc-bar-fill" style="width: {loc_pct}%;"></div></div>
    </div>

    <!-- Quality -->
    <div class="card">
      <h2>🔍 Quality</h2>
      <table>
        <tr><td>Clippy Warnings</td><td>{clippy_warnings}</td></tr>
        <tr><td>Clippy Errors</td><td>{clippy_errors}</td></tr>
        <tr><td>Unit Tests</td><td>{test_count}</td></tr>
        <tr><td>Doc Tests</td><td>{doc_test_count}</td></tr>
      </table>
    </div>

    <!-- Tools (full width) -->
    <div class="card full-width">
      <h2>🔧 Tools ({tool_count})</h2>
      <div class="tools-list">
        {tools_html}
      </div>
      <div class="api-endpoints">
        <h2 style="margin-top: 1rem;">📡 API Endpoints</h2>
        <table>
          <tr><td><code>GET /api/health</code></td><td style="color: #3fb950;">✓</td></tr>
          <tr><td><code>GET /api/status</code></td><td style="color: #3fb950;">✓</td></tr>
        </table>
      </div>
    </div>
  </div>

  <div class="footer">
    🦀 Tua Agent RS v{version} &mdash; <a href="https://rust-lang.org" target="_blank">Built with Rust</a>
  </div>
</div>
</body>
</html>"##,
        name = project.name,
        version = project.version,
        profile = project.profile,
        rust_version = project.rust_version,
        git_branch = project.git_branch,
        git_commit_short = &project.git_commit[..project.git_commit.len().min(7)],
        git_date = project.git_date,
        workspace_members = project.workspace_members.join(", "),
        build_badge_class = build_badge_class,
        build_badge_text = build_badge_text,
        last_check = build.last_check,
        build_errors = build.error_count,
        build_warnings = build.warning_count,
        loc = project.lines_of_code,
        total_files = quality.total_files,
        loc_pct = ((quality.total_files as f64 / 5000.0).min(100.0) as u64),
        clippy_warnings = quality.clippy_warnings,
        clippy_errors = quality.clippy_errors,
        test_count = quality.test_count,
        doc_test_count = quality.doc_test_count,
        tool_count = crate::tools::rust_tools().len(),
        tools_html = render_tool_tags(),
    )
}

/// Render inline HTML for tool tags.
fn render_tool_tags() -> String {
    crate::tools::rust_tools()
        .iter()
        .map(|t| format!(r#"<span class="tool-tag">{}</span>"#, t.name))
        .collect::<Vec<_>>()
        .join("\n        ")
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/health` — simple liveness check.
async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

/// `GET /api/status` — full project status JSON.
async fn status_handler(State(state): State<DashboardState>) -> Json<StatusResponse> {
    let (project, build, quality) = gather_metrics(&state).await;
    let tools = crate::tools::rust_tools()
        .iter()
        .map(|t| ToolInfo {
            name: t.name.clone(),
            description: t.description.clone(),
        })
        .collect();

    Json(StatusResponse {
        project,
        build,
        quality,
        tools,
    })
}

/// `GET /` — HTML dashboard page.
async fn index_handler(State(state): State<DashboardState>) -> Html<String> {
    let (project, build, quality) = gather_metrics(&state).await;
    Html(render_html(&project, &build, &quality))
}

// ---------------------------------------------------------------------------
// Metrics gathering
// ---------------------------------------------------------------------------

/// Gather all project metrics by running shell commands.
async fn gather_metrics(state: &DashboardState) -> (ProjectInfo, BuildStatus, QualityMetrics) {
    let dir = {
        let inner = state.inner.lock().await;
        inner.project_dir.clone()
    };

    // Run git commands concurrently
    let (git_branch, git_commit, git_date) = tokio::join!(
        run_cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"], &dir),
        run_cmd("git", &["rev-parse", "HEAD"], &dir),
        run_cmd("git", &["log", "-1", "--format=%ai"], &dir),
    );

    let git_branch = git_branch.unwrap_or_else(|_| "unknown".to_string());
    let git_commit = git_commit.unwrap_or_else(|_| "unknown".to_string());
    let git_date = git_date.unwrap_or_else(|_| "unknown".to_string());

    // Rust version
    let rust_version = run_cmd("rustc", &["--version"], &dir)
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    // Lines of code — count .rs files via find + wc
    let loc = run_cmd(
        "sh",
        &["-c", "find src/ -name '*.rs' -exec cat {} + | wc -l"],
        &dir,
    )
    .await
    .ok()
    .and_then(|s| s.trim().parse::<u64>().ok())
    .unwrap_or(0);

    // Source file count
    let total_files = run_cmd("sh", &["-c", "find src/ -name '*.rs' | wc -l"], &dir)
        .await
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // Clippy: run with --message-format=json to get structured counts
    let clippy_output = run_cmd(
        "sh",
        &["-c", "cargo clippy --message-format=json 2>&1 | tail -20"],
        &dir,
    )
    .await;

    let (clippy_warnings, clippy_errors) = if let Ok(out) = clippy_output {
        let warnings = out.matches("warning:").count() as u32;
        let errors = out.matches("error:").count() as u32;
        (warnings, errors)
    } else {
        (0, 0)
    };

    // Cargo metadata JSON
    let cargo_meta = run_cmd(
        "cargo",
        &["metadata", "--no-deps", "--format-version", "1"],
        &dir,
    )
    .await;

    let (name, version, workspace_members) = if let Ok(ref meta) = cargo_meta {
        (
            meta.split("\"name\":\"")
                .nth(1)
                .and_then(|s| s.split('\"').next())
                .unwrap_or("tua-rs")
                .to_string(),
            meta.split("\"version\":\"")
                .nth(1)
                .and_then(|s| s.split('\"').next())
                .unwrap_or("0.1.0")
                .to_string(),
            // Extract workspace members
            meta.split("\"workspace_members\":[")
                .nth(1)
                .map(|s| {
                    s.split(']')
                        .next()
                        .unwrap_or("")
                        .split(',')
                        .filter_map(|pkg| {
                            // package-id looks like "path+file:///... pkg-name version (source)"
                            // We extract a simple name from the package spec
                            pkg.split_whitespace().nth(1).map(|n| n.to_string())
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
    } else {
        ("tua-rs".to_string(), "0.1.0".to_string(), vec![])
    };

    // Workspace members fallback: if empty and we have a single package
    let workspace_members = if workspace_members.is_empty() {
        vec![name.clone()]
    } else {
        workspace_members
    };

    // Build status: simulate a cargo check (quick)
    let build_check = run_cmd("cargo", &["check", "--message-format=json"], &dir).await;
    let (build_success, build_errors, build_warnings, last_check) = match build_check {
        Ok(out) => {
            let warns = out.matches("warning:").count() as u32;
            let errs = out.matches("error:").count() as u32;
            let success = errs == 0;
            let time = chrono_formatted();
            (success, errs, warns, time)
        }
        Err(_) => (false, 1, 0, chrono_formatted()),
    };

    // Test counts
    let test_count = run_cmd(
        "sh",
        &[
            "-c",
            "grep -r '#\\[tokio::test\\]\\|#\\[test\\]' src/ tests/ 2>/dev/null | wc -l",
        ],
        &dir,
    )
    .await
    .ok()
    .and_then(|s| s.trim().parse::<u32>().ok())
    .unwrap_or(0);

    let doc_test_count = run_cmd(
        "sh",
        &["-c", "grep -r '/// ```' src/ 2>/dev/null | wc -l"],
        &dir,
    )
    .await
    .ok()
    .and_then(|s| s.trim().parse::<u32>().ok())
    .unwrap_or(0);

    // Current profile — pick the active one
    let profile = crate::profiles::ALL_PROFILES
        .first()
        .map(|p| format!("{} {}", p.emoji, p.name))
        .unwrap_or_else(|| "🚀 rustacean".to_string());

    let project = ProjectInfo {
        name,
        version,
        profile,
        rust_version,
        git_branch,
        git_commit,
        git_date,
        lines_of_code: loc,
        workspace_members,
    };

    let build = BuildStatus {
        last_check,
        success: build_success,
        error_count: build_errors,
        warning_count: build_warnings,
    };

    let quality = QualityMetrics {
        clippy_warnings,
        clippy_errors,
        total_files,
        test_count,
        doc_test_count,
    };

    (project, build, quality)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Run a shell command and return stdout as a string on success.
async fn run_cmd(program: &str, args: &[&str], cwd: &str) -> Result<String, String> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| format!("failed to spawn {program}: {e}"))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(stderr)
    }
}

/// Get a human-readable timestamp string.
fn chrono_formatted() -> String {
    // Simple fallback that doesn't require chrono crate
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Format as a simple ISO-like string
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{days}d {hours:02}:{minutes:02}:{seconds:02} UTC")
}

// ---------------------------------------------------------------------------
// Router factory
// ---------------------------------------------------------------------------

/// Build the dashboard [`Router`] with all routes.
///
/// Call this from `main.rs` to mount the dashboard on a prefix, or
/// use it standalone.
///
/// # Example
///
/// ```ignore
/// use dashboard::dashboard_router;
///
/// let app = dashboard_router();
/// let listener = tokio::net::TcpListener::bind("0.0.0.0:8765").await.unwrap();
/// axum::serve(listener, app).await.unwrap();
/// ```
pub fn dashboard_router() -> Router {
    let state = DashboardState::new();

    Router::new()
        .route("/", get(index_handler))
        .route("/api/health", get(health_handler))
        .route("/api/status", get(status_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    /// The health endpoint returns `{"status":"ok"}`.
    #[tokio::test]
    async fn test_health_endpoint() {
        let app = dashboard_router();

        // Use axum's test helpers via tower
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{}/api/health", addr))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(body["status"], "ok");
    }

    /// The HTML index page returns 200 and contains expected content.
    #[tokio::test]
    async fn test_index_page_returns_html() {
        let app = dashboard_router();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{}/", addr))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let html = resp.text().await.unwrap();
        assert!(html.contains("Tua Agent"), "HTML should mention Tua Agent");
        assert!(
            html.contains("#0d1117"),
            "HTML should include dark theme bg"
        );
        assert!(html.contains("Dashboard"), "HTML should contain dashboard");
    }

    /// Status endpoint returns JSON with expected fields.
    #[tokio::test]
    async fn test_status_endpoint() {
        let app = dashboard_router();

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{}/api/status", addr))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = resp.json().await.unwrap();
        assert!(
            body.get("project").is_some(),
            "status should have project field"
        );
        assert!(
            body.get("build").is_some(),
            "status should have build field"
        );
        assert!(
            body.get("quality").is_some(),
            "status should have quality field"
        );
        assert!(
            body.get("tools").is_some(),
            "status should have tools field"
        );
    }
}
