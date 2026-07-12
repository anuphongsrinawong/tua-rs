//! 🦀 Tua Orchestrator — Task decomposition, parallel group planning, and multi-agent dispatch.
//!
//! The orchestrator takes a high-level task description, splits it into independent subtasks,
//! analyzes which files each subtask touches, and spawns parallel or serial worker agents.
//!
//! # Features
//!
//! - **Task Dependency Graph** — Build a DAG from `TaskDependency` items and resolve
//!   execution levels via topological ordering.
//! - **Auto-Verify** — After each worker finishes, `cargo check` is run; on failure,
//!   the worker's files are rolled back with `git checkout`.
//! - **Cost Estimation** — Rough token/cost projection based on prompt length and
//!   per-provider pricing.
//! - **Self-Learning** — Success/failure patterns are saved to `~/.tua-rs/vault/learning/`
//!   for future reference.
//! - **Progress Bar** — `[█░░] X/Y` display during parallel execution.

use crate::context;
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// A single work item the orchestrator dispatches.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub id: usize,
    pub description: String,
    pub files: Vec<String>,
    pub prompt: String,
    pub can_parallel: bool,
}

/// Result of one worker — now includes auto-verify details.
#[derive(Debug)]
pub struct SubTaskResult {
    pub id: usize,
    pub description: String,
    pub success: bool,
    pub output: String,
    pub duration_secs: f64,
    /// Whether auto-verify (`cargo check`) passed after the worker finished.
    pub verify_passed: bool,
    /// Auto-verify output (stdout/stderr from `cargo check`).
    pub verify_output: String,
}

/// Overall orchestration result.
#[derive(Debug)]
pub struct OrchestrationResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<SubTaskResult>,
}

// ── Feature 1: Task Dependency Graph ──

/// A dependency edge: one task depends on another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDependency {
    pub task_id: String,
    pub depends_on: Vec<String>,
}

/// Build a dependency graph from `TaskDependency` items.
/// Returns levels where each level can execute in parallel.
/// Panics on circular dependencies.
pub fn build_dependency_graph(deps: &[TaskDependency]) -> Vec<Vec<String>> {
    if deps.is_empty() { return Vec::new(); }
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    for dep in deps {
        adj.entry(&dep.task_id).or_default();
        in_degree.entry(&dep.task_id).or_insert(0);
        for parent in &dep.depends_on {
            adj.entry(parent).or_default().push(&dep.task_id);
            *in_degree.entry(&dep.task_id).or_insert(0) += 1;
            in_degree.entry(parent).or_insert(0);
        }
    }
    let mut depth: HashMap<&str, usize> = HashMap::new();
    let mut queue: Vec<&str> = in_degree.iter()
        .filter(|(_, &deg)| deg == 0).map(|(&id, _)| id).collect();
    let mut order: Vec<&str> = Vec::new();
    while let Some(node) = queue.pop() {
        order.push(node);
        let cd = *depth.get(node).unwrap_or(&0);
        for &next in adj.get(node).unwrap_or(&Vec::new()) {
            let e = in_degree.get_mut(next).expect("missing in-degree");
            *e = e.saturating_sub(1);
            let nd = depth.entry(next).or_insert(0);
            *nd = (*nd).max(cd + 1);
            if *e == 0 { queue.push(next); }
        }
    }
    if order.len() != in_degree.len() {
        panic!("circular dependency detected — {} tasks remain unvisited",
            in_degree.len() - order.len());
    }
    let max_depth = depth.values().copied().max().unwrap_or(0);
    let mut levels: Vec<Vec<String>> = vec![Vec::new(); max_depth + 1];
    for id in &order {
        let d = *depth.get(id).unwrap_or(&0);
        levels[d].push(id.to_string());
    }
    levels.retain(|l| !l.is_empty());
    levels
}

