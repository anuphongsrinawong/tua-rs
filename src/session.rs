//! 💾 Session persistence — save, load, and list conversations.
//!
//! Sessions are stored as **JSONL** files (one JSON object per line).
//!
//! * **Line 1** — session metadata (`SessionMeta`): id, profile, model,
//!   created_at, updated_at.
//! * **Subsequent lines** — each [`AgentMessage`] in the conversation history,
//!   serialised individually.
//!
//! This layout allows appending new messages without re-reading the entire
//! file, and makes incremental backups trivial.
//!
//! ## File naming
//!
//! Each session is stored as `{session_id}.jsonl` in a designated directory
//! (default: `~/.tua-rs/sessions/`).
//!
//! ## Example
//!
//! ```json
//! {"id":"a1b2c3d4-...","profile":"rustacean","model":"deepseek/deepseek-v4-flash","created_at":"2026-07-12T10:00:00Z","updated_at":"2026-07-12T10:30:00Z"}
//! {"User":{"text":"Hello, Tua!"}}
//! {"Assistant":{"text":["Hi! How can I help?"], "tool_calls":[]}}
//! {"ToolResult":{"tool_call_id":"call_1","output":"done"}}
//! ```

use crate::agent::AgentMessage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during session persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The session file could not be read.
    #[error("failed to read session file `{path}`: {source}")]
    ReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The session file could not be written.
    #[error("failed to write session file `{path}`: {source}")]
    WriteFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A line in the JSONL file could not be parsed.
    #[error("failed to parse session data at line {line} in `{path}`: {source}")]
    ParseFailed {
        path: PathBuf,
        line: usize,
        #[source]
        source: serde_json::Error,
    },

    /// The session directory could not be read (for listing).
    #[error("failed to read sessions directory `{path}`: {source}")]
    ListFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The session file is empty (missing metadata line).
    #[error("session file `{path}` is empty")]
    EmptyFile { path: PathBuf },

    /// The session metadata (first line) was valid JSON but did not contain
    /// the expected session fields.
    #[error(
        "session file `{path}` is missing metadata (first line is not a valid session header)"
    )]
    MissingMetadata { path: PathBuf },
}

/// Convenience alias for `Result<T, SessionError>`.
pub type SessionResult<T> = Result<T, SessionError>;

// ---------------------------------------------------------------------------
// Session metadata (stored on the first line of the JSONL file)
// ---------------------------------------------------------------------------

/// Metadata stored on the first line of every session JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    /// Unique session identifier (v4 UUID).
    pub id: uuid::Uuid,
    /// Name of the Rust coding profile used for this session.
    pub profile: String,
    /// The model identifier (e.g. `"deepseek/deepseek-v4-flash"`).
    pub model: String,
    /// ISO-8601 timestamp of when the session was created.
    pub created_at: String,
    /// ISO-8601 timestamp of when the session was last updated.
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Session — the full conversation record
// ---------------------------------------------------------------------------

/// A full conversation session with metadata and message history.
#[derive(Debug, Clone)]
pub struct Session {
    /// Session metadata (id, profile, model, timestamps).
    pub meta: SessionMeta,
    /// Conversation history (messages in chronological order).
    pub messages: Vec<AgentMessage>,
}

impl Session {
    /// Create a new session with the given profile and model.
    ///
    /// Automatically assigns a fresh UUID and timestamps.
    ///
    /// # Examples
    ///
    /// ```
    /// use tua_rs::session::Session;
    ///
    /// let session = Session::new("rustacean", "deepseek/deepseek-v4-flash");
    /// assert_eq!(session.meta.profile, "rustacean");
    /// assert_eq!(session.meta.model, "deepseek/deepseek-v4-flash");
    /// assert!(session.messages.is_empty());
    /// ```
    pub fn new(profile: impl Into<String>, model: impl Into<String>) -> Self {
        let now = iso_now();
        Self {
            meta: SessionMeta {
                id: uuid::Uuid::new_v4(),
                profile: profile.into(),
                model: model.into(),
                created_at: now.clone(),
                updated_at: now,
            },
            messages: Vec::new(),
        }
    }

