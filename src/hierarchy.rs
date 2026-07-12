//! 🏗️ Hierarchical Agents — specialized subagent router
//!
//! Instead of one monolithic agent holding all 19 tools, this system
//! routes tasks to specialized subagents that each hold only the tools
//! relevant to their domain. Benefits:
//!
//! - **Smaller prompts**: each subagent has a focused context
//! - **Better accuracy**: specialization reduces tool confusion
//! - **Parallel execution**: subagents can work simultaneously
//! - **Lower cost**: smaller prompts = fewer tokens
//!
//! ## Specializations
//! - **rust-expert**: cargo, rustc, clippy, fmt, expand, explain, grep
//! - **test-expert**: cargo test, coverage, mutants, bench
//! - **dep-expert**: cargo add, cargo audit, cargo deny, cargo outdated
//! - **devops-expert**: docker, wasm-pack, cargo build --release

/// Agent specialization domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSpecialty {
    /// Core Rust development: compile, lint, format, search, expand.
    RustExpert,
    /// Testing and quality: test, coverage, mutation testing, benchmarks.
    TestExpert,
    /// Dependency management: add, audit, update, security.
    DepExpert,
    /// Build and deploy: release builds, Docker, WASM.
    DevOpsExpert,
    /// Full access: all tools (orchestrator only).
    Orchestrator,
}

impl AgentSpecialty {
    /// Detect the best specialization for a given task description.
    pub fn detect(task: &str) -> Self {
        let lower = task.to_lowercase();
        if lower.contains("test") || lower.contains("coverage") 
           || lower.contains("bench") || lower.contains("mutant") {
            return Self::TestExpert;
        }
        if lower.contains("dep") || lower.contains("crate") 
           || lower.contains("add") || lower.contains("audit") 
           || lower.contains("secure") || lower.contains("cargo.toml") {
            return Self::DepExpert;
        }
        if lower.contains("build") || lower.contains("release") 
           || lower.contains("docker") || lower.contains("wasm") 
           || lower.contains("deploy") {
            return Self::DevOpsExpert;
        }
        Self::RustExpert
    }

    /// List of tool names available to this specialization.
    pub fn tools(&self) -> &'static [&'static str] {
        match self {
            Self::RustExpert => &[
                "cargo", "rustc", "clippy", "fmt", "rustfmt",
                "grep", "cargo_expand", "rustc_explain",
            ],
            Self::TestExpert => &[
                "cargo", "coverage", "mutants", "cargo_bench",
                "cargo_test_doc",
            ],
            Self::DepExpert => &[
                "cargo_add", "cargo_audit", "cargo_deny",
                "cargo_outdated", "cargo_udeps",
            ],
            Self::DevOpsExpert => &[
                "cargo", "wasm_pack",
            ],
            Self::Orchestrator => &[
                "cargo", "rustc", "clippy", "fmt", "rustfmt",
                "grep", "cargo_expand", "rustc_explain",
                "coverage", "mutants", "cargo_bench", "cargo_test_doc",
                "cargo_add", "cargo_audit", "cargo_deny",
                "cargo_outdated", "cargo_udeps", "wasm_pack",
            ],
        }
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::RustExpert => "🦀 Rust Expert",
            Self::TestExpert => "🧪 Test Expert",
            Self::DepExpert => "📦 Dep Expert",
            Self::DevOpsExpert => "🐳 DevOps Expert",
            Self::Orchestrator => "🎯 Orchestrator",
        }
    }

    /// Task classification: what kind of work fits this specialty.
    pub fn description(&self) -> &'static str {
        match self {
            Self::RustExpert => "Compile, lint, format, search code, expand macros",
            Self::TestExpert => "Write tests, check coverage, run mutation testing",
            Self::DepExpert => "Manage dependencies, audit security, update crates",
            Self::DevOpsExpert => "Build releases, Docker, WASM, deployment",
            Self::Orchestrator => "Full access — routes to subagents as needed",
        }
    }
}

/// A task assignment for a subagent.
#[derive(Debug, Clone)]
pub struct SubAgentTask {
    pub specialty: AgentSpecialty,
    pub prompt: String,
    pub context: String,
    pub max_rounds: usize,
}