/// Format a dependency graph as a visual tree.
pub fn format_dependency_tree(deps: &[TaskDependency]) -> String {
    if deps.is_empty() { return "(empty)".to_string(); }
    let _l = build_dependency_graph(deps);
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
    for dep in deps {
        children.entry(&dep.task_id).or_default();
        for p in &dep.depends_on {
            children.entry(p).or_default().push(&dep.task_id);
            parents.entry(&dep.task_id).or_default().push(p);
        }
    }
    let roots: Vec<&str> = deps.iter()
        .filter(|d| d.depends_on.is_empty())
        .map(|d| d.task_id.as_str()).collect();
    let mut out = String::new();
    for (i, root) in roots.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        rn(root, &children, &parents, "", &mut out);
    }
    out
}

fn rn(id: &str, children: &HashMap<&str, Vec<&str>>, parents: &HashMap<&str, Vec<&str>>, prefix: &str, out: &mut String) {
    let after = parents.get(id).map(|p| p.join(", ")).unwrap_or_else(|| "none".to_string());
    out.push_str(&format!("{prefix}{id} (after: {after})\n"));
    let kids = children.get(id).cloned().unwrap_or_default();
    for (i, child) in kids.iter().enumerate() {
        let last = i == kids.len() - 1;
        let conn = if last { "└── " } else { "├── " };
        let cp = if last { format!("{prefix}    ") } else { format!("{prefix}│   ") };
        out.push_str(&format!("{prefix}{conn}"));
        rn2(child, children, parents, &cp, out);
    }
}

fn rn2(id: &str, children: &HashMap<&str, Vec<&str>>, parents: &HashMap<&str, Vec<&str>>, prefix: &str, out: &mut String) {
    let after = parents.get(id).map(|p| p.join(", ")).unwrap_or_else(|| "none".to_string());
    out.push_str(&format!("{id} (after: {after})\n"));
    let kids = children.get(id).cloned().unwrap_or_default();
    for (i, child) in kids.iter().enumerate() {
        let last = i == kids.len() - 1;
        let conn = if last { "└── " } else { "├── " };
        let cp = if last { format!("{prefix}    ") } else { format!("{prefix}│   ") };
        out.push_str(&format!("{prefix}{conn}"));
        rn2(child, children, parents, &cp, out);
    }
}

// ── Feature 2: Auto-Verify (cargo check + git checkout rollback) ──

fn auto_verify(task: &SubTask) -> (bool, String) {
    let check = Command::new("cargo").args(["check", "--message-format=short"]).output();
    match check {
        Ok(out) => {
            if out.status.success() {
                (true, "cargo check passed".to_string())
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                let mut output = format!("{stdout}\n{stderr}");
                let mut rbs: Vec<String> = Vec::new();
                for file in &task.files {
                    let rb = Command::new("git").args(["checkout", "--", file]).output();
                    match rb {
                        Ok(r) if r.status.success() => rbs.push(format!("rolled back: {file}")),
                        Ok(r) => rbs.push(format!("rollback fail {}: {}", file, String::from_utf8_lossy(&r.stderr))),
                        Err(e) => rbs.push(format!("rollback err {file}: {e}")),
                    }
                }
                output.push_str("\n── Rollback ──\n");
                output.push_str(&rbs.join("\n"));
                (false, output)
            }
        }
        Err(e) => (false, format!("cargo check spawn error: {e}")),
    }
}

// ── Feature 3: Cost Estimation (prompt_length/4 * provider_pricing) ──

#[derive(Debug, Clone)]
pub struct CostEstimate {
    pub prompt_chars: usize,
    pub estimated_tokens: usize,
    pub price_per_1k_tokens_cents: f64,
    pub estimated_cost_cents: f64,
}

fn provider_price_per_1k_tokens(provider: &str) -> f64 {
    match provider.to_lowercase().as_str() {
        "openai" | "gpt-4o" | "gpt-4" => 0.5,
        "anthropic" | "claude" | "claude-sonnet" => 0.8,
        "ollama" | "local" => 0.0,
        _ => 0.5,
    }
}

/// Estimate cost using heuristic: 1 token ≈ 4 characters.
pub fn estimate_cost(prompt: &str, provider: &str) -> CostEstimate {
    let chars = prompt.len();
    let tokens = chars / 4;
    let ppk = provider_price_per_1k_tokens(provider);
    let cost = (tokens as f64 / 1000.0) * ppk;
    CostEstimate { prompt_chars: chars, estimated_tokens: tokens, price_per_1k_tokens_cents: ppk, estimated_cost_cents: cost }
}

