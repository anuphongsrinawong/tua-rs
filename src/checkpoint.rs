//! Session checkpointing via git.
//!
//! Provides functions to create and roll back git checkpoints,
//! enabling the agent to snapshot its work and undo changes.
//!
//! All functions use [`std::process::Command`] to invoke `git`. When
//! git is not installed or the current directory is not a git repo,
//! the functions return `false` / `None` rather than panicking.

use std::process::Command;

/// Check whether the given directory (or current working directory) is a
/// git repository.
///
/// Returns `false` when `git` is not installed, the directory is not a git
/// repo, or any other error occurs (e.g. permission denied).
///
/// # Examples
///
/// ```
/// use tua_rs::checkpoint::is_git_repo;
///
/// // Check the current directory.
/// let in_repo = is_git_repo(None);
/// println!("in a git repo: {in_repo}");
/// ```
pub fn is_git_repo(cwd: Option<&str>) -> bool {
    let mut cmd = Command::new("git");
    cmd.args(["rev-parse", "--git-dir"]);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Check whether the git working tree is clean (no uncommitted changes).
///
/// Returns `false` if we are not in a git repo, if `git` is not installed,
/// or if the working tree has any staged / unstaged / untracked changes
/// (as reported by `git status --porcelain`).
///
/// # Examples
///
/// ```
/// use tua_rs::checkpoint::git_is_clean;
///
/// if tua_rs::checkpoint::is_git_repo(None) {
///     println!("working tree is clean: {}", git_is_clean());
/// }
/// ```
pub fn git_is_clean() -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| o.status.success() && o.stdout.is_empty())
        .unwrap_or(false)
}

/// Create a git checkpoint by staging all changes and committing.
///
/// Equivalent to running:
/// ```text
/// git add -A
/// git commit -m "<message>"
/// ```
///
/// On success, returns `Some(commit_hash)` where `commit_hash` is the
/// full 40-character SHA-1 hex digest of the new commit.
///
/// Returns `None` if:
/// * Not in a git repository.
/// * There are no changes to commit (working tree already clean).
/// * The `git add` or `git commit` command fails for any other reason.
///
/// # Examples
///
/// ```
/// use tua_rs::checkpoint::checkpoint;
///
/// // Only succeeds inside a git repo with uncommitted changes.
/// if let Some(hash) = checkpoint("✨ agent checkpoint") {
///     println!("checkpoint created: {hash}");
/// }
/// ```
pub fn checkpoint(message: &str) -> Option<String> {
    if !is_git_repo(None) {
        return None;
    }

    // Stage all changes (tracked and untracked).
    let add_status = Command::new("git").args(["add", "-A"]).output().ok()?;
    if !add_status.status.success() {
        return None;
    }

    // Commit with the provided message.
    let commit_status = Command::new("git")
        .args(["commit", "-m", message])
        .output()
        .ok()?;
    if !commit_status.status.success() {
        return None;
    }

    // Return the hash of the newly created commit.
    last_commit_hash()
}

