# Build Tua Orchestrator — Agent แม่สั่ง Agent ลูก

## New file: src/orchestrator.rs

## Architecture

The orchestrator takes a high-level task description, splits it into independent subtasks, 
analyzes which files each subtask touches, and spawns parallel/serial worker agents.

## Data structures

```rust
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

/// A single work item the orchestrator dispatches.
#[derive(Debug, Clone)]
pub struct SubTask {
    pub id: usize,
    pub description: String,
    pub files: Vec<String>,       // files this task modifies
    pub prompt: String,           // exact prompt for tua worker
    pub can_parallel: bool,       // true if files don't conflict
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
```

## Core functions

### 1. `split_tasks(description: &str) -> Vec<SubTask>`
Analyzes the task description and splits into subtasks by detecting:
- Module names mentioned (e.g. "parallel.rs", "dashboard.rs") → creates per-module subtask
- "add tests to X modules" → splits per module
- Counts modules and creates one subtask per module
- Each subtask gets a clear prompt and file list

### 2. `plan_parallel_groups(tasks: &[SubTask]) -> Vec<Vec<usize>>`  
Groups tasks that can run simultaneously (no overlapping files).
- Tasks with disjoint file sets → same group (can run parallel)
- Tasks modifying same file → different groups (must run serial)

### 3. `orchestrate(description: &str, max_parallel: usize) -> OrchestrationResult`
Main entry point:
1. Call split_tasks to break down the task
2. Print task plan to user
3. For each parallel group:
   - Spawn N child agents via `Command::new("uv").args(["run", "--project", HOME/tua-agent, "tua", "-p", prompt])`
   - Wait for all in group to complete
   - Collect results
4. Return summarized result

### 4. CLI output format
When run, the orchestrator prints a nice plan before executing:
```
🦀 Tua Orchestrator — Analyzing task...
📋 Found N subtasks:
   [1] task desc → PARALLEL ✅ (file1.rs)
   [2] task desc → SERIAL ⚠️ (shared file)
   ...
🚀 Running N workers...
   [1/N] 🦀 task .......... ✅ 10 tests (45s)
   [2/N] 🦀 task .......... ✅ done (32s)
```

## Integration

### src/lib.rs — add:
```rust
pub mod orchestrator;
```

### src/main.rs — add command:
```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing ...
    /// Orchestrate multiple agent workers
    Orchestrate {
        /// Task description
        task: String,
        /// Max parallel workers [default: 4]
        #[arg(long, default_value = "4")]
        parallel: usize,
    },
}
```

Handle in main:
```rust
Some(Commands::Orchestrate { task, parallel }) => {
    println!("🦀 Tua Orchestrator");
    let result = tua_rs::orchestrator::plan_and_run(&task, parallel);
    // Print results
}
```

## Tests (in orchestrator.rs)
- test_split_tasks_detects_modules
- test_split_tasks_single_task
- test_plan_parallel_groups_no_conflict
- test_plan_parallel_groups_same_file_serial
- test_plan_parallel_groups_mixed

## Implementation
1. Read src/main.rs first to understand CLI structure
2. Create src/orchestrator.rs with all the code
3. Add `pub mod orchestrator;` to src/lib.rs
4. Add Orchestrate command to src/main.rs
5. Run `cargo build` → fix errors
6. Run `cargo test --lib` → verify all pass
7. Run `cargo clippy` → 0 warnings

CRITICAL: Use Write tool for new file. Use patch/edit for existing files.