// ── Feature 4: Self-Learning → vault/learning/ ──

/// Save success/failure patterns to ~/.tua-rs/vault/learning/.
pub fn save_learning(task_desc: &str, result: &SubTaskResult) -> std::io::Result<()> {
    let home = std::env::var("HOME").ok()
        .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
        .unwrap_or_else(|| "/tmp".to_string());
    let vld = format!("{home}/.tua-rs/vault/learning");
    std::fs::create_dir_all(&vld)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
    let st = if result.success { "success" } else { "failure" };
    let sd: String = task_desc.chars().take(40)
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let fnm = format!("{vld}/{now}_{st}_{sd}.md");
    let content = format!(
        "# Learning Entry — {status}\n**Timestamp**: {ts}\n**Task**: {td}\n**Duration**: {dur:.1}s\n**Verify**: {vfy}\n\n## Output\n```\n{out}\n```\n\n## Verify\n```\n{vo}\n```\n",
        status = if result.success { "SUCCESS" } else { "FAILURE" },
        ts = now, td = task_desc, dur = result.duration_secs,
        vfy = if result.verify_passed { "passed" } else { "failed" },
        out = result.output, vo = result.verify_output,
    );
    std::fs::write(&fnm, content)?;
    let lp = format!("{vld}/_learning_log.md");
    let le = format!("| {ts} | {st} | {sd} | {dur:.1}s | vfy:{vf} |\n",
        ts = now, st = st, sd = sd, dur = result.duration_secs,
        vf = if result.verify_passed { "y" } else { "n" });
    if std::path::Path::new(&lp).exists() {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(&lp)?;
        f.write_all(le.as_bytes())?;
    } else {
        let h = "| Timestamp | Status | Task | Duration | Verify |\n| --- | --- | --- | --- | --- |\n";
        std::fs::write(&lp, format!("{h}{le}"))?;
    }
    Ok(())
}

// ── Feature 5: Progress Bar [█░░] X/Y ──

/// Render an ASCII progress bar: `[██████░░░░] 6/10`
pub fn render_progress_bar(current: usize, total: usize, width: usize) -> String {
    let et = if total == 0 { 1 } else { total };
    let filled = (current * width) / et;
    let empty = width.saturating_sub(filled);
    format!("[{}] {}/{}", "█".repeat(filled) + &"░".repeat(empty), current, total)
}

// ── Internal helpers ──

const KNOWN_MODULES: &[&str] = &[
    "agent","checkpoint","completion","config","dashboard","learning","orchestrator",
    "parallel","profiles","prompts","providers","review","session","skills","tools",
    "tui","wasm","workspace",
];

fn detect_modules(description: &str) -> Vec<String> {
    let lower = description.to_lowercase();
    let mut modules: Vec<String> = Vec::new();
    if let Some(count) = lower.split_whitespace().filter_map(|w| w.parse::<usize>().ok()).next() {
        if lower.contains("modules") || lower.contains("module") {
            return KNOWN_MODULES.iter().take(count.min(KNOWN_MODULES.len())).map(|&m| m.to_string()).collect();
        }
    }
    for word in lower.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_');
        if let Some(stripped) = cleaned.strip_suffix(".rs") {
            if !stripped.is_empty() && stripped.chars().all(|c| c.is_alphanumeric() || c == '_') {
                modules.push(stripped.to_string());
            }
        }
    }
    for &module in KNOWN_MODULES {
        let ml = module.to_lowercase();
        let pat = format!(" {ml} ");
        if (lower.contains(&pat) || lower.starts_with(&ml)) && !modules.contains(&module.to_string()) {
            modules.push(module.to_string());
        }
    }
    modules
}

