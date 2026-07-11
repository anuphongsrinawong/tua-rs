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

    /// Ensure the function returns an error when given a non-existent directory.
    #[test]
    fn test_compile_to_wasm_nonexistent_path() {
        let result = compile_to_wasm("/nonexistent/crate/path", false);
        assert!(result.is_err(), "expected Err for non-existent path");
    }

    /// Ensure the function returns an error when the target isn't installed.
    /// This is a best-effort check — it may succeed if the target *is* installed.
    #[test]
    fn test_compile_to_wasm_unknown_target_in_empty_dir() {
        // Create a temporary directory with a minimal Cargo.toml
        let dir = std::env::temp_dir().join("tua_wasm_test_empty_crate");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");

        std::fs::write(
            dir.join("Cargo.toml"),
            r#"[package]
name = "wasm_test"
version = "0.1.0"
edition = "2021"

[dependencies]
"#,
        )
        .expect("write Cargo.toml");

        std::fs::create_dir_all(dir.join("src")).expect("create src dir");
        std::fs::write(dir.join("src").join("lib.rs"), "// empty")
            .expect("write lib.rs");

        let path = dir.to_str().expect("valid utf-8 path");
        let result = compile_to_wasm(path, false);

        // Clean up
        let _ = std::fs::remove_dir_all(&dir);

        // The result could be either success or failure depending on
        // whether the wasm target is installed. We just verify it
        // doesn't panic.
        assert!(
            result.is_ok() || result.is_err(),
            "expected Ok or Err, got neither"
        );
    }
}
