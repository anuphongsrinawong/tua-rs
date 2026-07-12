//! 🧠 Learning Database — persistent memory of past compiler errors and their fixes.
//!
//! [`LearningDB`] records every successful fix the agent applies, indexed by
//! the Rust compiler error code (e.g. `E0502`, `E0382`). Over time this
//! builds a database of common patterns that can be queried to suggest
//! fixes when the same error recurs.
//!
//! # Database schema
//!
//! ```sql
//! CREATE TABLE fixes (
//!     id          INTEGER PRIMARY KEY AUTOINCREMENT,
//!     error_code  TEXT    NOT NULL,
//!     file_path   TEXT    NOT NULL,
//!     fix         TEXT    NOT NULL,
//!     created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
//! );
//! CREATE INDEX fixes_error_code_idx ON fixes(error_code);
//! ```

use rusqlite::{params, Connection, Result};

// ---------------------------------------------------------------------------
// LearningStats — aggregate statistics over the fix database
// ---------------------------------------------------------------------------

/// Aggregate statistics computed from the learning database.
#[derive(Debug, Clone, PartialEq)]
pub struct LearningStats {
    /// Total number of recorded fixes across all error codes.
    pub total_fixes: u64,
    /// Top error codes ranked by frequency, most common first.
    ///
    /// Each entry is `(error_code, count)`.
    pub top_errors: Vec<(String, u32)>,
}

// ---------------------------------------------------------------------------
// LearningDB — persistent database of fixes
// ---------------------------------------------------------------------------

/// A persistent database of compiler-error fixes, backed by SQLite.
///
/// # Example
///
/// ```rust
/// use tua_rs::learning::LearningDB;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let db = LearningDB::new(":memory:")?;
///
/// db.record_fix("E0502", "src/main.rs", "Clone the value before the borrow")?;
///
/// if let Some(suggestion) = db.suggest_fix("E0502") {
///     println!("Suggested fix: {suggestion}");
/// }
///
/// let stats = db.stats();
/// assert_eq!(stats.total_fixes, 1);
/// assert_eq!(stats.top_errors, vec![("E0502".into(), 1)]);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct LearningDB {
    conn: Connection,
}