/// Roll back the most recent checkpoint via `git reset --hard HEAD~1`.
///
/// This discards the most recent commit and resets both the index and
/// the working tree to the state of `HEAD~1`. Any changes that were
/// part of the rolled-back commit are lost.
///
/// Returns `true` if the reset succeeded, `false` otherwise (not a git
/// repo, no prior commit, etc.).
///
/// # Examples
///
/// ```
/// use tua_rs::checkpoint::rollback;
///
/// // Only rolls back if there is at least one commit to undo.
/// if rollback() {
///     println!("rolled back to previous commit");
/// }
/// ```
pub fn rollback() -> bool {
    if !is_git_repo(None) {
        return false;
    }

    Command::new("git")
        .args(["reset", "--hard", "HEAD~1"])
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Return the full commit hash of `HEAD`.
///
/// Returns `None` if not in a git repo, if `HEAD` has no commits yet
/// (orphan branch / empty repo), or if the `git rev-parse HEAD` command
/// fails.
///
/// # Examples
///
/// ```
/// use tua_rs::checkpoint::last_commit_hash;
///
/// if let Some(hash) = last_commit_hash() {
///     println!("HEAD is at {hash}");
/// }
/// ```
pub fn last_commit_hash() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;

    if output.status.success() {
        let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if hash.is_empty() {
            None
        } else {
            Some(hash)
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Early-return from the enclosing test if we are NOT in a git repo.
    macro_rules! require_git_repo {
        () => {
            if !is_git_repo(None) {
                eprintln!("⚠️  not a git repo — skipping");
                return;
            }
        };
    }

    /// A global mutex that serialises tests which change the process CWD.
    ///
    /// Without this, parallel tests that call `std::env::set_current_dir`
    /// race against each other and sporadically fail.
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // ── is_git_repo ──────────────────────────────────────────────────────

    #[test]
    fn test_is_git_repo_in_source_tree() {
        let _lock = CWD_LOCK.lock().unwrap();
        assert!(
            is_git_repo(None),
            "expected the current directory to be a git repo"
        );
    }

    #[test]
    fn test_is_git_repo_outside_repo() {
        let tmp = std::env::temp_dir();
        // temp dir is unlikely to be a git repo.
        assert!(!is_git_repo(Some(tmp.to_str().unwrap())));
    }

    #[test]
    fn test_is_git_repo_nonexistent_dir() {
        // A non-existent directory cannot be a git repo.
        assert!(!is_git_repo(Some("/nonexistent/path/12345")));
    }

    // ── git_is_clean ─────────────────────────────────────────────────────

    #[test]
    fn test_git_is_clean_does_not_panic() {
        let _lock = CWD_LOCK.lock().unwrap();
        // Simply call the function — it should return a bool.
        let _clean = git_is_clean();
    }

    // ── last_commit_hash ─────────────────────────────────────────────────

    #[test]
    fn test_last_commit_hash_in_source_tree() {
        let _lock = CWD_LOCK.lock().unwrap();
        require_git_repo!();

        let hash = last_commit_hash();
        assert!(hash.is_some(), "expected a commit hash in this git repo");
        if let Some(h) = &hash {
            // SHA-1 hashes are 40 hex characters.
            assert_eq!(h.len(), 40, "commit hash should be 40 hex chars: {h}");
            assert!(
                h.chars().all(|c| c.is_ascii_hexdigit()),
                "hash should contain only hex digits: {h}"
            );
        }
    }

    #[test]
    fn test_last_commit_hash_outside_repo() {
        let _lock = CWD_LOCK.lock().unwrap();

        let original_cwd = std::env::current_dir().ok();
        let tmp = std::env::temp_dir();
        std::env::set_current_dir(&tmp).ok();

        let hash = last_commit_hash();
        assert!(hash.is_none(), "expected None outside a git repo");

        if let Some(dir) = original_cwd {
            std::env::set_current_dir(dir).ok();
        }
    }

    // ── checkpoint / rollback round-trip (isolated temp repo) ────────────

    #[test]
    fn test_checkpoint_and_rollback_roundtrip() {
        let _lock = CWD_LOCK.lock().unwrap();

        let dir = std::env::temp_dir().join("__tua_checkpoint_roundtrip__");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Initialise a new git repo.
        let init = Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(init.status.success(), "git init should succeed");

        // Set local git config so commits succeed without a global config.
        Command::new("git")
            .args(["config", "user.email", "checkpoint-test@tua.rs"])
            .current_dir(&dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Tua Checkpoint Test"])
            .current_dir(&dir)
            .output()
            .unwrap();

        // Create an initial commit so HEAD exists.
        std::fs::write(dir.join("README.md"), "# Checkpoint Test Repo").unwrap();
        let add_init = Command::new("git")
            .args(["add", "-A"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(add_init.status.success());
        let commit_init = Command::new("git")
            .args(["commit", "-m", "initial commit"])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(commit_init.status.success());

        // Save the original cwd, switch to temp repo.
        let original_cwd = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        // Confirm we are in a git repo.
        assert!(is_git_repo(None), "should be in the temp git repo");

        // Create a new file (uncommitted change).
        std::fs::write(dir.join("test.txt"), "checkpoint data").unwrap();

        // Create a checkpoint.
        let hash = checkpoint("test checkpoint: test.txt");
        assert!(hash.is_some(), "checkpoint should succeed in the temp repo");
        let commit_hash = hash.unwrap();
        assert_eq!(commit_hash.len(), 40, "commit hash should be 40 hex chars");
        assert!(
            commit_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be hex"
        );

        // Verify the file still exists after the commit.
        assert!(
            dir.join("test.txt").exists(),
            "file should exist after commit"
        );

        // Roll back the checkpoint.
        let rolled = rollback();
        assert!(rolled, "rollback should succeed");

        // After `git reset --hard HEAD~1`, the file added in the checkpoint
        // commit should be gone from the working tree.
        assert!(
            !dir.join("test.txt").exists(),
            "file should be removed after rollback"
        );

        // Restore cwd and clean up.
        std::env::set_current_dir(original_cwd).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_checkpoint_fails_when_not_a_repo() {
        let _lock = CWD_LOCK.lock().unwrap();

        let original_cwd = std::env::current_dir().ok();
        let tmp = std::env::temp_dir();
        std::env::set_current_dir(&tmp).ok();

        let result = checkpoint("should not work");
        assert!(
            result.is_none(),
            "checkpoint outside repo should return None"
        );

        if let Some(dir) = original_cwd {
            std::env::set_current_dir(dir).ok();
        }
    }

    #[test]
    fn test_rollback_fails_when_not_a_repo() {
        let _lock = CWD_LOCK.lock().unwrap();

        let original_cwd = std::env::current_dir().ok();
        let tmp = std::env::temp_dir();
        std::env::set_current_dir(&tmp).ok();

        assert!(!rollback(), "rollback outside repo should return false");

        if let Some(dir) = original_cwd {
            std::env::set_current_dir(dir).ok();
        }
    }

    #[test]
    fn test_checkpoint_no_changes() {
        let _lock = CWD_LOCK.lock().unwrap();

        // Only run this test if we are in a clean git repo (i.e. the
        // project itself has no uncommitted changes).
        if !is_git_repo(None) {
            eprintln!("⚠️  not a git repo — test_checkpoint_no_changes: skipped");
            return;
        }
        if !git_is_clean() {
            eprintln!("⚠️  dirty working tree — test_checkpoint_no_changes: skipped");
            return;
        }

        // When there are no changes to commit, checkpoint should return None
        // (git commit fails with "nothing to commit").
        let result = checkpoint("no-op checkpoint");
        assert!(
            result.is_none(),
            "checkpoint with no changes should return None"
        );
    }
}
