//! 🦀 Tua Orchestrator — Task decomposition, parallel group planning, and multi-agent dispatch.
//!
//! The orchestrator takes a high-level task description, splits it into independent subtasks,
//! analyzes which files each subtask touches, and spawns parallel or serial worker agents.

use std::collections::HashSet;
use std::process::Command;
use std::time::Instant;

/// A single work item the orchestrator dispatches.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub id: usize,
    pub description: String,
    pub files: Vec<String>,
    pub prompt: String,
    pub can_parallel: bool,
}

/// Result of one worker.
#[derive(Debug)]
pub struct SubTaskResult {
    pub id: usize,
    pub description: String,
    pub success: bool,
    pub output: String,
    pub duration_secs: f64,
}

/// Overall orchestration result.
#[derive(Debug)]
pub struct OrchestrationResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<SubTaskResult>,
}

/// Known modules in this workspace that the splitter can detect.
const KNOWN_MODULES: &[&str] = &[
    "agent",
    "checkpoint",
    "completion",
    "config",
    "dashboard",
    "learning",
    "orchestrator",
    "parallel",
    "profiles",
    "prompts",
    "providers",
    "review",
    "session",
    "skills",
    "tools",
    "tui",
    "wasm",
    "workspace",
];

/// Detect module names mentioned in a task description.
///
/// Looks for:
/// - "X.rs" patterns (e.g. `parallel.rs`)
/// - Known module names as words (e.g. `dashboard`)
/// - "add tests to N modules" → yields all known modules (up to 5)
fn detect_modules(description: &str) -> Vec<String> {
    let lower = description.to_lowercase();
    let mut modules: Vec<String> = Vec::new();

    // Check for "add tests to X modules" or "write tests for N modules"
    if let Some(count) = lower
        .split_whitespace()
        .filter_map(|w| w.parse::<usize>().ok())
        .next()
    {
        if lower.contains("modules") || lower.contains("module") {
            // Return the first `count` known modules as test targets
            return KNOWN_MODULES
                .iter()
                .take(count.min(KNOWN_MODULES.len()))
                .map(|&m| m.to_string())
                .collect();
        }
    }

    // Check for "X.rs" patterns
    for word in lower.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_');
        if let Some(stripped) = cleaned.strip_suffix(".rs") {
            if !stripped.is_empty() && stripped.chars().all(|c| c.is_alphanumeric() || c == '_') {
                modules.push(stripped.to_string());
            }
        }
    }

    // Check for known module names mentioned as standalone words
    for &module in KNOWN_MODULES {
        let module_lower = module.to_lowercase();
        let pattern = format!(" {module_lower} ");
        if (lower.contains(&pattern) || lower.starts_with(&module_lower))
            && !modules.contains(&module.to_string())
        {
            modules.push(module.to_string());
        }
    }

    modules
}

/// Split a high-level task description into discrete [`SubTask`] items.
///
/// The splitter detects module names in the description and creates one subtask
/// per detected module. Each subtask receives a targeted prompt suitable for
/// dispatching to a worker agent.
pub fn split_tasks(description: &str) -> Vec<SubTask> {
    let modules = detect_modules(description);

    if modules.is_empty() {
        // No modules detected — return a single monolithic task
        return vec![SubTask {
            id: 0,
            description: description.to_string(),
            files: Vec::new(),
            prompt: description.to_string(),
            can_parallel: false,
        }];
    }

    let is_testing = description.to_lowercase().contains("test");
    let is_doc = description.to_lowercase().contains("doc");
    let is_fix = description.to_lowercase().contains("fix");

    modules
        .into_iter()
        .enumerate()
        .map(|(i, module)| {
            let action = if is_testing {
                "Write tests for"
            } else if is_doc {
                "Document"
            } else if is_fix {
                "Fix issues in"
            } else {
                "Implement"
            };

            let files = vec![format!("src/{module}.rs")];

            SubTask {
                id: i,
                description: format!("{action} {module}"),
                files,
                prompt: format!(
                    "{action} the `{module}` module in the tua-rs project. \
                     Focus on the `src/{module}.rs` file. \
                     Ensure code is idiomatic, safe, and well-documented.",
                ),
                can_parallel: true,
            }
        })
        .collect()
}

/// Plan parallel execution groups from a set of subtasks.
///
/// Tasks with disjoint file sets are grouped together (can run in parallel).
/// Tasks that modify the same file are placed in different groups (serial within a group).
///
/// Returns a vector of groups, where each group is a vector of subtask indices.
pub fn plan_parallel_groups(tasks: &[SubTask]) -> Vec<Vec<usize>> {
    if tasks.is_empty() {
        return Vec::new();
    }

    // Build file sets for each task
    let file_sets: Vec<HashSet<&str>> = tasks
        .iter()
        .map(|t| t.files.iter().map(|f| f.as_str()).collect())
        .collect();

    let n = tasks.len();
    let mut groups: Vec<Vec<usize>> = Vec::new();

    // Greedy grouping: for each task, try to put it in the first compatible group
    for task_idx in 0..n {
        let task_files = &file_sets[task_idx];
        let mut placed = false;

        for group in groups.iter_mut() {
            // Check if this task conflicts with any task already in the group
            let conflicts = group.iter().any(|&other_idx| {
                let other_files = &file_sets[other_idx];
                !task_files.is_disjoint(other_files)
            });

            if !conflicts {
                group.push(task_idx);
                placed = true;
                break;
            }
        }

        if !placed {
            // Need a new group
            groups.push(vec![task_idx]);
        }
    }

    groups
}

