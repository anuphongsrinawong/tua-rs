//! Rust coding profiles — style guardrails for the agent.
//!
//! Each profile defines what kind of Rust code the agent should produce.
//! Based on Tua Agent v0.0.2 Python profiles.

/// A Rust coding profile with style guardrails.
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
        description: "Idiomatic, performant Rust",
        use_when: "Production Rust code",
        forbid_unwrap: true,
        forbid_unsafe: false,
        require_doc_tests: true,
        enforce_clippy_pedantic: false,
    },
    RustProfile {
        name: "cargo-cult",
        emoji: "📦",
        description: "Dependency-smart",
        use_when: "Crate selection, feature flags",
        forbid_unwrap: false,
        forbid_unsafe: false,
        require_doc_tests: false,
        enforce_clippy_pedantic: false,
    },
    RustProfile {
        name: "strict",
        emoji: "🛡️",
        description: "All guardrails enabled",
        use_when: "Mission-critical Rust",
        forbid_unwrap: true,
        forbid_unsafe: true,
        require_doc_tests: true,
        enforce_clippy_pedantic: true,
    },
];