pub fn split_tasks(description: &str) -> Vec<SubTask> {
    let modules = detect_modules(description);
    if modules.is_empty() {
        return vec![SubTask { id: 0, description: description.to_string(), files: Vec::new(), prompt: description.to_string(), can_parallel: false }];
    }
    let is_testing = description.to_lowercase().contains("test");
    let is_doc = description.to_lowercase().contains("doc");
    let is_fix = description.to_lowercase().contains("fix");
    modules.into_iter().enumerate().map(|(i, module)| {
        let action = if is_testing { "Write tests for" }
            else if is_doc { "Document" }
            else if is_fix { "Fix issues in" }
            else { "Implement" };
        let files = vec![format!("src/{module}.rs")];
        SubTask { id: i, description: format!("{action} {module}"), files,
            prompt: format!("{action} the `{module}` module in the tua-rs project. Focus on the `src/{module}.rs` file. Ensure code is idiomatic, safe, and well-documented."),
            can_parallel: true,
        }
    }).collect()
}

pub fn plan_parallel_groups(tasks: &[SubTask]) -> Vec<Vec<usize>> {
    if tasks.is_empty() { return Vec::new(); }
    let file_sets: Vec<HashSet<&str>> = tasks.iter()
        .map(|t| t.files.iter().map(|f| f.as_str()).collect()).collect();
    let n = tasks.len();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for task_idx in 0..n {
        let task_files = &file_sets[task_idx];
        let mut placed = false;
        for group in groups.iter_mut() {
            let conflicts = group.iter().any(|&oi| {
                !task_files.is_disjoint(&file_sets[oi])
            });
            if !conflicts { group.push(task_idx); placed = true; break; }
        }
        if !placed { groups.push(vec![task_idx]); }
    }
    groups
}

fn run_subtask(task: &SubTask) -> SubTaskResult {
    let start = Instant::now();
    let home = std::env::var("HOME").ok()
        .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().to_string()))
        .unwrap_or_else(|| "/tmp".to_string());
    let tua_project = format!("{home}/tua-agent");
    let vault = format!("{home}/.tua-rs/vault");
    let project_md = std::fs::read_to_string("PROJECT.md").unwrap_or_default();
    let vault_entries: Vec<String> = [
        "rules/rust-do.md","rules/rust-dont.md","architecture/PROJECT_ARCHITECTURE.md","INDEX.md","TECHNIQUES.md",
    ].iter().filter_map(|r| std::fs::read_to_string(format!("{vault}/{r}")).ok()).collect();
    let error_matches: Vec<String> = if let Ok(ed) = std::fs::read_dir(format!("{vault}/errors")) {
        ed.flatten().filter(|e| {
            let fnm = e.path().file_stem().unwrap_or_default().to_string_lossy().into_owned();
            task.description.contains(&fnm) || task.prompt.contains(&fnm)
        }).filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect()
    } else { vec![] };
    let sessions: Vec<String> = if let Ok(sd) = std::fs::read_dir(format!("{vault}/sessions")) {
        let mut files: Vec<_> = sd.flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "md")).collect();
        files.sort_by_key(|e| e.file_name());
        files.iter().rev().take(3).filter_map(|e| std::fs::read_to_string(e.path()).ok()).collect()
    } else { vec![] };
    let ctx = context::compress_for_worker(&task.prompt, &project_md, &vault_entries, &error_matches, &sessions, 50_000);
    let prompt = format!("{ctx}\n\nTask:\n{}", task.prompt);
    let output = Command::new("uv").args(["run", "--project", &tua_project, "tua", "-p", &prompt]).output();
    let (success, output_str) = match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            (out.status.success(), stdout)
        }
        Err(e) => (false, format!("Failed to spawn worker: {e}")),
    };
    let (verify_passed, verify_output) = if success {
        auto_verify(task)
    } else {
        let (vp, vo) = auto_verify(task);
        (vp, format!("Worker failed before verify:\n{vo}"))
    };
    let duration = start.elapsed().as_secs_f64();
    SubTaskResult { id: task.id, description: task.description.clone(), success: success && verify_passed, output: output_str, duration_secs: duration, verify_passed, verify_output }
}