/// Result from a subagent execution.
#[derive(Debug, Clone)]
pub struct SubAgentResult {
    pub specialty: AgentSpecialty,
    pub success: bool,
    pub output: String,
    pub rounds_used: usize,
    pub duration_ms: u64,
}

/// Orchestrator that routes tasks to specialized subagents.
pub struct AgentRouter {
    /// Currently available specialties.
    available: Vec<AgentSpecialty>,
    /// Results from last run.
    last_results: Vec<SubAgentResult>,
}

impl AgentRouter {
    pub fn new() -> Self {
        Self {
            available: vec![
                AgentSpecialty::RustExpert,
                AgentSpecialty::TestExpert,
                AgentSpecialty::DepExpert,
                AgentSpecialty::DevOpsExpert,
            ],
            last_results: Vec::new(),
        }
    }

    /// Route a task to the appropriate specialty.
    pub fn route(&self, task: &str) -> AgentSpecialty {
        AgentSpecialty::detect(task)
    }

    /// Split a complex task into specialized subtasks.
    ///
    /// Simplistic split: detect keywords and assign to specialties.
    /// A more sophisticated implementation would use the orchestrator's
    /// task splitting logic.
    pub fn split(&self, task: &str) -> Vec<SubAgentTask> {
        let mut tasks = Vec::new();
        let lower = task.to_lowercase();

        // Detect test-related work
        if lower.contains("test") || lower.contains("coverage") 
           || lower.contains("bench") || lower.contains("mutant") {
            tasks.push(SubAgentTask {
                specialty: AgentSpecialty::TestExpert,
                prompt: "Run tests and quality checks".into(),
                context: "cargo test, coverage, and mutation testing".into(),
                max_rounds: 5,
            });
        }

        // Detect dependency work
        if lower.contains("dep") || lower.contains("add") 
           || lower.contains("audit") || lower.contains("cargo.toml") {
            tasks.push(SubAgentTask {
                specialty: AgentSpecialty::DepExpert,
                prompt: "Manage dependencies".into(),
                context: "Add, audit, or update crates".into(),
                max_rounds: 3,
            });
        }

        // Always include rust expert for code work
        if tasks.is_empty() || lower.contains("fix") 
           || lower.contains("refactor") || lower.contains("implement")
           || lower.contains("build") {
            tasks.push(SubAgentTask {
                specialty: AgentSpecialty::RustExpert,
                prompt: task.to_string(),
                context: "Write, fix, or refactor Rust code".into(),
                max_rounds: 10,
            });
        }

        tasks
    }
}

impl Default for AgentRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rust_expert() {
        let spec = AgentSpecialty::detect("fix the borrow checker error");
        assert_eq!(spec, AgentSpecialty::RustExpert);
    }

    #[test]
    fn test_detect_test_expert() {
        let spec = AgentSpecialty::detect("add coverage tests for session module");
        assert_eq!(spec, AgentSpecialty::TestExpert);
    }

    #[test]
    fn test_detect_dep_expert() {
        let spec = AgentSpecialty::detect("audit dependencies in Cargo.toml");
        assert_eq!(spec, AgentSpecialty::DepExpert);
    }

    #[test]
    fn test_detect_devops_expert() {
        let spec = AgentSpecialty::detect("build release for docker deployment");
        assert_eq!(spec, AgentSpecialty::DevOpsExpert);
    }

    #[test]
    fn test_each_specialty_has_tools() {
        for spec in &[
            AgentSpecialty::RustExpert,
            AgentSpecialty::TestExpert,
            AgentSpecialty::DepExpert,
            AgentSpecialty::DevOpsExpert,
            AgentSpecialty::Orchestrator,
        ] {
            assert!(!spec.tools().is_empty(), "{} should have tools", spec.name());
        }
    }

    #[test]
    fn test_router_split_complex_task() {
        let router = AgentRouter::new();
        let tasks = router.split("add tests and update dependencies");
        assert!(tasks.len() >= 2, "should create at least 2 subtasks");
    }
}