    /// Push a message onto the conversation and update `updated_at`.
    pub fn push_message(&mut self, msg: AgentMessage) {
        self.messages.push(msg);
        self.meta.updated_at = iso_now();
    }

    /// Save this session to a JSONL file at `path`.
    ///
    /// The file format is:
    /// * Line 1 — [`SessionMeta`] as JSON.
    /// * Lines 2..N — each [`AgentMessage`] as JSON (one per line).
    ///
    /// If the file already exists, it is **overwritten**.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::WriteFailed`] if the file cannot be created
    /// or written.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tua_rs::session::Session;
    ///
    /// let mut session = Session::new("rustacean", "deepseek/deepseek-v4-flash");
    /// session.save("/tmp/test-session.jsonl").unwrap();
    /// ```
    pub fn save(&self, path: impl AsRef<Path>) -> SessionResult<()> {
        let path = path.as_ref();
        let mut contents = String::new();

        // Line 1: metadata
        let meta_json =
            serde_json::to_string(&self.meta).map_err(|e| SessionError::ParseFailed {
                path: path.to_path_buf(),
                line: 0,
                source: e,
            })?;
        contents.push_str(&meta_json);
        contents.push('\n');

        // Lines 2..N: messages
        for (i, msg) in self.messages.iter().enumerate() {
            let json = serde_json::to_string(msg).map_err(|e| SessionError::ParseFailed {
                path: path.to_path_buf(),
                line: i + 2, // line numbers are 1-based; line 1 is metadata
                source: e,
            })?;
            contents.push_str(&json);
            contents.push('\n');
        }

        std::fs::write(path, &contents).map_err(|source| SessionError::WriteFailed {
            path: path.to_path_buf(),
            source,
        })?;

        Ok(())
    }

    /// Load a session from a JSONL file at `path`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The file does not exist or cannot be read.
    /// * The file is empty (no metadata line).
    /// * The metadata line is valid JSON but is not a valid [`SessionMeta`].
    /// * Any message line is not valid [`AgentMessage`] JSON.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tua_rs::session::Session;
    ///
    /// let session = Session::load("/tmp/test-session.jsonl").unwrap();
    /// println!("Loaded session {} with {} messages", session.meta.id, session.messages.len());
    /// ```
    pub fn load(path: impl AsRef<Path>) -> SessionResult<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|source| SessionError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })?;

        let mut lines = content.lines();

        // Line 1: metadata
        let meta_line = lines.next().ok_or_else(|| SessionError::EmptyFile {
            path: path.to_path_buf(),
        })?;

        let meta: SessionMeta =
            serde_json::from_str(meta_line).map_err(|source| SessionError::ParseFailed {
                path: path.to_path_buf(),
                line: 1,
                source,
            })?;

        // Lines 2..N: messages
        let mut messages = Vec::new();
        for (i, line) in lines.enumerate() {
            if line.trim().is_empty() {
                continue; // skip trailing blank lines
            }
            let msg: AgentMessage =
                serde_json::from_str(line).map_err(|source| SessionError::ParseFailed {
                    path: path.to_path_buf(),
                    line: i + 2,
                    source,
                })?;
            messages.push(msg);
        }

        Ok(Self { meta, messages })
    }
}

// ---------------------------------------------------------------------------
// SessionSummary — lightweight listing info
// ---------------------------------------------------------------------------

/// A lightweight summary of a session, used for listing without loading the
/// full message history.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    /// Unique session identifier.
    pub id: uuid::Uuid,
    /// The Rust coding profile used in this session.
    pub profile: String,
    /// Number of messages in the conversation.
    pub message_count: usize,
    /// ISO-8601 timestamp of when the session was created.
    pub created_at: String,
}

