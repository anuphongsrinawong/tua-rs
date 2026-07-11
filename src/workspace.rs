//! 🦀 Workspace discovery via `cargo metadata`.
//!
//! This module provides [`detect_workspace`] to inspect a Rust project's
//! workspace structure by shelling out to `cargo metadata` and parsing
//! the JSON output.

use serde::Deserialize;

use std::path::Path;
use std::process::Command as StdCommand;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur while detecting a workspace.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// The `cargo` binary could not be spawned or the command failed.
    #[error("failed to run `cargo metadata` in `{dir}`: {source}")]
    CommandFailed {
        /// The directory where the command was run.
        dir: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// The `cargo metadata` process exited with a non-zero status.
    #[error("`cargo metadata` failed in `{dir}`:\n{stderr}")]
    NonZeroExit {
        /// The directory where the command was run.
        dir: String,
        /// Combined stderr from the process.
        stderr: String,
    },

    /// The stdout of `cargo metadata` could not be decoded as UTF-8.
    #[error("`cargo metadata` output was not valid UTF-8 in `{dir}`")]
    InvalidUtf8 {
        /// The directory where the command was run.
        dir: String,
    },

    /// The JSON output could not be parsed (version mismatch, malformed,
    /// etc.).
    #[error("failed to parse `cargo metadata` JSON in `{dir}`: {source}")]
    ParseFailed {
        /// The directory where the command was run.
        dir: String,
        /// The underlying parse error.
        #[source]
        source: serde_json::Error,
    },
}

/// Convenience alias.
pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A Rust workspace, consisting of a root directory and its member crates.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    /// Absolute path to the workspace root.
    pub root: String,
    /// All workspace members (including the root package in a single-crate
    /// project).
    pub members: Vec<CrateInfo>,
}

/// Information about a single crate within a workspace.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrateInfo {
    /// Crate name as declared in `Cargo.toml`.
    pub name: String,
    /// Directory containing the crate's `Cargo.toml`.
    pub path: String,
    /// Names of direct (non-dev, non-build) dependencies.
    pub dependencies: Vec<String>,
}

// ---------------------------------------------------------------------------
// Internal JSON models for `cargo metadata --format-version=1`
// ---------------------------------------------------------------------------

/// Top-level structure returned by `cargo metadata`.
#[derive(Debug, Deserialize)]
struct CargoMetadata {
    /// All packages in the workspace.
    packages: Vec<Package>,
    /// Absolute path to the workspace root.
    workspace_root: String,
}

/// A single package entry.
#[derive(Debug, Deserialize)]
struct Package {
    /// Package name.
    name: String,
    /// Path to the `Cargo.toml` manifest.
    manifest_path: String,
    /// Direct dependencies declared by this package.
    dependencies: Vec<Dependency>,
}