impl LearningDB {
    /// Open (or create) the learning database at `path`.
    ///
    /// The database is created with the `fixes` table if it does not already
    /// exist, along with an index on `error_code` for fast lookups.
    ///
    /// # Errors
    ///
    /// Returns `rusqlite::Error` if the database cannot be opened or the
    /// schema initialisation fails.
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS fixes (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                error_code  TEXT    NOT NULL,
                file_path   TEXT    NOT NULL,
                fix         TEXT    NOT NULL,
                created_at  TEXT    NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS fixes_error_code_idx ON fixes(error_code);",
        )?;
        Ok(Self { conn })
    }

    /// Record a successful fix for the given compiler error.
    ///
    /// * `error_code` — the Rust compiler error code (e.g. `"E0502"`).
    /// * `file` — the file that was modified to apply the fix.
    /// * `fix` — a human-readable description of what was done.
    ///
    /// # Errors
    ///
    /// Returns `rusqlite::Error` if the insert query fails.
    pub fn record_fix(&self, error_code: &str, file: &str, fix: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO fixes (error_code, file_path, fix) VALUES (?1, ?2, ?3)",
            params![error_code, file, fix],
        )?;
        Ok(())
    }

    /// Suggest the most recent fix for a given error code, if any exist.
    ///
    /// Returns `Some(fix)` when at least one fix has been recorded for
    /// `error_code`. The most recently recorded fix is returned (based on
    /// `created_at`).
    ///
    /// Returns `None` when no fix has been recorded for the error code or
    /// the query fails (in which case the failure is silently swallowed —
    /// a missing suggestion is not critical).
    pub fn suggest_fix(&self, error_code: &str) -> Option<String> {
        self.conn
            .query_row(
                "SELECT fix FROM fixes WHERE error_code = ?1 ORDER BY id DESC LIMIT 1",
                params![error_code],
                |row| row.get(0),
            )
            .ok()
    }

    /// Compute aggregate statistics over all recorded fixes.
    ///
    /// Returns `total_fixes` (the total number of recorded fixes) and
    /// `top_errors` (the top error codes by frequency, ordered most
    /// common first). On query failure an empty stats object is returned
    /// (the caller can treat missing data gracefully).
    pub fn stats(&self) -> LearningStats {
        let total_fixes: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM fixes", [], |row| row.get(0))
            .unwrap_or(0);

        let top_errors = {
            let mut stmt = match self.conn.prepare(
                "SELECT error_code, COUNT(*) AS cnt FROM fixes \
                 GROUP BY error_code ORDER BY cnt DESC LIMIT 10",
            ) {
                Ok(s) => s,
                Err(_) => {
                    return LearningStats {
                        total_fixes,
                        top_errors: Vec::new(),
                    }
                }
            };

            let rows = match stmt.query_map([], |row| {
                let code: String = row.get(0)?;
                let count: u32 = row.get(1)?;
                Ok((code, count))
            }) {
                Ok(r) => r,
                Err(_) => {
                    return LearningStats {
                        total_fixes,
                        top_errors: Vec::new(),
                    }
                }
            };

            rows.filter_map(|r| r.ok()).collect()
        };

        LearningStats {
            total_fixes,
            top_errors,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_db_in_memory() {
        let db = LearningDB::new(":memory:").expect("failed to create in-memory DB");
        let stats = db.stats();
        assert_eq!(stats.total_fixes, 0);
        assert!(stats.top_errors.is_empty());
    }

    #[test]
    fn test_record_and_suggest_fix() {
        let db = LearningDB::new(":memory:").expect("failed to create DB");

        db.record_fix("E0502", "src/main.rs", "use clone() before the borrow")
            .expect("record_fix failed");

        let suggestion = db.suggest_fix("E0502");
        assert_eq!(suggestion.as_deref(), Some("use clone() before the borrow"));

        // Different error code → no suggestion
        assert!(db.suggest_fix("E0382").is_none());
    }

    #[test]
    fn test_multiple_fixes_same_error_returns_most_recent() {
        let db = LearningDB::new(":memory:").expect("failed to create DB");

        db.record_fix("E0502", "src/main.rs", "first fix").unwrap();
        db.record_fix("E0502", "src/lib.rs", "second fix").unwrap();
        db.record_fix("E0502", "src/lib.rs", "third fix").unwrap();

        // Most recent should be "third fix"
        let suggestion = db.suggest_fix("E0502");
        assert_eq!(suggestion.as_deref(), Some("third fix"));
    }

    #[test]
    fn test_stats() {
        let db = LearningDB::new(":memory:").expect("failed to create DB");

        db.record_fix("E0502", "a.rs", "fix a").unwrap();
        db.record_fix("E0502", "b.rs", "fix b").unwrap();
        db.record_fix("E0382", "c.rs", "fix c").unwrap();
        db.record_fix("E0596", "d.rs", "fix d").unwrap();
        db.record_fix("E0596", "e.rs", "fix e").unwrap();
        db.record_fix("E0596", "f.rs", "fix f").unwrap();

        let stats = db.stats();
        assert_eq!(stats.total_fixes, 6);

        // Top error should be E0596 (3 fixes), then E0502 (2), then E0382 (1)
        assert_eq!(stats.top_errors.len(), 3);
        assert_eq!(stats.top_errors[0], ("E0596".to_string(), 3));
        assert_eq!(stats.top_errors[1], ("E0502".to_string(), 2));
        assert_eq!(stats.top_errors[2], ("E0382".to_string(), 1));
    }

    #[test]
    fn test_empty_stats() {
        let db = LearningDB::new(":memory:").expect("failed to create DB");
        let stats = db.stats();
        assert_eq!(stats.total_fixes, 0);
        assert!(stats.top_errors.is_empty());
    }

    #[test]
    fn test_suggest_fix_no_records_returns_none() {
        let db = LearningDB::new(":memory:").expect("failed to create DB");
        assert!(db.suggest_fix("E0000").is_none());
    }

    #[test]
    fn test_record_empty_strings() {
        let db = LearningDB::new(":memory:").expect("failed to create DB");

        // Empty strings are valid — the schema doesn't forbid them.
        db.record_fix("", "", "")
            .expect("record_fix with empty strings failed");
        assert_eq!(db.stats().total_fixes, 1);
        assert_eq!(db.suggest_fix("").as_deref(), Some(""));
    }

    #[test]
    fn test_db_file_persistence() {
        let tmp = std::env::temp_dir().join(format!("test_learning_{}.db", std::process::id()));
        // Ensure clean state
        let _ = std::fs::remove_file(&tmp);

        // Write
        {
            let db = LearningDB::new(tmp.to_str().unwrap()).unwrap();
            db.record_fix("E0502", "main.rs", "persisted fix").unwrap();
        }

        // Re-open and read
        {
            let db = LearningDB::new(tmp.to_str().unwrap()).unwrap();
            assert_eq!(db.stats().total_fixes, 1);
            assert_eq!(db.suggest_fix("E0502").as_deref(), Some("persisted fix"));
        }

        // Cleanup
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_stats_returns_empty_vec_on_empty_db() {
        let db = LearningDB::new(":memory:").unwrap();
        let stats = db.stats();
        assert!(stats.top_errors.is_empty());
    }
}