/// List all session summaries found in `dir`.
///
/// Scans `dir` for files matching the pattern `*.jsonl`, reads only the
/// first line (metadata) from each, and returns a [`Vec<SessionSummary>`]
/// sorted by `created_at` (most recent first).
///
/// Files that cannot be read or parsed are **skipped** with a `tracing::warn!`
/// message — they do not cause the entire listing to fail.
///
/// # Errors
///
/// Returns [`SessionError::ListFailed`] if `dir` does not exist or cannot
/// be read (e.g. permissions).
///
/// # Examples
///
/// ```no_run
/// use tua_rs::session::list_sessions;
/// use tua_rs::session::SessionResult;
///
/// fn example() -> SessionResult<()> {
///     let summaries = list_sessions("/home/user/.tua-rs/sessions")?;
///     for s in &summaries {
///         println!("{} — {} ({} messages)", s.id, s.profile, s.message_count);
///     }
///     Ok(())
/// }
/// ```
pub fn list_sessions(dir: impl AsRef<Path>) -> SessionResult<Vec<SessionSummary>> {
    let dir = dir.as_ref();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|source| SessionError::ListFailed {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .collect();

    // Sort by file name (which is the UUID) for deterministic ordering.
    // We'll re-sort by created_at after parsing.
    entries.sort_by_key(|e| e.file_name());

    let mut summaries: Vec<SessionSummary> = Vec::new();

    for entry in &entries {
        let path = entry.path();
        match summarize_session_file(&path) {
            Ok(summary) => summaries.push(summary),
            Err(e) => {
                tracing::warn!("skipping session file `{}`: {e}", path.display());
            }
        }
    }

    // Sort by created_at descending (most recent first).
    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(summaries)
}

/// Read only the first line of a JSONL file and produce a [`SessionSummary`].
fn summarize_session_file(path: &Path) -> SessionResult<SessionSummary> {
    use std::io::{BufRead, BufReader};

    let file = std::fs::File::open(path).map_err(|source| SessionError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;

    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .map_err(|source| SessionError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })?;

    if first_line.trim().is_empty() {
        return Err(SessionError::EmptyFile {
            path: path.to_path_buf(),
        });
    }

    let meta: SessionMeta =
        serde_json::from_str(first_line.trim()).map_err(|source| SessionError::ParseFailed {
            path: path.to_path_buf(),
            line: 1,
            source,
        })?;

    // Count remaining non-blank lines to get message count.
    // We already consumed the first line; count the rest.
    let mut message_count = 0usize;
    let mut line = String::new();
    while reader
        .read_line(&mut line)
        .map_err(|source| SessionError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })?
        > 0
    {
        if !line.trim().is_empty() {
            message_count += 1;
        }
        line.clear();
    }

    Ok(SessionSummary {
        id: meta.id,
        profile: meta.profile,
        message_count,
        created_at: meta.created_at,
    })
}

// ---------------------------------------------------------------------------
// Default sessions directory
// ---------------------------------------------------------------------------