/// A dependency entry within a package.
#[derive(Debug, Deserialize)]
struct Dependency {
    /// Name of the dependency.
    name: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Detect the workspace structure by running `cargo metadata` in the
/// given directory.
///
/// The `path` argument should point to a directory containing a
/// `Cargo.toml` (the workspace root or a member crate).  `cargo
/// metadata` will automatically resolve the full workspace.
///
/// # Errors
///
/// Returns an error if:
///
/// * `cargo` is not installed or cannot be run.
/// * `cargo metadata` exits with a non-zero status (e.g. the directory
///   is not a Rust project).
/// * The metadata output is not valid UTF-8 or valid JSON.
///
/// # Example
///
/// ```no_run
/// use tua_rs::workspace::{detect_workspace, WorkspaceError};
///
/// match detect_workspace(".") {
///     Ok(ws) => println!("root: {}, members: {}", ws.root, ws.members.len()),
///     Err(e) => eprintln!("{e}"),
/// }
/// ```
pub fn detect_workspace(path: &str) -> WorkspaceResult<Workspace> {
    let dir = path.to_string();

    // --- Run `cargo metadata` ---
    let output = StdCommand::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(path)
        .output()
        .map_err(|source| WorkspaceError::CommandFailed {
            dir: dir.clone(),
            source,
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(WorkspaceError::NonZeroExit {
            dir: dir.clone(),
            stderr,
        });
    }

    // --- Decode stdout ---
    let stdout =
        std::str::from_utf8(&output.stdout)
            .map_err(|_| WorkspaceError::InvalidUtf8 { dir: dir.clone() })?;

    // --- Parse JSON ---
    let meta: CargoMetadata = serde_json::from_str(stdout).map_err(|source| {
        WorkspaceError::ParseFailed {
            dir: dir.clone(),
            source,
        }
    })?;

    // --- Deduplicate packages by manifest_path (workspaces can have the
    // same crate in multiple members; cargo metadata lists packages, which
    // are unique by (name, version, source), but we want unique crates by
    // manifest_path).
    let mut seen = std::collections::HashSet::new();
    let mut members: Vec<CrateInfo> = Vec::new();

    for pkg in meta.packages {
        let manifest_dir = Path::new(&pkg.manifest_path)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| pkg.manifest_path.clone());

        // Skip duplicates
        if !seen.insert(manifest_dir.clone()) {
            continue;
        }

        let deps: Vec<String> = pkg
            .dependencies
            .into_iter()
            .map(|d| d.name)
            .collect();

        members.push(CrateInfo {
            name: pkg.name,
            path: manifest_dir,
            dependencies: deps,
        });
    }

    let root = meta.workspace_root;

    Ok(Workspace { root, members })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_workspace_current() {
        // This project itself is a valid Cargo workspace
        let ws = detect_workspace(".").expect("should detect workspace in project root");
        assert!(!ws.root.is_empty(), "workspace_root should not be empty");
        assert!(!ws.members.is_empty(), "should have at least one member");
        // The member list should include this crate (tua-rs)
        assert!(
            ws.members.iter().any(|m| m.name == "tua-rs"),
            "expected member 'tua-rs', got: {:?}",
            ws.members.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_detect_workspace_bogus_dir() {
        let err = detect_workspace("/tmp/nonexistent-rs-project-12345").unwrap_err();
        // It should be a CommandFailed or NonZeroExit
        assert!(
            matches!(
                &err,
                WorkspaceError::CommandFailed { .. } | WorkspaceError::NonZeroExit { .. }
            ),
            "expected CommandFailed or NonZeroExit, got: {err:?}"
        );
    }

    #[test]
    fn test_crate_info_fields() {
        let ws = detect_workspace(".").expect("should detect workspace");
        let this_crate = ws
            .members
            .iter()
            .find(|m| m.name == "tua-rs")
            .expect("tua-rs should be a member");
        assert!(!this_crate.path.is_empty(), "path should not be empty");
        // Should have at least some dependencies (serde, tokio, etc.)
        assert!(
            !this_crate.dependencies.is_empty(),
            "tua-rs should have dependencies, got: {:?}",
            this_crate.dependencies
        );
        assert!(
            this_crate.dependencies.contains(&"serde".to_string()),
            "expected 'serde' in dependencies, got: {:?}",
            this_crate.dependencies
        );
    }

    #[test]
    fn test_crate_path_resolution() {
        let ws = detect_workspace(".").expect("should detect workspace");
        for member in &ws.members {
            let manifest = format!("{}/Cargo.toml", member.path);
            assert!(
                std::path::Path::new(&manifest).exists(),
                "expected Cargo.toml at {}",
                manifest
            );
        }
    }

    #[test]
    fn test_workspace_error_display() {
        let err = WorkspaceError::CommandFailed {
            dir: "/fake".into(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "cargo not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("cargo metadata"));
        assert!(msg.contains("/fake"));

        let err = WorkspaceError::NonZeroExit {
            dir: "/fake".into(),
            stderr: "error: could not find Cargo.toml".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("failed"));
        assert!(msg.contains("could not find"));
    }
}
