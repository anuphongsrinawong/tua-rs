//! Integration tests for the `tua-rs` crate.
//!
//! These tests exercise public APIs across module boundaries as
//! an end-user would use them.

use tua_rs::config::TuaConfig;
use tua_rs::profiles::{build_profile_context, get_profile, ALL_PROFILES};

// ---------------------------------------------------------------------------
// Profiles integration
// ---------------------------------------------------------------------------

/// All 8 profiles can be looked up by name.
#[test]
fn test_all_profiles_accessible_from_crate() {
    let names: Vec<&str> = ALL_PROFILES.iter().map(|p| p.name).collect();
    assert_eq!(names.len(), 8);

    let expected = [
        "ferris",
        "borrow-checker",
        "rustacean",
        "cargo-cult",
        "unsafe-ferris",
        "test-crab",
        "doc-crab",
        "strict",
    ];
    for name in &expected {
        assert!(
            names.contains(name),
            "expected profile '{name}' to be present in ALL_PROFILES"
        );
    }
}

/// `get_profile` works across module boundaries.
#[test]
fn test_get_profile_cross_module() {
    for p in ALL_PROFILES {
        let found = get_profile(p.name).expect("get_profile should work cross-module");
        assert_eq!(found.name, p.name);
        assert_eq!(found.emoji, p.emoji);
        assert_eq!(found.forbid_unwrap, p.forbid_unwrap);
    }
}

/// `build_profile_context` output can be constructed from any profile.
#[test]
fn test_build_context_cross_module() {
    for p in ALL_PROFILES {
        let ctx = build_profile_context(p);
        assert!(
            ctx.contains(p.emoji),
            "context missing emoji for profile '{}'",
            p.name
        );
        assert!(
            ctx.contains("Profile Guardrails:"),
            "context should contain guardrails header"
        );
    }
}

/// The `strict` profile should forbid everything.
#[test]
fn test_strict_profile_forbids_all() {
    let p = get_profile("strict").unwrap();
    assert!(p.forbid_unwrap);
    assert!(p.forbid_unsafe);
    assert!(p.require_doc_tests);
    assert!(p.enforce_clippy_pedantic);
}

/// The `ferris` profile should be beginner-friendly (no unwrap ban, no clippy pedantic).
#[test]
fn test_ferris_profile_beginner_friendly() {
    let p = get_profile("ferris").unwrap();
    assert!(!p.forbid_unwrap, "ferris should not forbid unwrap");
    assert!(p.forbid_unsafe, "ferris should forbid unsafe");
    assert!(
        !p.enforce_clippy_pedantic,
        "ferris should not enforce clippy pedantic"
    );
}

// ---------------------------------------------------------------------------
// Config integration
// ---------------------------------------------------------------------------

/// Default config values match expectations when used across modules.
#[test]
fn test_default_config_values() {
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

/// Config can be deserialised from TOML strings with partial fields.
#[test]
fn test_config_partial_deserialization() {
    let toml_str = r#"default_profile = "claude-sonnet-4""#;
    let cfg: TuaConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.default_profile, "claude-sonnet-4");
    // Unspecified fields should use defaults
    assert_eq!(cfg.tool_timeout_secs, 30);
    assert!(cfg.self_correction);
    assert!(cfg.checkpoint_enabled);
    assert_eq!(cfg.context_limit, 128_000);
}

/// Config can be fully deserialised.
#[test]
fn test_config_full_deserialization() {
    let toml_str = r#"
        default_profile = "o3-mini"
        tool_timeout_secs = 60
        max_output_chars = 5000
        self_correction = false
        max_self_corrections = 5
        checkpoint_enabled = false
        context_limit = 65536
        prompt_caching = false
        review_enabled = false
    "#;
    let cfg: TuaConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(cfg.default_profile, "o3-mini");
    assert_eq!(cfg.tool_timeout_secs, 60);
    assert_eq!(cfg.max_output_chars, 5_000);
    assert!(!cfg.self_correction);
    assert_eq!(cfg.max_self_corrections, 5);
    assert!(!cfg.checkpoint_enabled);
    assert_eq!(cfg.context_limit, 65_536);
    assert!(!cfg.prompt_caching);
    assert!(!cfg.review_enabled);
}

/// Empty TOML should deserialise to all defaults.
#[test]
fn test_config_empty_toml() {
    let cfg: TuaConfig = toml::from_str("").unwrap();
    assert_eq!(cfg.default_profile, "default");
    assert_eq!(cfg.tool_timeout_secs, 30);
}

// ---------------------------------------------------------------------------
// Profile + Config interaction
// ---------------------------------------------------------------------------

/// A `TuaConfig` with a known profile name can be looked up.
#[test]
fn test_config_profile_name_maps_to_valid_profile() {
    // The 'rustacean' profile should exist and match
    let profile = get_profile("rustacean");
    assert!(profile.is_some(), "expected 'rustacean' profile to exist");
    let p = profile.unwrap();
    assert_eq!(p.emoji, "🚀");
}

/// Profile guardrails and config settings are independent but compatible.
#[test]
fn test_profile_and_config_compatibility() {
    let cfg = TuaConfig::default();
    // Pick a known profile for testing
    let profile = get_profile("strict").unwrap();

    // Both express similar safety preferences
    assert!(
        cfg.review_enabled,
        "review should be enabled by default for safety"
    );

    // The strict profile forbids everything
    assert!(profile.forbid_unwrap);
    assert!(profile.forbid_unsafe);
    assert!(profile.require_doc_tests);
    assert!(profile.enforce_clippy_pedantic);
}
