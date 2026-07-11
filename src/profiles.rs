//! Rust coding profiles — style guardrails for the agent.
//!
//! Each profile defines what kind of Rust code the agent should produce.
//! Based on Tua Agent v0.0.2 Python profiles.

/// A Rust coding profile with style guardrails.
#[derive(Debug, Clone, Copy)]
pub struct RustProfile {
    pub name: &'static str,
    pub emoji: &'static str,
    pub description: &'static str,
    pub use_when: &'static str,
    pub forbid_unwrap: bool,
    pub forbid_unsafe: bool,
    pub require_doc_tests: bool,
    pub enforce_clippy_pedantic: bool,
}

/// All available Rust coding profiles.
pub const ALL_PROFILES: &[RustProfile] = &[
    RustProfile {
        name: "ferris",
        emoji: "🦀",
        description: "Friendly, beginner-friendly Rust",
        use_when: "Teaching Rust, onboarding",
        forbid_unwrap: false,
        forbid_unsafe: true,
        require_doc_tests: true,
        enforce_clippy_pedantic: false,
    },
    RustProfile {
        name: "borrow-checker",
        emoji: "🔍",
        description: "Strict lifetime auditing",
        use_when: "Debugging ownership issues",
        forbid_unwrap: false,
        forbid_unsafe: false,
        require_doc_tests: false,
        enforce_clippy_pedantic: true,
    },
    RustProfile {
        name: "rustacean",
        emoji: "🚀",
        description: "Idiomatic, performant Rust engineer",
        use_when: "Production Rust code, zero-cost abstractions",
        forbid_unwrap: true,
        forbid_unsafe: true,
        require_doc_tests: true,
        enforce_clippy_pedantic: true,
    },
    RustProfile {
        name: "cargo-cult",
        emoji: "📦",
        description: "Dependency-smart, ecosystem-aware",
        use_when: "Crate selection, feature flags, Cargo.toml editing",
        forbid_unwrap: false,
        forbid_unsafe: false,
        require_doc_tests: false,
        enforce_clippy_pedantic: false,
    },
    RustProfile {
        name: "unsafe-ferris",
        emoji: "🔓",
        description: "Unsafe Rust specialist",
        use_when: "FFI, raw pointers, inline assembly, unsafe abstractions",
        forbid_unwrap: false,
        forbid_unsafe: false,
        require_doc_tests: true,
        enforce_clippy_pedantic: true,
    },
    RustProfile {
        name: "test-crab",
        emoji: "🧪",
        description: "Testing-focused, thorough coverage",
        use_when: "Writing unit tests, integration tests, property tests, fuzzing harnesses",
        forbid_unwrap: false,
        forbid_unsafe: true,
        require_doc_tests: false,
        enforce_clippy_pedantic: false,
    },
    RustProfile {
        name: "doc-crab",
        emoji: "📖",
        description: "Documentation-focused, educational",
        use_when: "Writing doc comments, examples, rustdoc, educational material",
        forbid_unwrap: false,
        forbid_unsafe: true,
        require_doc_tests: true,
        enforce_clippy_pedantic: false,
    },
    RustProfile {
        name: "strict",
        emoji: "🛡️",
        description: "All guardrails enabled",
        use_when: "Mission-critical Rust, safety-critical systems",
        forbid_unwrap: true,
        forbid_unsafe: true,
        require_doc_tests: true,
        enforce_clippy_pedantic: true,
    },
];

