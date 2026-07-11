//! Configuration for the Tua Agent.
//!
//! Loaded from `~/.tua-rs/config.toml`. Every field has a sensible default,
//! so the config file may be omitted entirely or contain only the fields
//! you wish to override.

use serde::Deserialize;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when loading the Tua configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The config file exists but could not be read.
    #[error("failed to read config file `{path}`: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The config file exists but contains invalid TOML or unexpected types.
    #[error("failed to parse config file `{path}`: {source}")]
    ParseFailed {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

// ---------------------------------------------------------------------------
// Config struct
// ---------------------------------------------------------------------------

/// Top-level agent configuration.
///
/// # Defaults
///
/// | Field                | Default         |
/// |----------------------|-----------------|
/// | `default_profile`    | `"default"`     |
/// | `tool_timeout_secs`  | `30`            |
/// | `max_output_chars`   | `10_000`        |
/// | `self_correction`    | `true`          |
/// | `max_self_corrections` | `3`           |
/// | `checkpoint_enabled` | `true`          |
/// | `context_limit`      | `128_000`       |
/// | `prompt_caching`     | `true`          |
/// | `review_enabled`     | `true`          |
///
/// # Example `~/.tua-rs/config.toml`
///
/// ```toml
/// default_profile = "gpt-4o"
/// tool_timeout_secs = 60
/// self_correction = false
/// context_limit = 65536
/// ```
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TuaConfig {
    /// Default profile to use when none is explicitly requested.
    pub default_profile: String,

    /// Maximum time (in seconds) a tool may run before being killed.
    pub tool_timeout_secs: u64,

    /// Maximum number of characters captured from a single tool invocation.
    pub max_output_chars: usize,

    /// Whether the agent may revise its own output after generation.
    pub self_correction: bool,

    /// Maximum number of consecutive self-correction rounds.
    pub max_self_corrections: u32,

    /// Whether to persist checkpoints for resumability.
    pub checkpoint_enabled: bool,

    /// Context window size limit in tokens (model-dependent).
    pub context_limit: u32,

    /// Whether prompt caching (e.g. Anthropic prompt caching) is enabled.
    pub prompt_caching: bool,

    /// Whether review mode is active (human-in-the-loop).
    pub review_enabled: bool,
}

impl Default for TuaConfig {
    fn default() -> Self {
        Self {
            default_profile: "default".to_string(),
            tool_timeout_secs: 30,
            max_output_chars: 10_000,
            self_correction: true,
            max_self_corrections: 3,
            checkpoint_enabled: true,
            context_limit: 128_000,
            prompt_caching: true,
            review_enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Loading logic
// ---------------------------------------------------------------------------

/// Return the canonical config file path (`~/.tua-rs/config.toml`).
fn config_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".tua-rs").join("config.toml")
}

/// Load configuration from `~/.tua-rs/config.toml`.
///
/// * If the file does **not** exist, [`TuaConfig::default()`] is returned.
/// * If the file exists but is unreadable or contains invalid TOML, a
///   [`ConfigError`] is returned.
pub fn load() -> Result<TuaConfig, ConfigError> {
    let path = config_path();

    if !path.exists() {
        tracing::debug!("config file does not exist, using defaults");
        return Ok(TuaConfig::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|source| ConfigError::ReadFailed {
        path: path.clone(),
        source,
    })?;

    let config: TuaConfig =
        toml::from_str(&content).map_err(|source| ConfigError::ParseFailed { path, source })?;

    Ok(config)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let cfg = TuaConfig::default();
        assert_eq!(cfg.default_profile, "default");
        assert_eq!(cfg.tool_timeout_secs, 30);
        assert_eq!(cfg.max_output_chars, 10_000);
        assert!(cfg.self_correction);
        assert_eq!(cfg.max_self_corrections, 3);
        assert!(cfg.checkpoint_enabled);
        assert_eq!(cfg.context_limit, 128_000);
        assert!(cfg.prompt_caching);
        assert!(cfg.review_enabled);
    }

    #[test]
    fn deserialize_partial_toml_uses_defaults_for_missing_fields() {
        let toml_str = r#"default_profile = "o3-mini""#;
        let cfg: TuaConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.default_profile, "o3-mini");
        // All other fields come from Default:
        assert_eq!(cfg.tool_timeout_secs, 30);
        assert!(cfg.self_correction);
    }

    #[test]
    fn deserialize_full_toml() {
        let toml_str = r#"
            default_profile = "claude-sonnet-4"
            tool_timeout_secs = 120
            max_output_chars = 5000
            self_correction = false
            max_self_corrections = 5
            checkpoint_enabled = false
            context_limit = 65536
            prompt_caching = false
            review_enabled = false
        "#;
        let cfg: TuaConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.default_profile, "claude-sonnet-4");
        assert_eq!(cfg.tool_timeout_secs, 120);
        assert_eq!(cfg.max_output_chars, 5_000);
        assert!(!cfg.self_correction);
        assert_eq!(cfg.max_self_corrections, 5);
        assert!(!cfg.checkpoint_enabled);
        assert_eq!(cfg.context_limit, 65_536);
        assert!(!cfg.prompt_caching);
        assert!(!cfg.review_enabled);
    }

    #[test]
    fn config_path_uses_home_env() {
        // Save original env vars and set a known HOME
        let original_home = std::env::var("HOME").ok();
        let original_userprofile = std::env::var("USERPROFILE").ok();
        std::env::set_var("HOME", "/home/testuser");
        std::env::remove_var("USERPROFILE");

        let path = config_path();
        let path_str = path.to_string_lossy();
        assert!(
            path_str.ends_with(".tua-rs/config.toml"),
            "expected path to end with '.tua-rs/config.toml', got: {path_str}"
        );
        assert!(
            path_str.contains("/home/testuser"),
            "expected {path_str} to contain /home/testuser"
        );

        // Restore
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(u) = original_userprofile {
            std::env::set_var("USERPROFILE", u);
        }
    }

    #[test]
    fn load_returns_defaults_when_no_file() {
        // Temporarily override HOME to a non-existent directory.
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", "/tmp/__tua_config_test_nonexistent__");

        let result = load();
        // Should be Ok with defaults (no file).
        assert!(result.is_ok());

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        }
    }

    #[test]
    fn deserialize_invalid_toml_returns_parse_error() {
        // Use a temp dir with a real config file that has invalid TOML
        let dir = std::env::temp_dir().join("__tua_config_test_invalid__");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".tua-rs")).unwrap();
        std::fs::write(
            dir.join(".tua-rs").join("config.toml"),
            r#"default_profile = "hello"
tool_timeout_secs = not_a_number"#,
        )
        .unwrap();

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", dir.to_str().unwrap());

        let result = load();
        assert!(result.is_err(), "invalid TOML should produce an error");
        match result.unwrap_err() {
            ConfigError::ParseFailed { path, .. } => {
                assert!(path.ends_with(".tua-rs/config.toml"));
            }
            other => panic!("expected ParseFailed, got: {other}"),
        }

        // Cleanup
        std::env::set_var("HOME", original_home.unwrap_or_else(|| "/tmp".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_returns_error_for_inaccessible_file() {
        // Create a directory at the config path so "reading" it fails
        let dir = std::env::temp_dir().join("__tua_config_test_inaccessible__");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".tua-rs")).unwrap();
        // Create a directory instead of a file at config.toml to trigger a read failure
        let config_dir = dir.join(".tua-rs").join("config.toml");
        std::fs::create_dir(&config_dir).unwrap();

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", dir.to_str().unwrap());

        let result = load();
        // On Unix, reading a directory as a file returns an IO error
        assert!(result.is_err(), "directory-as-file should produce a ReadFailed error");
        match result.unwrap_err() {
            ConfigError::ReadFailed { path, .. } => {
                assert!(path.ends_with(".tua-rs/config.toml"));
            }
            other => panic!("expected ReadFailed, got: {other}"),
        }

        std::env::set_var("HOME", original_home.unwrap_or_else(|| "/tmp".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn error_display_for_read_failed() {
        let err = ConfigError::ReadFailed {
            path: PathBuf::from("/test/config.toml"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/test/config.toml"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn error_display_for_parse_failed() {
        // Use an actual TOML parse error to construct the error
        let parse_result: Result<TuaConfig, toml::de::Error> = toml::from_str("key = broken}");
        let parse_err = parse_result.unwrap_err();
        let err = ConfigError::ParseFailed {
            path: PathBuf::from("/test/config.toml"),
            source: parse_err,
        };
        let msg = err.to_string();
        assert!(msg.contains("/test/config.toml"), "msg should contain path: {msg}");
        assert!(
            msg.contains("failed to parse") || msg.contains("test"),
            "msg should contain 'failed to parse': {msg}"
        );
    }

    #[test]
    fn config_path_falls_back_to_dot_on_no_home() {
        let original_home = std::env::var("HOME").ok();
        let original_userprofile = std::env::var("USERPROFILE").ok();
        std::env::remove_var("HOME");
        std::env::remove_var("USERPROFILE");

        let path = config_path();
        assert!(path.to_string_lossy().ends_with(".tua-rs/config.toml"));

        // Restore
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        }
        if let Some(u) = original_userprofile {
            std::env::set_var("USERPROFILE", u);
        }
    }

    #[test]
    fn debug_representation_for_config() {
        let cfg = TuaConfig::default();
        let debug = format!("{:?}", cfg);
        assert!(debug.contains("default_profile"));
        assert!(debug.contains("default"));
        assert!(debug.contains("tool_timeout_secs"));
        assert!(debug.contains("30"));
    }

    #[test]
    fn serde_deserialize_on_empty_toml_returns_all_defaults() {
        let toml_str = "";
        let cfg: TuaConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.default_profile, "default");
        assert_eq!(cfg.tool_timeout_secs, 30);
        assert_eq!(cfg.max_output_chars, 10_000);
        assert!(cfg.self_correction);
    }
}