/// Run a single subtask via the `uv` tool, returning a [`SubTaskResult`].
///
/// This is used internally by [`orchestrate`] to dispatch work to a Tua worker agent.
fn run_subtask(task: &SubTask) -> SubTaskResult {
    let start = Instant::now();

    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let tua_project = format!("{home}/tua-agent");

    let output = Command::new("uv")
        .args(["run", "--project", &tua_project, "tua", "-p", &task.prompt])
        .output();

    let (success, output_str) = match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            (out.status.success(), stdout)
        }
        Err(e) => (false, format!("Failed to spawn worker: {e}")),
    };

    let duration = start.elapsed().as_secs_f64();

    SubTaskResult {
        id: task.id,
        description: task.description.clone(),
        success,
        output: output_str,
        duration_secs: duration,
    }
}

/// Main orchestration entry point.
///
/// 1. Calls [`split_tasks`] to break down the description.
/// 2. Calls [`plan_parallel_groups`] to group tasks.
/// 3. For each group, dispatches workers (serial within a group, parallel across groups).
/// 4. Returns a summarized [`OrchestrationResult`].
pub fn orchestrate(description: &str, max_parallel: usize) -> OrchestrationResult {
    let tasks = split_tasks(description);
    let groups = plan_parallel_groups(&tasks);

    let total = tasks.len();
    let mut results: Vec<SubTaskResult> = Vec::with_capacity(total);

    for group in &groups {
        // Within a group, run up to max_parallel tasks at once
        // Since tasks in a group are file-disjoint, they can run in parallel.
        // We use a simple chunked approach: run up to max_parallel at a time.

        for chunk in group.chunks(max_parallel) {
            let mut handles: Vec<(usize, std::thread::JoinHandle<SubTaskResult>)> = Vec::new();

            for &task_idx in chunk {
                let task = tasks[task_idx].clone();
                let handle = std::thread::spawn(move || run_subtask(&task));
                handles.push((task_idx, handle));
            }

            for (task_idx, handle) in handles {
                match handle.join() {
                    Ok(result) => results.push(result),
                    Err(_) => {
                        // Thread panicked
                        let task = &tasks[task_idx];
                        results.push(SubTaskResult {
                            id: task.id,
                            description: task.description.clone(),
                            success: false,
                            output: "Worker thread panicked".to_string(),
                            duration_secs: 0.0,
                        });
                    }
                }
            }
        }
    }

    // Sort results by id for consistent output
    results.sort_by_key(|r| r.id);

    let passed = results.iter().filter(|r| r.success).count();
    let failed = total - passed;

    OrchestrationResult {
        total,
        passed,
        failed,
        results,
    }
}

