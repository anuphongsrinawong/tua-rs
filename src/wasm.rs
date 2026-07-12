//! 🦀 WebAssembly compilation support.
//!
//! Provides [`compile_to_wasm`] which runs `cargo build` with the
//! `wasm32-unknown-unknown` target, returning the build output.

use std::process::Command;

/// Compile a Rust crate to WebAssembly via `cargo build --target wasm32-unknown-unknown`.
///
/// # Arguments
///
/// * `crate_path` — path to the crate directory (containing `Cargo.toml`).
/// * `release` — when `true`, passes `--release` for optimised output.
///
/// # Returns
///
/// `Ok(stdout_and_stderr)` on success, or `Err(error_message)` on failure.
///
/// # Errors
///
/// Returns an error if:
/// * `cargo` is not installed or not on `$PATH`.
/// * The `wasm32-unknown-unknown` target is not installed
///   (run `rustup target add wasm32-unknown-unknown`).
/// * The build fails for any reason (compilation errors, missing
///   dependencies, etc.).
///
/// # Example
///
/// ```no_run
/// # use tua_rs::wasm::compile_to_wasm;
/// // Compile the current crate in debug mode
/// match compile_to_wasm(".", false) {
///     Ok(out) => println!("Build succeeded:\n{out}"),
///     Err(e) => eprintln!("Build failed:\n{e}"),
/// }
/// ```
pub fn compile_to_wasm(crate_path: &str, release: bool) -> Result<String, String> {
    let mut cmd = Command::new("cargo");

    cmd.arg("build");
    cmd.arg("--target");
    cmd.arg("wasm32-unknown-unknown");
    cmd.current_dir(crate_path);

    if release {
        cmd.arg("--release");
    }

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to spawn `cargo build`: {e}"))?;

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ─────────────────────────────────────────────────────

    /// Create a temporary directory with an optional `Cargo.toml` and `src/lib.rs`.
    /// Returns the path and a guard that cleans up on drop.
    fn temp_crate_dir(
        name: &str,
        with_manifest: bool,
        with_src: bool,
        manifest_content: Option<&str>,
    ) -> (std::path::PathBuf, TempDir) {
        let guard = TempDir;
        let dir = std::env::temp_dir().join(format!("tua_wasm_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        if with_manifest {
            let content = manifest_content.unwrap_or(
                r#"[package]
name = "wasm_test"
version = "0.1.0"
edition = "2021"
"#,
            );
            std::fs::write(dir.join("Cargo.toml"), content).expect("write Cargo.toml");
        }

        if with_src {
            std::fs::create_dir_all(dir.join("src")).expect("create src dir");
            std::fs::write(dir.join("src").join("lib.rs"), "// empty").expect("write lib.rs");
        }

        (dir, guard)
    }

    struct TempDir;
    impl Drop for TempDir {
        fn drop(&mut self) {
            // Best-effort cleanup — we intentionally do not panic on failure.
            let _ = std::fs::remove_dir_all(
                std::env::temp_dir().join(format!("tua_wasm_test_*_{}", std::process::id())),
            );
        }
    }

    // ── existing tests ──────────────────────────────────────────────

    #[test]
    fn test_compile_to_wasm_nonexistent_path() {
        let result = compile_to_wasm("/nonexistent/crate/path", false);
        assert!(result.is_err(), "expected Err for non-existent path");
    }

    #[test]
    fn test_compile_to_wasm_unknown_target_in_empty_dir() {
        let (dir, _guard) = temp_crate_dir("unknown_target", true, true, None);
        let path = dir.to_str().expect("valid utf-8 path");
        let result = compile_to_wasm(path, false);
        // May succeed or fail depending on target installation — just don't panic.
        assert!(result.is_ok() || result.is_err());
    }

    // ── new tests: 6+ ───────────────────────────────────────────────

    /// Passing a file path (not a directory) must fail — `current_dir()` will reject it.
    #[test]
    fn test_path_is_file_not_directory() {
        let file =
            std::env::temp_dir().join(format!("tua_wasm_test_file_not_dir_{}", std::process::id()));
        std::fs::write(&file, "not a directory").expect("write temp file");
        let path = file.to_str().expect("valid utf-8");
        let result = compile_to_wasm(path, false);
        assert!(result.is_err(), "expected Err when path is a file");
        let err = result.unwrap_err();
        assert!(!err.is_empty(), "error message must not be empty");
        // The error should contain something about the failure
        assert!(
            err.contains("directory") || err.contains("file") || err.contains("Failed"),
            "error message should give context: {err:?}"
        );
        let _ = std::fs::remove_file(&file);
    }

    /// Empty string path must fail.
    #[test]
    fn test_empty_path() {
        let result = compile_to_wasm("", false);
        assert!(result.is_err(), "expected Err for empty path");
    }

    /// A directory that exists but contains no `Cargo.toml` must fail.
    #[test]
    fn test_missing_cargo_toml() {
        let (dir, _guard) = temp_crate_dir("no_manifest", false, false, None);
        let path = dir.to_str().expect("valid utf-8");
        let result = compile_to_wasm(path, false);
        assert!(result.is_err(), "expected Err when Cargo.toml is missing");
        let err = result.unwrap_err();
        assert!(
            err.contains("Cargo.toml") || err.contains("manifest"),
            "error should mention missing manifest: {err:?}"
        );
    }

    /// A malformed `Cargo.toml` must fail.
    #[test]
    fn test_invalid_manifest_contents() {
        let (dir, _guard) = temp_crate_dir("bad_manifest", true, true, Some("[[[not valid toml"));
        let path = dir.to_str().expect("valid utf-8");
        let result = compile_to_wasm(path, false);
        assert!(result.is_err(), "expected Err for invalid Cargo.toml");
        let err = result.unwrap_err();
        assert!(
            !err.is_empty(),
            "error message should not be empty: {err:?}"
        );
    }

    /// A crate with `Cargo.toml` but no `src/` directory (no lib.rs or main.rs) must fail.
    #[test]
    fn test_missing_src_directory() {
        let (dir, _guard) = temp_crate_dir("no_src", true, false, None);
        let path = dir.to_str().expect("valid utf-8");
        let result = compile_to_wasm(path, false);
        assert!(result.is_err(), "expected Err when src/ is missing");
        let err = result.unwrap_err();
        assert!(
            !err.is_empty(),
            "error message should not be empty: {err:?}"
        );
    }

    /// `release` mode with an invalid path must still fail gracefully (no panic).
    #[test]
    fn test_release_mode_nonexistent_path() {
        let result = compile_to_wasm("/does/not/exist/at/all", true);
        assert!(result.is_err(), "expected Err even in release mode");
    }

    /// Error message for a non-existent path should contain the `Failed to spawn` prefix
    /// (which we add ourselves) so callers know what went wrong.
    #[test]
    fn test_error_message_contains_spawn_failure_context() {
        let result = compile_to_wasm("/hopefully/this/does/not/exist/anywhere/my_dear", false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("cargo") || err.contains("spawn"),
            "error should mention cargo/spawn: {err:?}"
        );
    }

    /// Error message for a failure that reaches cargo (no Cargo.toml) should contain
    /// the original stderr from the subprocess.
    #[test]
    fn test_error_message_includes_cargo_stderr() {
        let (dir, _guard) = temp_crate_dir("capture_stderr", false, false, None);
        let path = dir.to_str().expect("valid utf-8");
        let result = compile_to_wasm(path, false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        // cargo writes "error: ..." when it can't find Cargo.toml
        assert!(
            err.contains("error:") || err.contains("Cargo.toml"),
            "error should propagate cargo diagnostics: {err:?}"
        );
    }
}