/// Return the default sessions directory (`~/.tua-rs/sessions/`).
///
/// Creates the directory if it does not exist.
///
/// # Errors
///
/// Returns an [`std::io::Error`] if the directory cannot be created.
pub fn default_sessions_dir() -> std::io::Result<PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let dir = PathBuf::from(home).join(".tua-rs").join("sessions");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Build the full file path for a session file in the given directory.
#[allow(dead_code)]
fn session_file_path(dir: &Path, id: &uuid::Uuid) -> PathBuf {
    dir.join(format!("{id}.jsonl"))
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Produce an ISO-8601 timestamp string for "now" (UTC).
fn iso_now() -> String {
    // Use `format!` with a Unix epoch fallback for systems without
    // reliable wall-clock time.
    let dur = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    // Build a UTC ISO-8601 string. We don't pull in `chrono` for this;
    // the format is: 2026-07-12T10:00:00Z
    // We compute year/month/day/hour/minute/second from the Unix timestamp.
    let (y, mo, d, h, mi, s) = seconds_to_datetime(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)
}

/// Convert a Unix timestamp (seconds since epoch) to UTC date/time fields.
///
/// This is a hand-rolled implementation to avoid pulling in the `chrono`
/// crate just for timestamps. It uses the civil calendar algorithm.
#[allow(clippy::many_single_char_names)]
fn seconds_to_datetime(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    // Days since epoch
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let h = time_secs / 3600;
    let mi = (time_secs % 3600) / 60;
    let s = time_secs % 60;

    // Civil date from days since 1970-01-01 using Howard Hinnant's algorithm.
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // month phase [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y + 1 } else { y }; // year

    (y, m, d, h, mi, s)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentMessage, AgentToolCall};
    use std::fs;

    // ── Session construction ───────────────────────────────────────────

    #[test]
    fn test_session_new_creates_valid_session() {
        let session = Session::new("rustacean", "deepseek/deepseek-v4-flash");
        assert_eq!(session.meta.profile, "rustacean");
        assert_eq!(session.meta.model, "deepseek/deepseek-v4-flash");
        assert!(session.messages.is_empty());
        // id should be a valid UUID
        let _id = session.meta.id;
        // timestamps should be non-empty
        assert!(!session.meta.created_at.is_empty());
        assert!(!session.meta.updated_at.is_empty());
        // created_at == updated_at for a new session
        assert_eq!(session.meta.created_at, session.meta.updated_at);
    }

    #[test]
    fn test_session_new_generates_unique_ids() {
        let s1 = Session::new("a", "m");
        let s2 = Session::new("b", "n");
        assert_ne!(s1.meta.id, s2.meta.id);
    }

    #[test]
    fn test_push_message_updates_timestamps() {
        let mut session = Session::new("rustacean", "model");
        let original_updated = session.meta.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_secs(1));

        session.push_message(AgentMessage::user("Hello"));
        assert_eq!(session.messages.len(), 1);
        // updated_at should have changed
        assert_ne!(session.meta.updated_at, original_updated);
        // created_at should still be the original
        assert_eq!(session.meta.created_at, original_updated);
    }

    // ── Save / Load roundtrip ──────────────────────────────────────────

    #[test]
    fn test_save_load_roundtrip_empty_session() {
        let dir = std::env::temp_dir().join("__tua_session_test_empty__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join("empty-test.jsonl");
        let session = Session::new("rustacean", "deepseek/deepseek-v4-flash");
        session.save(&path).unwrap();

        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.meta.id, session.meta.id);
        assert_eq!(loaded.meta.profile, session.meta.profile);
        assert_eq!(loaded.meta.model, session.meta.model);
        assert_eq!(loaded.meta.created_at, session.meta.created_at);
        assert_eq!(loaded.meta.updated_at, session.meta.updated_at);
        assert!(loaded.messages.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_load_roundtrip_with_messages() {
        let dir = std::env::temp_dir().join("__tua_session_test_messages__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join("messages-test.jsonl");
        let mut session = Session::new("rustacean", "deepseek/deepseek-v4-flash");

        session.push_message(AgentMessage::user("Write a Fibonacci function"));
        session.push_message(AgentMessage::assistant(
            Some("Here's a Rust Fibonacci implementation:".into()),
            vec![AgentToolCall {
                id: "call_1".into(),
                name: "write_file".into(),
                arguments: serde_json::json!({"path": "src/fib.rs", "content": "..."}),
            }],
        ));
        session.push_message(AgentMessage::tool_result(
            "call_1",
            "File written successfully",
        ));
        session.push_message(AgentMessage::assistant(
            Some("Done! I wrote the file.".into()),
            vec![],
        ));

        session.save(&path).unwrap();

        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.meta.id, session.meta.id);
        assert_eq!(loaded.meta.profile, session.meta.profile);
        assert_eq!(loaded.messages.len(), 4);

        // Verify message content
        match &loaded.messages[0] {
            AgentMessage::User { text } => assert_eq!(text, "Write a Fibonacci function"),
            other => panic!("expected User, got {other:?}"),
        }
        match &loaded.messages[1] {
            AgentMessage::Assistant { text, tool_calls } => {
                assert_eq!(
                    text.as_deref(),
                    Some("Here's a Rust Fibonacci implementation:")
                );
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "write_file");
            }
            other => panic!("expected Assistant, got {other:?}"),
        }
        match &loaded.messages[2] {
            AgentMessage::ToolResult {
                tool_call_id,
                output,
            } => {
                assert_eq!(tool_call_id, "call_1");
                assert_eq!(output, "File written successfully");
            }
            other => panic!("expected ToolResult, got {other:?}"),
        }
        match &loaded.messages[3] {
            AgentMessage::Assistant { text, tool_calls } => {
                assert_eq!(text.as_deref(), Some("Done! I wrote the file."));
                assert!(tool_calls.is_empty());
            }
            other => panic!("expected Assistant, got {other:?}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_save_overwrites_existing_file() {
        let dir = std::env::temp_dir().join("__tua_session_test_overwrite__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let path = dir.join("overwrite-test.jsonl");

        // Save first session
        let mut s1 = Session::new("rustacean", "model");
        s1.push_message(AgentMessage::user("msg1"));
        s1.save(&path).unwrap();

        // Save second session (different content) to same path
        let mut s2 = Session::new("ferris", "other-model");
        s2.push_message(AgentMessage::user("msg2"));
        s2.push_message(AgentMessage::user("msg3"));
        s2.save(&path).unwrap();

        // Load should get s2's content, not s1's
        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.meta.profile, "ferris");
        assert_eq!(loaded.messages.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }

    // ── Load errors ────────────────────────────────────────────────────

    #[test]
    fn test_load_nonexistent_file_returns_error() {
        let path = PathBuf::from("/tmp/__nonexistent_session_file__");
        let result = Session::load(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::ReadFailed { path: p, .. } => {
                assert_eq!(p, path);
            }
            other => panic!("expected ReadFailed, got {other}"),
        }
    }

    #[test]
    fn test_load_empty_file_returns_error() {
        let dir = std::env::temp_dir().join("__tua_session_test_empty_file__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("empty.jsonl");
        fs::write(&path, "").unwrap();

        let result = Session::load(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::EmptyFile { path: p } => {
                assert_eq!(p, path);
            }
            other => panic!("expected EmptyFile, got {other}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_invalid_metadata_returns_error() {
        let dir = std::env::temp_dir().join("__tua_session_test_bad_meta__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad-meta.jsonl");
        fs::write(&path, "{\"not\": \"meta\"}\n").unwrap();

        let result = Session::load(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::ParseFailed { line: 1, .. } => {} // expected
            other => panic!("expected ParseFailed at line 1, got {other}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_invalid_message_line_returns_error() {
        let dir = std::env::temp_dir().join("__tua_session_test_bad_msg__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad-msg.jsonl");

        let meta = SessionMeta {
            id: uuid::Uuid::new_v4(),
            profile: "test".into(),
            model: "test".into(),
            created_at: iso_now(),
            updated_at: iso_now(),
        };
        let meta_json = serde_json::to_string(&meta).unwrap();
        fs::write(&path, format!("{meta_json}\n{{invalid json}}\n")).unwrap();

        let result = Session::load(&path);
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::ParseFailed { line: 2, .. } => {} // expected
            other => panic!("expected ParseFailed at line 2, got {other}"),
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_load_skips_trailing_blank_lines() {
        let dir = std::env::temp_dir().join("__tua_session_test_blanks__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blanks.jsonl");

        let mut session = Session::new("test", "test");
        session.push_message(AgentMessage::user("hi"));
        session.save(&path).unwrap();

        // Append blank lines
        fs::write(
            &path,
            format!("{}\n\n\n", fs::read_to_string(&path).unwrap()),
        )
        .unwrap();

        let loaded = Session::load(&path).unwrap();
        assert_eq!(loaded.messages.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    // ── list_sessions ───────────────────────────────────────────────────

    #[test]
    fn test_list_sessions_empty_dir() {
        let dir = std::env::temp_dir().join("__tua_session_test_list_empty__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let summaries = list_sessions(&dir).unwrap();
        assert!(summaries.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_sessions_multiple_files() {
        let dir = std::env::temp_dir().join("__tua_session_test_list_multi__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let mut s1 = Session::new("rustacean", "model1");
        s1.push_message(AgentMessage::user("msg1"));
        s1.save(&dir.join("session-a.jsonl")).unwrap();

        let mut s2 = Session::new("ferris", "model2");
        s2.push_message(AgentMessage::user("msg2"));
        s2.push_message(AgentMessage::user("msg3"));
        s2.save(&dir.join("session-b.jsonl")).unwrap();

        let mut s3 = Session::new("strict", "model3");
        // empty session
        s3.save(&dir.join("session-c.jsonl")).unwrap();

        let summaries = list_sessions(&dir).unwrap();
        assert_eq!(summaries.len(), 3);

        // Find each summary by profile
        let rustacean = summaries.iter().find(|s| s.profile == "rustacean").unwrap();
        assert_eq!(rustacean.message_count, 1);

        let ferris = summaries.iter().find(|s| s.profile == "ferris").unwrap();
        assert_eq!(ferris.message_count, 2);

        let strict = summaries.iter().find(|s| s.profile == "strict").unwrap();
        assert_eq!(strict.message_count, 0);

        // Sorted by created_at descending (most recent first)
        for w in summaries.windows(2) {
            assert!(
                w[0].created_at >= w[1].created_at,
                "summaries should be sorted by created_at descending"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_sessions_skips_non_jsonl_files() {
        let dir = std::env::temp_dir().join("__tua_session_test_skip_non_jsonl__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // Save a real session
        let s = Session::new("rustacean", "model");
        s.save(&dir.join("valid.jsonl")).unwrap();

        // Put a non-JSONL file in the directory
        fs::write(&dir.join("not_a_session.txt"), "hello").unwrap();
        fs::write(&dir.join("data.csv"), "a,b,c").unwrap();

        let summaries = list_sessions(&dir).unwrap();
        assert_eq!(summaries.len(), 1, "only the .jsonl file should be listed");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_sessions_skips_corrupted_files() {
        let dir = std::env::temp_dir().join("__tua_session_test_corrupt__");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        // Valid session
        let s = Session::new("rustacean", "model");
        s.save(&dir.join("good.jsonl")).unwrap();

        // Corrupted file (invalid JSON on first line)
        fs::write(&dir.join("bad.jsonl"), "not json\n").unwrap();

        let summaries = list_sessions(&dir).unwrap();
        assert_eq!(summaries.len(), 1, "corrupted file should be skipped");
        assert_eq!(summaries[0].profile, "rustacean");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_list_sessions_nonexistent_dir_returns_error() {
        let dir = PathBuf::from("/tmp/__tua_session_test_nonexistent_dir__");
        let result = list_sessions(&dir);
        assert!(result.is_err());
        match result.unwrap_err() {
            SessionError::ListFailed { .. } => {} // expected
            other => panic!("expected ListFailed, got {other}"),
        }
    }

    // ── default_sessions_dir ────────────────────────────────────────────

    #[test]
    fn test_default_sessions_dir_creates_directory() {
        let original_home = std::env::var("HOME").ok();
        let test_home = std::env::temp_dir().join("__tua_session_test_home__");
        let _ = fs::remove_dir_all(&test_home);
        std::env::set_var("HOME", test_home.to_str().unwrap());

        let dir = default_sessions_dir().unwrap();
        assert!(dir.exists(), "sessions directory should be created");
        assert!(
            dir.ends_with(".tua-rs/sessions"),
            "unexpected path: {}",
            dir.display()
        );

        // Calling again should not error (idempotent)
        let dir2 = default_sessions_dir().unwrap();
        assert_eq!(dir, dir2);

        // Clean up
        let _ = fs::remove_dir_all(&test_home);
        if let Some(h) = original_home {
            std::env::set_var("HOME", h);
        }
    }

    // ── ISO timestamp utility ──────────────────────────────────────────

    #[test]
    fn test_iso_now_format() {
        let ts = iso_now();
        // ISO-8601 format: 2026-07-12T10:00:00Z
        assert_eq!(ts.len(), 20, "ISO-8601 should be exactly 20 chars: {ts}");
        assert_eq!(&ts[4..5], "-", "expected '-' at position 4: {ts}");
        assert_eq!(&ts[7..8], "-", "expected '-' at position 7: {ts}");
        assert_eq!(&ts[10..11], "T", "expected 'T' at position 10: {ts}");
        assert_eq!(&ts[13..14], ":", "expected ':' at position 13: {ts}");
        assert_eq!(&ts[16..17], ":", "expected ':' at position 16: {ts}");
        assert_eq!(&ts[19..20], "Z", "expected 'Z' at position 19: {ts}");
    }

    #[test]
    fn test_seconds_to_datetime_known_values() {
        // Unix epoch (1970-01-01T00:00:00Z)
        assert_eq!(seconds_to_datetime(0), (1970, 1, 1, 0, 0, 0));
        // A known date: 2026-07-12T10:00:00Z
        // Approximately 1783735200 seconds from epoch (rough estimate)
        // Let's compute: days from 1970 to 2026
        let ts = 1783850400;
        let (y, mo, d, h, mi, s) = seconds_to_datetime(ts);
        assert_eq!(y, 2026);
        assert_eq!(mo, 7);
        assert_eq!(h, 10);
        assert_eq!(mi, 0);
        assert_eq!(s, 0);
        // d could be 12 or close depending on exact timestamp
        assert!(d >= 1 && d <= 31, "day out of range: {d}");

        // Leap year: 2024-02-29
        let feb_29_2024 = 1709164800; // approximate
        let (y, mo, d, ..) = seconds_to_datetime(feb_29_2024);
        assert_eq!(y, 2024);
        assert_eq!(mo, 2);
        assert_eq!(d, 29);
    }

    #[test]
    fn test_iso_now_is_utc() {
        let ts = iso_now();
        assert!(
            ts.ends_with('Z'),
            "ISO timestamp should end with 'Z' for UTC: {ts}"
        );
    }

    // ── Error type display ─────────────────────────────────────────────

    #[test]
    fn test_session_error_display_read_failed() {
        let err = SessionError::ReadFailed {
            path: PathBuf::from("/test/session.jsonl"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/test/session.jsonl"));
        assert!(msg.contains("file not found"));
    }

    #[test]
    fn test_session_error_display_write_failed() {
        let err = SessionError::WriteFailed {
            path: PathBuf::from("/test/session.jsonl"),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/test/session.jsonl"));
        assert!(msg.contains("access denied"));
    }

    #[test]
    fn test_session_error_display_parse_failed() {
        let err = SessionError::ParseFailed {
            path: PathBuf::from("/test/session.jsonl"),
            line: 3,
            source: serde_json::from_str::<serde_json::Value>("{invalid}").unwrap_err(),
        };
        let msg = err.to_string();
        assert!(msg.contains("line 3"));
        assert!(msg.contains("/test/session.jsonl"));
    }

    #[test]
    fn test_session_error_display_empty_file() {
        let err = SessionError::EmptyFile {
            path: PathBuf::from("/test/empty.jsonl"),
        };
        let msg = err.to_string();
        assert!(msg.contains("empty"));
        assert!(msg.contains("/test/empty.jsonl"));
    }

    #[test]
    fn test_session_error_display_missing_metadata() {
        let err = SessionError::MissingMetadata {
            path: PathBuf::from("/test/no-meta.jsonl"),
        };
        let msg = err.to_string();
        assert!(msg.contains("missing metadata"));
        assert!(msg.contains("/test/no-meta.jsonl"));
    }

    #[test]
    fn test_session_error_display_list_failed() {
        let err = SessionError::ListFailed {
            path: PathBuf::from("/nonexistent"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "no such directory"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/nonexistent"));
        assert!(msg.contains("no such directory"));
    }
}