/// CLI-friendly wrapper around [`orchestrate`] that prints a plan before executing.
///
/// This is the function called from `main.rs`.
pub fn plan_and_run(description: &str, max_parallel: usize) -> OrchestrationResult {
    println!("🦀 Tua Orchestrator — Analyzing task...");

    let tasks = split_tasks(description);
    let groups = plan_parallel_groups(&tasks);

    println!("📋 Found {} subtasks:", tasks.len());

    // Print task plan
    for task in &tasks {
        let parallel_status = if task.can_parallel {
            "PARALLEL ✅"
        } else {
            "SERIAL ⚠️"
        };
        let files = task.files.join(", ");
        println!(
            "   [{}] {} → {} ({})",
            task.id + 1,
            task.description,
            parallel_status,
            files
        );
    }

    println!("🚀 Running {} workers...", tasks.len());
    println!("   {} parallel group(s)", groups.len());

    // Show group assignments
    for (gi, group) in groups.iter().enumerate() {
        let indices: Vec<String> = group.iter().map(|&i| format!("#{}", i + 1)).collect();
        println!("   Group {}: [{}]", gi + 1, indices.join(", "));
    }

    // ── Confirmation prompt ──
    println!();
    println!(
        "🚀 {} workers in {} group(s) — max {} parallel. Run? [Y/n]: ",
        tasks.len(),
        groups.len(),
        max_parallel
    );
    use std::io::{self, Write};
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let input = input.trim().to_lowercase();
    if input == "n" || input == "no" {
        println!("❌ Cancelled.");
        return OrchestrationResult {
            total: tasks.len(),
            passed: 0,
            failed: 0,
            results: vec![],
        };
    }

    let result = orchestrate(description, max_parallel);

    // Print summary
    for (i, r) in result.results.iter().enumerate() {
        let status = if r.success { "✅" } else { "❌" };
        println!(
            "   [{}/{}] 🦀 {} .......... {} ({:.0}s)",
            i + 1,
            result.total,
            r.description,
            status,
            r.duration_secs,
        );
    }

    println!(
        "📊 Result: {} passed, {} failed ({} total)",
        result.passed, result.failed, result.total,
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_tasks_detects_modules() {
        let tasks = split_tasks("add tests to parallel.rs and dashboard.rs");
        assert_eq!(tasks.len(), 2, "should detect two modules");
        assert!(
            tasks[0].description.contains("parallel"),
            "first task targets parallel"
        );
        assert!(
            tasks[1].description.contains("dashboard"),
            "second task targets dashboard"
        );
        assert!(tasks[0].can_parallel, "tasks should be parallel");
    }

    #[test]
    fn test_split_tasks_single_task() {
        let tasks = split_tasks("fix the parallel module");
        // "parallel" is a known module — should detect it
        assert!(!tasks.is_empty(), "should detect at least one task");
        let has_parallel = tasks.iter().any(|t| {
            t.description.to_lowercase().contains("parallel")
                || t.files.iter().any(|f| f.contains("parallel"))
        });
        assert!(has_parallel, "should include parallel module");
    }

    #[test]
    fn test_plan_parallel_groups_no_conflict() {
        let tasks = vec![
            SubTask {
                id: 0,
                description: "Task A".to_string(),
                files: vec!["src/a.rs".to_string()],
                prompt: "do A".to_string(),
                can_parallel: true,
            },
            SubTask {
                id: 1,
                description: "Task B".to_string(),
                files: vec!["src/b.rs".to_string()],
                prompt: "do B".to_string(),
                can_parallel: true,
            },
            SubTask {
                id: 2,
                description: "Task C".to_string(),
                files: vec!["src/c.rs".to_string()],
                prompt: "do C".to_string(),
                can_parallel: true,
            },
        ];

        let groups = plan_parallel_groups(&tasks);
        assert_eq!(groups.len(), 1, "all tasks should fit in one group");
        assert_eq!(groups[0].len(), 3, "all three tasks in same group");
    }

    #[test]
    fn test_plan_parallel_groups_same_file_serial() {
        let tasks = vec![
            SubTask {
                id: 0,
                description: "Task A".to_string(),
                files: vec!["src/shared.rs".to_string()],
                prompt: "do A".to_string(),
                can_parallel: false,
            },
            SubTask {
                id: 1,
                description: "Task B".to_string(),
                files: vec!["src/shared.rs".to_string()],
                prompt: "do B".to_string(),
                can_parallel: false,
            },
        ];

        let groups = plan_parallel_groups(&tasks);
        assert!(
            groups.len() >= 2,
            "conflicting tasks must be in separate groups"
        );
        assert_eq!(groups[0].len(), 1, "each group should have one task");
        assert_eq!(groups[1].len(), 1, "each group should have one task");
    }

    #[test]
    fn test_plan_parallel_groups_mixed() {
        let tasks = vec![
            SubTask {
                id: 0,
                description: "Task A".to_string(),
                files: vec!["src/a.rs".to_string()],
                prompt: "do A".to_string(),
                can_parallel: true,
            },
            SubTask {
                id: 1,
                description: "Task B".to_string(),
                files: vec!["src/b.rs".to_string()],
                prompt: "do B".to_string(),
                can_parallel: true,
            },
            SubTask {
                id: 2,
                description: "Task C".to_string(),
                files: vec!["src/a.rs".to_string()],
                prompt: "do C".to_string(),
                can_parallel: true,
            },
            SubTask {
                id: 3,
                description: "Task D".to_string(),
                files: vec!["src/d.rs".to_string(), "src/b.rs".to_string()],
                prompt: "do D".to_string(),
                can_parallel: true,
            },
        ];

        let groups = plan_parallel_groups(&tasks);

        // Check invariant: no group has tasks that share files
        for (gi, group) in groups.iter().enumerate() {
            for &i in group {
                for &j in group {
                    if i == j {
                        continue;
                    }
                    let set_i: HashSet<&str> = tasks[i].files.iter().map(|s| s.as_str()).collect();
                    let set_j: HashSet<&str> = tasks[j].files.iter().map(|s| s.as_str()).collect();
                    assert!(
                        set_i.is_disjoint(&set_j),
                        "tasks {i} and {j} in group {gi} overlap in files"
                    );
                }
            }
        }

        // Task 0 (a) and task 1 (b) can be together; task 2 (a) conflicts with task 0;
        // task 3 (d,b) conflicts with task 1 (b)
        // Expect at least 2 groups
        assert!(
            groups.len() >= 2,
            "mixed conflicts should yield multiple groups"
        );
    }
}