pub fn orchestrate(description: &str, max_parallel: usize) -> OrchestrationResult {
    let tasks = split_tasks(description);
    let groups = plan_parallel_groups(&tasks);
    let total = tasks.len();
    let mut results: Vec<SubTaskResult> = Vec::with_capacity(total);
    let progress = Arc::new(AtomicUsize::new(0));
    for group in &groups {
        for chunk in group.chunks(max_parallel) {
            let mut handles: Vec<(usize, std::thread::JoinHandle<SubTaskResult>)> = Vec::new();
            for &task_idx in chunk {
                let task = tasks[task_idx].clone();
                let p = Arc::clone(&progress);
                let handle = std::thread::spawn(move || {
                    let r = run_subtask(&task);
                    p.fetch_add(1, Ordering::SeqCst);
                    r
                });
                handles.push((task_idx, handle));
            }
            for (task_idx, handle) in handles {
                match handle.join() {
                    Ok(r) => results.push(r),
                    Err(_) => {
                        let task = &tasks[task_idx];
                        results.push(SubTaskResult { id: task.id, description: task.description.clone(), success: false, output: "Worker thread panicked".to_string(), duration_secs: 0.0, verify_passed: false, verify_output: String::new() });
                        progress.fetch_add(1, Ordering::SeqCst);
                    }
                }
            }
        }
    }
    results.sort_by_key(|r| r.id);
    let passed = results.iter().filter(|r| r.success).count();
    OrchestrationResult { total, passed, failed: total - passed, results }
}

