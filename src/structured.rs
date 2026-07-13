//! 🎯 Structured output from LLMs via [`rstructor`].
//!
//! Provides type-safe extraction of structured data from LLM responses
//! using JSON Schema derivation. The agent can ask the model to return
//! data in a specific format and parse it into native Rust types.

use rstructor::Instructor;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Structured types
// ---------------------------------------------------------------------------

/// A list of file edits the agent wants to make.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(description = "A set of file edits to apply to a Rust project")]
pub struct FileEdits {
    #[llm(description = "List of individual file edits")]
    pub edits: Vec<FileEdit>,
}

/// A single file edit with find-and-replace semantics.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(description = "A single file modification using find-and-replace")]
pub struct FileEdit {
    #[llm(description = "Path to the file to modify, relative to project root")]
    pub path: String,
    #[llm(description = "Exact string to find in the file")]
    pub old_string: String,
    #[llm(description = "Replacement text")]
    pub new_string: String,
    #[llm(description = "Brief reason for this change")]
    pub reason: String,
}

/// A code review result with issues and approval status.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(description = "Complete code review findings")]
pub struct CodeReview {
    #[llm(description = "Overall assessment of the code")]
    pub summary: String,
    #[llm(description = "List of issues found during review")]
    pub issues: Vec<ReviewIssue>,
    #[llm(description = "Whether the reviewed code is ready to merge")]
    pub approved: bool,
}

/// A single review issue with severity and location.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(description = "A single issue found during code review")]
pub struct ReviewIssue {
    #[llm(description = "Severity level: error, warning, or info")]
    pub severity: String,
    #[llm(description = "File path where the issue was found")]
    pub file: String,
    #[llm(description = "Line number where the issue starts (1-based)")]
    pub line: usize,
    #[llm(description = "Human-readable description of the issue")]
    pub message: String,
    #[llm(description = "Suggested fix if one is obvious, otherwise null")]
    pub suggestion: Option<String>,
}

/// A task decomposition plan for parallel execution.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(description = "Decomposed subtasks for parallel execution by the orchestrator")]
pub struct TaskPlan {
    #[llm(description = "Ordered list of subtasks to execute")]
    pub tasks: Vec<PlannedTask>,
}

/// A single subtask in an execution plan.
#[derive(Debug, Clone, Serialize, Deserialize, Instructor)]
#[llm(description = "A single subtask that can be executed independently or in parallel")]
pub struct PlannedTask {
    #[llm(description = "Short description of what this subtask does")]
    pub description: String,
    #[llm(description = "Files this task will read or modify")]
    pub files: Vec<String>,
    #[llm(description = "True if this task can run in parallel with other non-conflicting tasks")]
    pub can_parallel: bool,
}

// ---------------------------------------------------------------------------
// Extraction
// ---------------------------------------------------------------------------

/// Extract a structured type from an LLM response string.
///
/// Uses rstructor's JSON Schema extraction to parse the response
/// into the requested Rust type.
pub fn extract<T: Instructor + for<'de> Deserialize<'de>>(
    response: &str,
) -> Result<T, String> {
    T::extract(response).map_err(|e| format!("Extraction failed: {e}"))
}