/// Look up a profile by name (case-insensitive).
///
/// Returns `None` if no profile matches the given name.
///
/// # Examples
///
/// ```
/// # use tua_rs::profiles::get_profile;
/// let profile = get_profile("rustacean").expect("profile should exist");
/// assert_eq!(profile.name, "rustacean");
/// assert_eq!(profile.emoji, "🚀");
/// ```
pub fn get_profile(name: &str) -> Option<&'static RustProfile> {
    ALL_PROFILES
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Build a human-readable guardrail context string from a profile.
///
/// The output lists active restrictions and requirements in a format suitable
/// for inclusion in a system prompt.
///
/// # Examples
///
/// ```
/// # use tua_rs::profiles::{get_profile, build_profile_context};
/// let profile = get_profile("strict").unwrap();
/// let context = build_profile_context(profile);
/// assert!(context.contains("🛡️"));
/// assert!(context.contains(".unwrap() / .expect() FORBIDDEN"));
/// assert!(context.contains("unsafe code FORBIDDEN"));
/// assert!(context.contains("Doc-tests REQUIRED"));
/// assert!(context.contains("clippy::pedantic ENFORCED"));
/// ```
pub fn build_profile_context(profile: &RustProfile) -> String {
    let mut lines = Vec::with_capacity(8);

    lines.push(format!(
        "## Active Rust Profile: {} {}",
        profile.emoji, profile.name
    ));
    lines.push(format!("  {}", profile.description));
    lines.push(String::new());
    lines.push("Profile Guardrails:".to_string());

    if profile.forbid_unwrap {
        lines.push("  ❌ .unwrap() / .expect() FORBIDDEN".to_string());
    }
    if profile.forbid_unsafe {
        lines.push("  ❌ unsafe code FORBIDDEN".to_string());
    }
    if profile.require_doc_tests {
        lines.push("  ✅ Doc-tests REQUIRED on all public API".to_string());
    }
    if profile.enforce_clippy_pedantic {
        lines.push("  ✅ clippy::pedantic ENFORCED".to_string());
    }

    // Always-enforced baseline rules
    lines.push("  ✅ rustfmt REQUIRED after every change".to_string());
    lines.push("  ✅ cargo check REQUIRED before suggesting changes".to_string());

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensure all 8 profiles are present.
    #[test]
    fn test_all_profiles_present() {
        assert_eq!(ALL_PROFILES.len(), 8, "expected exactly 8 profiles");
    }

    /// Every profile must have non-empty fields.
    #[test]
    fn test_profile_fields_non_empty() {
        for p in ALL_PROFILES {
            assert!(!p.name.is_empty(), "profile name is empty");
            assert!(!p.emoji.is_empty(), "profile {} emoji is empty", p.name);
            assert!(
                !p.description.is_empty(),
                "profile {} description is empty",
                p.name
            );
            assert!(
                !p.use_when.is_empty(),
                "profile {} use_when is empty",
                p.name
            );
        }
    }

    /// Profile names must be unique.
    #[test]
    fn test_profile_names_unique() {
        let mut names: Vec<&str> = ALL_PROFILES.iter().map(|p| p.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            ALL_PROFILES.len(),
            "duplicate profile names found"
        );
    }

    /// `get_profile` returns the correct profile for all valid names.
    #[test]
    fn test_get_profile_by_name() {
        for p in ALL_PROFILES {
            let found = get_profile(p.name).expect("get_profile should find profile");
            assert_eq!(
                found.name, p.name,
                "get_profile returned wrong profile for '{}'",
                p.name
            );
        }
    }

    /// `get_profile` is case-insensitive.
    #[test]
    fn test_get_profile_case_insensitive() {
        let variants = ["RUSTACEAN", "Rustacean", "rustacean", "RuStAcEaN"];
        for v in &variants {
            let found = get_profile(v)
                .unwrap_or_else(|| panic!("get_profile should find '{}' case-insensitively", v));
            assert_eq!(found.name, "rustacean");
        }
    }

    /// `get_profile` returns `None` for unknown names.
    #[test]
    fn test_get_profile_unknown() {
        assert!(get_profile("nonexistent").is_none());
        assert!(get_profile("").is_none());
        assert!(get_profile("rustacean-extra").is_none());
    }

    /// `build_profile_context` includes the profile emoji and name.
    #[test]
    fn test_context_contains_profile_header() {
        for p in ALL_PROFILES {
            let ctx = build_profile_context(p);
            assert!(
                ctx.contains(p.emoji),
                "context for '{}' should contain emoji '{}'",
                p.name,
                p.emoji
            );
            assert!(
                ctx.contains(p.name),
                "context for '{}' should contain name",
                p.name
            );
        }
    }

    /// `build_profile_context` includes forced guardrails.
    #[test]
    fn test_context_forced_guardrails() {
        let ctx = build_profile_context(&ALL_PROFILES[0]);
        assert!(ctx.contains("rustfmt REQUIRED"));
        assert!(ctx.contains("cargo check REQUIRED"));
    }

    /// `build_profile_context` content matches guardrail booleans.
    #[test]
    fn test_context_guardrails_match_profile() {
        for p in ALL_PROFILES {
            let ctx = build_profile_context(p);
            let has_unwrap_ban = ctx.contains(".unwrap() / .expect() FORBIDDEN");
            let has_unsafe_ban = ctx.contains("unsafe code FORBIDDEN");
            let has_doc_test_req = ctx.contains("Doc-tests REQUIRED");
            let has_clippy_pedantic = ctx.contains("clippy::pedantic ENFORCED");

            assert_eq!(
                has_unwrap_ban, p.forbid_unwrap,
                "profile '{}': forbid_unwrap mismatch in context",
                p.name
            );
            assert_eq!(
                has_unsafe_ban, p.forbid_unsafe,
                "profile '{}': forbid_unsafe mismatch in context",
                p.name
            );
            assert_eq!(
                has_doc_test_req, p.require_doc_tests,
                "profile '{}': require_doc_tests mismatch in context",
                p.name
            );
            assert_eq!(
                has_clippy_pedantic, p.enforce_clippy_pedantic,
                "profile '{}': enforce_clippy_pedantic mismatch in context",
                p.name
            );
        }
    }

    // ── Individual profile smoke tests ──────────────────────────────────

    macro_rules! assert_profile {
        ($name:ident, $expected_name:literal, $expected_emoji:literal) => {
            let p = get_profile($expected_name)
                .unwrap_or_else(|| panic!("profile '{}' not found", $expected_name));
            assert_eq!(
                p.name,
                $expected_name,
                "{} name mismatch",
                stringify!($name)
            );
            assert_eq!(
                p.emoji,
                $expected_emoji,
                "{} emoji mismatch",
                stringify!($name)
            );
        };
    }

    #[test]
    fn test_ferris_profile() {
        assert_profile!(ferris, "ferris", "🦀");
        let p = get_profile("ferris").unwrap();
        assert!(!p.forbid_unwrap);
        assert!(p.forbid_unsafe);
        assert!(p.require_doc_tests);
        assert!(!p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_borrow_checker_profile() {
        assert_profile!(borrow_checker, "borrow-checker", "🔍");
        let p = get_profile("borrow-checker").unwrap();
        assert!(!p.forbid_unwrap);
        assert!(!p.forbid_unsafe);
        assert!(!p.require_doc_tests);
        assert!(p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_rustacean_profile() {
        assert_profile!(rustacean, "rustacean", "🚀");
        let p = get_profile("rustacean").unwrap();
        assert!(p.forbid_unwrap);
        assert!(p.forbid_unsafe);
        assert!(p.require_doc_tests);
        assert!(p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_cargo_cult_profile() {
        assert_profile!(cargo_cult, "cargo-cult", "📦");
        let p = get_profile("cargo-cult").unwrap();
        assert!(!p.forbid_unwrap);
        assert!(!p.forbid_unsafe);
        assert!(!p.require_doc_tests);
        assert!(!p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_unsafe_ferris_profile() {
        assert_profile!(unsafe_ferris, "unsafe-ferris", "🔓");
        let p = get_profile("unsafe-ferris").unwrap();
        assert!(!p.forbid_unwrap);
        assert!(!p.forbid_unsafe);
        assert!(p.require_doc_tests);
        assert!(p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_test_crab_profile() {
        assert_profile!(test_crab, "test-crab", "🧪");
        let p = get_profile("test-crab").unwrap();
        assert!(!p.forbid_unwrap);
        assert!(p.forbid_unsafe);
        assert!(!p.require_doc_tests);
        assert!(!p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_doc_crab_profile() {
        assert_profile!(doc_crab, "doc-crab", "📖");
        let p = get_profile("doc-crab").unwrap();
        assert!(!p.forbid_unwrap);
        assert!(p.forbid_unsafe);
        assert!(p.require_doc_tests);
        assert!(!p.enforce_clippy_pedantic);
    }

    #[test]
    fn test_strict_profile() {
        assert_profile!(strict, "strict", "🛡️");
        let p = get_profile("strict").unwrap();
        assert!(p.forbid_unwrap);
        assert!(p.forbid_unsafe);
        assert!(p.require_doc_tests);
        assert!(p.enforce_clippy_pedantic);
    }

    /// The `rusacean` profile description was updated to be more descriptive.
    #[test]
    fn test_rustacean_description() {
        let p = get_profile("rustacean").unwrap();
        assert!(
            p.description.contains("performant"),
            "rustacean description should mention performance: {}",
            p.description
        );
        assert!(
            p.use_when.contains("Production"),
            "rustacean use_when should mention Production: {}",
            p.use_when
        );
    }
}
