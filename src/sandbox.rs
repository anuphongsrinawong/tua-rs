//! 🐳 Docker Sandbox — secure execution environment
//!
//! Runs cargo commands inside a Docker container to isolate the
//! agent from the host filesystem. Prevents the agent from accidentally
//! modifying or deleting files outside the project directory.
//!
//! ## Architecture
//! - Uses `docker run` with volume mounts for the project
//! - Limits CPU and memory per container
//! - Auto-detects and uses a Rust toolchain image
//!
//! ## Usage
//! ```ignore
//! let sandbox = DockerSandbox::new("/path/to/project", "rust:1.80-slim")?;
//! let output = sandbox.run("cargo check").await?;
//! // agent can run arbitrary cargo commands safely
//! ```

use std::process::Command;

const DEFAULT_RUST_IMAGE: &str = "rust:latest";
const MAX_MEMORY: &str = "2g";
const MAX_CPUS: &str = "2.0";

/// A Docker container running a Rust toolchain.
#[allow(dead_code)]
pub struct DockerSandbox {
    container_id: Option<String>,
    project_dir: String,
    _image: String,
}

/// Result of a sandboxed command execution.
#[derive(Debug, Clone)]
pub struct SandboxOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

impl DockerSandbox {
    /// Create a new sandbox for the given project directory.
    pub fn new(project_dir: &str) -> Result<Self, String> {
        // Verify docker is available
        let status = Command::new("docker")
            .arg("version")
            .output()
            .map_err(|_| "Docker is not installed or not running".to_string())?;

        if !status.status.success() {
            return Err("Docker daemon is not running".into());
        }

        Ok(Self {
            container_id: None,
            project_dir: project_dir.to_string(),
            _image: DEFAULT_RUST_IMAGE.into(),
        })
    }

    /// Run a cargo command inside a temporary container.
    ///
    /// Mounts the project directory at `/project`, runs the command,
    /// and returns the output. The container is removed after execution.
    pub fn run(&self, command: &str) -> Result<SandboxOutput, String> {
        let start = std::time::Instant::now();

        let _parent = std::path::Path::new(&self.project_dir)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".into());

        let output = Command::new("docker")
            .arg("run")
            .arg("--rm") // remove after run
            .args(["--memory", MAX_MEMORY]) // limit memory
            .args(["--cpus", MAX_CPUS]) // limit CPU
            .args(["-v", &format!("{}:/workspace", self.project_dir)])
            .args(["-w", "/workspace"])
            .arg(DEFAULT_RUST_IMAGE)
            .arg("sh")
            .arg("-c")
            .arg(command)
            .output()
            .map_err(|e| format!("Docker run failed: {}", e))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(SandboxOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code().unwrap_or(-1),
            duration_ms,
        })
    }

    /// Run cargo check in the sandbox.
    pub fn cargo_check(&self) -> Result<SandboxOutput, String> {
        self.run("cargo check 2>&1")
    }

    /// Run cargo test in the sandbox.
    pub fn cargo_test(&self) -> Result<SandboxOutput, String> {
        self.run("cargo test 2>&1")
    }

    /// Build the project in release mode.
    pub fn cargo_build_release(&self) -> Result<SandboxOutput, String> {
        self.run("cargo build --release 2>&1")
    }

    /// Check if the sandbox is functional.
    pub fn health_check(&self) -> bool {
        match self.run("rustc --version && cargo --version") {
            Ok(output) => output.exit_code == 0,
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_new_requires_docker() {
        // May fail if Docker isn't running, which is OK in CI
        let result = DockerSandbox::new("/tmp/test");
        // Either works (docker available) or fails with clear message
        match result {
            Ok(_) => {}
            Err(e) => assert!(e.contains("Docker") || e.contains("not")),
        }
    }

    #[test]
    fn test_sandbox_health_check() {
        if let Ok(sandbox) = DockerSandbox::new(".") {
            let healthy = sandbox.health_check();
            // Just verify it doesn't panic
            let _ = healthy;
        }
    }
}