pub fn plan_and_run(description: &str, max_parallel: usize) -> OrchestrationResult {
    println!("🦀 Tua Orchestrator — Analyzing task...");
    let tasks = split_tasks(description);
    let groups = plan_parallel_groups(&tasks);
    println!("📋 Found {} subtasks:", tasks.len());
    let provider = "openai";
    let mut total_cost = 0.0_f64;
    for task in &tasks {
        let ps = if task.can_parallel { "PARALLEL ✅" } else { "SERIAL ⚠️" };
        let files = task.files.join(", ");
        let cost = estimate_cost(&task.prompt, provider);
        total_cost += cost.estimated_cost_cents;
        println!("   [{}] {} → {} ({}) — ~{} tk · ${:.4}",
            task.id + 1, task.description, ps, files, cost.estimated_tokens, cost.estimated_cost_cents / 100.0);
    }
    if total_cost > 0.0 {
        println!("💰 Estimated total: ~${:.4} (~{} tokens)",
            total_cost / 100.0, tasks.iter().map(|t| t.prompt.len() / 4).sum::<usize>());
    }
    if tasks.len() > 1 && groups.len() > 1 {
        println!("\n🔗 Execution plan:");
        for (gi, group) in groups.iter().enumerate() {
            let idx: Vec<String> = group.iter().map(|&i| format!("#{}", i + 1)).collect();
            println!("  {}Level {gi}: [{}]", "  ".repeat(gi), idx.join(", "));
        }
    }
    println!();
    println!("🚀 {} workers in {} group(s) — max {} parallel. Run? [Y/n]: ",
        tasks.len(), groups.len(), max_parallel);
    { use std::io::{self, Write}; let _ = io::stdout().flush(); let mut input = String::new(); let _ = io::stdin().read_line(&mut input); if input.trim().to_lowercase() == "n" || input.trim().to_lowercase() == "no" { println!("❌ Cancelled."); return OrchestrationResult { total: tasks.len(), passed: 0, failed: 0, results: vec![] }; } }
    println!();
    let total = tasks.len();
    let progress = Arc::new(AtomicUsize::new(0));
    let pc = Arc::clone(&progress);
    let pd = Arc::new(AtomicUsize::new(0));
    let pdc = Arc::clone(&pd);
    let desc = description.to_string();
    let handle = std::thread::spawn(move || {
        let r = orchestrate(&desc, max_parallel);
        pc.store(r.total, Ordering::SeqCst);
        pdc.store(1, Ordering::SeqCst);
        r
    });
    { use std::io::Write; while pd.load(Ordering::SeqCst) == 0 {
        let c = progress.load(Ordering::SeqCst);
        let bar = render_progress_bar(c, total, 20);
        print!("\r   {bar}"); let _ = std::io::stdout().flush();
        if c >= total { break; } std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let c = progress.load(Ordering::SeqCst);
    println!("\r   {}", render_progress_bar(c, total, 20)); }
    let result = handle.join().unwrap_or_else(|_| OrchestrationResult { total, passed: 0, failed: total, results: vec![] });
    println!();
    for (i, r) in result.results.iter().enumerate() {
        let st = if r.success { "✅" } else if r.verify_passed { "⚠️" } else { "❌" };
        let vm = if r.verify_passed { "✓" } else { "✗" };
        println!("   [{}/{}] 🦀 {} .......... {} (verify:{vm}, {:.0}s)",
            i + 1, result.total, r.description, st, r.duration_secs);
    }
    println!("\n📊 Result: {} passed, {} failed ({} total)", result.passed, result.failed, result.total);
    let mut ls = 0;
    for r in &result.results { if save_learning(&r.description, r).is_ok() { ls += 1; } }
    if ls > 0 { println!("🧠 Saved {ls} learning entries to vault/learning/"); }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn test_dep_chain() {
        let deps = vec![TD("A", &[]), TD("B", &["A"]), TD("C", &["B"])];
        let lvls = build_dependency_graph(&deps);
        assert_eq!(lvls.len(), 3);
        assert_eq!(lvls[0], vec!["A"]); assert_eq!(lvls[1], vec!["B"]); assert_eq!(lvls[2], vec!["C"]);
    }
    #[test] fn test_dep_siblings() {
        let deps = vec![TD("A", &[]), TD("B", &["A"]), TD("C", &["A"])];
        let lvls = build_dependency_graph(&deps);
        assert_eq!(lvls.len(), 2); assert_eq!(lvls[0], vec!["A"]);
    }
    #[test] fn test_dep_diamond() {
        let deps = vec![TD("A", &[]), TD("B", &["A"]), TD("C", &["A"]), TD("D", &["B","C"])];
        let lvls = build_dependency_graph(&deps);
        assert_eq!(lvls.len(), 3);
    }
    #[test] fn test_dep_no_deps() {
        let lvls = build_dependency_graph(&[TD("X",&[]),TD("Y",&[]),TD("Z",&[])]);
        assert_eq!(lvls.len(), 1); assert_eq!(lvls[0].len(), 3);
    }
    #[test] fn test_dep_empty() { assert!(build_dependency_graph(&[]).is_empty()); }
    #[test] #[should_panic(expected = "circular")]
    fn test_dep_cycle() { build_dependency_graph(&[TD("A",&["B"]),TD("B",&["A"])]); }
    #[test] fn test_format_tree() {
        let t = format_dependency_tree(&[TD("A",&[]),TD("B",&["A"]),TD("C",&["A"])]);
        assert!(t.contains("A")); assert!(t.contains("B")); assert!(t.contains("C"));
    }
    #[test] fn test_format_tree_empty() { assert_eq!(format_dependency_tree(&[]), "(empty)"); }

    #[test] fn test_auto_verify_clean() {
        let task = SubTask { id:0, description:"t".into(), files:vec!["src/orchestrator.rs".into()], prompt:"t".into(), can_parallel:false };
        let (p, o) = auto_verify(&task); assert!(p, "verify should pass: {o}");
    }
    #[test] fn test_auto_verify_nonexistent() {
        let task = SubTask { id:0, description:"t".into(), files:vec!["src/nope_xyz.rs".into()], prompt:"t".into(), can_parallel:false };
        let (_, o) = auto_verify(&task); assert!(!o.is_empty());
    }

    #[test] fn test_cost_openai() {
        let e = estimate_cost("Hello, world!", "openai");
        assert_eq!(e.prompt_chars, 13); assert_eq!(e.estimated_tokens, 3);
    }
    #[test] fn test_cost_anthropic() {
        let e = estimate_cost("A".repeat(4000).as_str(), "anthropic");
        assert_eq!(e.estimated_tokens, 1000);
    }
    #[test] fn test_cost_ollama() { assert_eq!(estimate_cost("p", "ollama").estimated_cost_cents, 0.0); }
    #[test] fn test_cost_unknown() { assert_eq!(estimate_cost("t","unk").price_per_1k_tokens_cents, 0.5); }
    #[test] fn test_cost_empty() {
        let e = estimate_cost("", "openai"); assert_eq!(e.prompt_chars, 0);
    }
    #[test] fn test_price_per_1k() {
        assert_eq!(provider_price_per_1k_tokens("openai"), 0.5);
        assert_eq!(provider_price_per_1k_tokens("anthropic"), 0.8);
        assert_eq!(provider_price_per_1k_tokens("ollama"), 0.0);
    }

    #[test] fn test_save_learning() {
        let r = SubTaskResult { id:0, description:"tl".into(), success:true, output:"ok".into(), duration_secs:1.5, verify_passed:true, verify_output:"pass".into() };
        assert!(save_learning("Test", &r).is_ok());
    }
    #[test] fn test_save_learning_fail() {
        let r = SubTaskResult { id:1, description:"tf".into(), success:false, output:"err".into(), duration_secs:0.5, verify_passed:false, verify_output:"fail".into() };
        assert!(save_learning("Test fail", &r).is_ok());
    }

    #[test] fn test_bar_empty() { assert_eq!(render_progress_bar(0,10,10), "[░░░░░░░░░░] 0/10"); }
    #[test] fn test_bar_full() { assert_eq!(render_progress_bar(10,10,10), "[██████████] 10/10"); }
    #[test] fn test_bar_half() { assert_eq!(render_progress_bar(5,10,10), "[█████░░░░░] 5/10"); }
    #[test] fn test_bar_zero() { assert_eq!(render_progress_bar(0,0,10), "[░░░░░░░░░░] 0/0"); }
    #[test] fn test_bar_width() { assert_eq!(render_progress_bar(3,6,6), "[███░░░] 3/6"); }
    #[test] fn test_bar_round() { assert_eq!(render_progress_bar(1,3,10), "[███░░░░░░░] 1/3"); }

    #[test] fn test_split_modules() {
        let t = split_tasks("add tests to parallel.rs and dashboard.rs");
        assert_eq!(t.len(), 2);
    }
    #[test] fn test_split_single() {
        let t = split_tasks("fix the parallel module"); assert!(!t.is_empty());
    }
    #[test] fn test_groups_no_conflict() {
        let t = vec![st(0,"a",true), st(1,"b",true), st(2,"c",true)];
        assert_eq!(plan_parallel_groups(&t).len(), 1);
    }
    #[test] fn test_groups_conflict() {
        let t = vec![st_file(0,"shared"), st_file(1,"shared")];
        assert!(plan_parallel_groups(&t).len() >= 2);
    }
    #[test] fn test_groups_mixed() {
        let t = vec![st_file(0,"a"), st_file(1,"b"), st_file(2,"a"), st_file2(3,"d","b")];
        let g = plan_parallel_groups(&t);
        for (_gi, grp) in g.iter().enumerate() {
            for &i in grp { for &j in grp { if i==j {continue;}
                let si: HashSet<&str> = t[i].files.iter().map(|s| s.as_str()).collect();
                let sj: HashSet<&str> = t[j].files.iter().map(|s| s.as_str()).collect();
                assert!(si.is_disjoint(&sj));
            }}
        }
        assert!(g.len() >= 2);
    }
    #[test] fn test_result_verify() {
        let r = SubTaskResult { id:0, description:"t".into(), success:true, output:"o".into(), duration_secs:0.1, verify_passed:true, verify_output:"c".into() };
        assert!(r.verify_passed);
    }
    #[test] fn test_result_struct() {
        let e = estimate_cost("hello world", "openai");
        assert!(e.prompt_chars > 0); assert!(e.estimated_tokens > 0);
    }

    // ── helpers ──
    #[allow(non_snake_case)]
    fn TD(id: &str, deps: &[&str]) -> TaskDependency {
        TaskDependency { task_id: id.into(), depends_on: deps.iter().map(|s| s.to_string()).collect() }
    }
    fn st(id: usize, file: &str, cp: bool) -> SubTask {
        SubTask { id, description: file.into(), files: vec![format!("src/{file}.rs")], prompt: "".into(), can_parallel: cp }
    }
    fn st_file(id: usize, file: &str) -> SubTask { st(id, file, false) }
    fn st_file2(id: usize, f1: &str, f2: &str) -> SubTask {
        SubTask { id, description: format!("{f1}+{f2}"), files: vec![format!("src/{f1}.rs"), format!("src/{f2}.rs")], prompt: "".into(), can_parallel: true }
    }
}
