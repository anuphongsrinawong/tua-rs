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
    let stdout = std::str::from_utf8(&output.stdout)
        .map_err(|_| WorkspaceError::InvalidUtf8 { dir: dir.clone() })?;

    // --- Parse JSON ---
    let meta: CargoMetadata =
        serde_json::from_str(stdout).map_err(|source| WorkspaceError::ParseFailed {
            dir: dir.clone(),
            source,
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

        let deps: Vec<String> = pkg.dependencies.into_iter().map(|d| d.name).collect();

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
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    /// Generate content for a minimal single-crate `Cargo.toml`.
    fn single_crate_toml(name: &str) -> String {
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"
"#
        )
    }

    /// Generate content for a single-crate manifest with dependencies.
    fn crate_with_deps_toml(name: &str, deps: &[&str]) -> String {
        let dep_lines: String = deps
            .iter()
            .map(|d| format!(r#"{d} = "0.1""#))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
{dep_lines}
"#
        )
    }

    /// Generate content for a workspace root `Cargo.toml`.
    fn workspace_root_toml(members: &[&str]) -> String {
        let members_list: String = members
            .iter()
            .map(|m| format!("    \"{m}\""))
            .collect::<Vec<_>>()
            .join(",\n");
        format!(
            r#"[workspace]
members = [
{members_list}
]
"#
        )
    }

    /// A temporary Rust project directory that is cleaned up on drop.
    struct TempProject {
        root: PathBuf,
        /// Only set to `false` to inspect the directory after a test failure.
        cleanup: bool,
    }

    impl TempProject {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("tua_workspace_test_{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&root).expect("failed to create temp dir");
            Self {
                root,
                cleanup: true,
            }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        /// Write `Cargo.toml` at the project root and create `src/lib.rs`.
        fn write_cargo_toml(&self, content: &str) {
            let path = self.root.join("Cargo.toml");
            let mut f = fs::File::create(&path).expect("failed to create Cargo.toml");
            write!(f, "{content}").expect("failed to write Cargo.toml");
            // `cargo metadata` requires a target (lib.rs, main.rs, etc.)
            let src_dir = self.root.join("src");
            fs::create_dir_all(&src_dir).expect("failed to create src dir");
            let mut lib =
                fs::File::create(src_dir.join("lib.rs")).expect("failed to create lib.rs");
            write!(lib, "// empty").expect("failed to write lib.rs");
        }

        /// Create a member crate directory with its own `Cargo.toml` and `src/lib.rs`.
        fn add_member(&self, name: &str, content: &str) -> PathBuf {
            let dir = self.root.join(name);
            fs::create_dir_all(&dir).expect("failed to create member dir");
            let mut f = fs::File::create(dir.join("Cargo.toml"))
                .expect("failed to create member Cargo.toml");
            write!(f, "{content}").expect("failed to write member Cargo.toml");
            // `cargo metadata` requires a target
            let src_dir = dir.join("src");
            fs::create_dir_all(&src_dir).expect("failed to create member src dir");
            let mut lib =
                fs::File::create(src_dir.join("lib.rs")).expect("failed to create member lib.rs");
            write!(lib, "// empty").expect("failed to write member lib.rs");
            dir
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            if self.cleanup {
                let _ = fs::remove_dir_all(&self.root);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Existing tests
    // -----------------------------------------------------------------------

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

    // -----------------------------------------------------------------------
    // New tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_single_crate_detected_as_workspace() {
        /// Even a single-crate project should be detected as a workspace with
        /// one member (the root package itself).
        let proj = TempProject::new();
        proj.write_cargo_toml(&single_crate_toml("my_crate"));

        let ws = detect_workspace(proj.path().to_str().unwrap())
            .expect("single-crate project should be detected");

        assert_eq!(ws.members.len(), 1, "expected exactly one member");
        assert_eq!(ws.members[0].name, "my_crate", "member name mismatch");
        assert_eq!(
            ws.members[0].path,
            proj.root.to_str().unwrap(),
            "member path should equal the project root"
        );
    }

    #[test]
    fn test_multi_member_workspace() {
        /// A workspace with two members should report both.
        let proj = TempProject::new();
        proj.write_cargo_toml(&workspace_root_toml(&["alpha", "beta"]));
        proj.add_member("alpha", &single_crate_toml("alpha"));
        proj.add_member("beta", &single_crate_toml("beta"));

        let ws = detect_workspace(proj.path().to_str().unwrap())
            .expect("multi-member workspace should be detected");

        assert_eq!(ws.members.len(), 2, "expected exactly 2 members");

        // Collect names into a sorted vec for stable comparison
        let mut names: Vec<&str> = ws.members.iter().map(|m| m.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_empty_directory_gives_non_zero_exit_error() {
        /// An existing directory with no Cargo.toml should produce a
        /// `NonZeroExit` error.
        let proj = TempProject::new();
        // deliberately NOT writing a Cargo.toml

        let err = detect_workspace(proj.path().to_str().unwrap()).unwrap_err();

        assert!(
            matches!(&err, WorkspaceError::NonZeroExit { .. }),
            "expected NonZeroExit for directory without Cargo.toml, got: {err:?}"
        );

        // Verify the error message contains useful information
        let msg = err.to_string();
        assert!(
            msg.contains("failed") || msg.contains("error"),
            "error message should describe the failure: {msg}"
        );
    }

    #[test]
    fn test_all_members_have_nonempty_fields() {
        /// Every member in the current workspace must have non-empty `name`,
        /// `path`, and a non-null `dependencies` vec.
        let ws = detect_workspace(".").expect("should detect workspace");
        assert!(
            !ws.members.is_empty(),
            "expected at least one workspace member"
        );

        for member in &ws.members {
            assert!(!member.name.is_empty(), "member name should not be empty");
            assert!(
                !member.path.is_empty(),
                "member path should not be empty, name={}",
                member.name
            );
            // dependencies may be empty for some members, that's fine — just
            // the Vec itself must be valid
            let _ = &member.dependencies;
        }
    }

    #[test]
    fn test_workspace_root_is_absolute() {
        /// The `Workspace::root` field should be an absolute path.
        let ws = detect_workspace(".").expect("should detect workspace");
        let root_path = Path::new(&ws.root);
        assert!(
            root_path.is_absolute(),
            "workspace root must be absolute, got: {}",
            ws.root
        );
    }

    #[test]
    fn test_crate_dependencies_included() {
        /// When a crate declares dependencies, they should appear in
        /// `CrateInfo::dependencies`.
        let proj = TempProject::new();
        proj.write_cargo_toml(&crate_with_deps_toml("with-deps", &["serde", "tokio"]));

        let ws = detect_workspace(proj.path().to_str().unwrap())
            .expect("crate with deps should be detected");

        assert_eq!(ws.members.len(), 1, "expected exactly one member");
        let member = &ws.members[0];

        assert!(
            member.dependencies.contains(&"serde".to_string()),
            "expected 'serde' in dependencies, got: {:?}",
            member.dependencies
        );
        assert!(
            member.dependencies.contains(&"tokio".to_string()),
            "expected 'tokio' in dependencies, got: {:?}",
            member.dependencies
        );
    }

    #[test]
    fn test_workspace_serialization_roundtrip() {
        /// `Workspace` and `CrateInfo` must round-trip through JSON
        /// serialization / deserialization faithfully.
        let ws = Workspace {
            root: "/tmp/fake-root".into(),
            members: vec![
                CrateInfo {
                    name: "crate-a".into(),
                    path: "/tmp/fake-root/crate-a".into(),
                    dependencies: vec!["serde".into(), "tokio".into()],
                },
                CrateInfo {
                    name: "crate-b".into(),
                    path: "/tmp/fake-root/crate-b".into(),
                    dependencies: vec![],
                },
            ],
        };

        let json = serde_json::to_string(&ws).expect("serialization should succeed");
        let deserialized: Workspace =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(deserialized.root, ws.root);
        assert_eq!(deserialized.members.len(), ws.members.len());

        for (a, b) in deserialized.members.iter().zip(ws.members.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.path, b.path);
            assert_eq!(a.dependencies, b.dependencies);
        }
    }

    #[test]
    fn test_invalid_manifest_toml_syntax() {
        /// A Cargo.toml with invalid syntax should produce a
        /// `NonZeroExit` error (cargo metadata will fail to parse it).
        let proj = TempProject::new();
        // Write garbage that isn't valid TOML
        proj.write_cargo_toml("<<<invalid toml {{{");

        let err = detect_workspace(proj.path().to_str().unwrap()).unwrap_err();

        assert!(
            matches!(&err, WorkspaceError::NonZeroExit { .. }),
            "expected NonZeroExit for invalid manifest, got: {err:?}"
        );
    }

    #[test]
    fn test_detect_subdir_in_workspace() {
        /// Calling `detect_workspace` from a member crate's subdirectory
        /// (not the workspace root) should still resolve the full workspace.
        let proj = TempProject::new();
        proj.write_cargo_toml(&workspace_root_toml(&["member-a"]));
        proj.add_member("member-a", &single_crate_toml("member-a"));

        // Detect from inside `member-a` rather than the workspace root
        let member_path = proj.root.join("member-a");
        let ws = detect_workspace(member_path.to_str().unwrap())
            .expect("detection from member subdir should succeed");

        assert!(
            ws.members.iter().any(|m| m.name == "member-a"),
            "expected 'member-a' in members, got: {:?}",
            ws.members.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
        // Verify the workspace root is the parent, not the member dir
        assert_eq!(
            Path::new(&ws.root),
            proj.path(),
            "workspace root should be the parent project dir"
        );
    }
}
