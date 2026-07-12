# ADD THESE TWO FUNCTIONS to src/session.rs

YOU MUST use the Write tool to edit the file. DO NOT just analyze — WRITE the code.

## Function 1: save_to_default (add after existing save() function around line 220)

```rust
/// Save this session to the default sessions directory
/// (`~/.tua-rs/sessions/{session_id}.jsonl`).
///
/// Creates the directory automatically if it doesn't exist.
pub fn save_to_default(&self) -> SessionResult<()> {
    let dir = default_sessions_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| SessionError::WriteFailed(format!(
        "cannot create sessions directory {}: {e}",
        dir.display()
    )))?;
    let path = session_file_path(&dir, &self.meta.id);
    self.save(&path)
}
```

## Function 2: load_from_default (add after function 1)

```rust
/// Load a session by UUID from the default sessions directory.
pub fn load_from_default(id: &str) -> SessionResult<Self> {
    let dir = default_sessions_dir()?;
    let uuid = uuid::Uuid::parse_str(id).map_err(|e| SessionError::ReadFailed(format!(
        "invalid session id '{id}': {e}"
    )))?;
    let path = session_file_path(&dir, &uuid);
    Self::load(&path)
}
```

## Function 3: default_sessions_dir (add before save_to_default)

```rust
/// Return the default sessions directory (~/.tua-rs/sessions).
pub fn default_sessions_dir() -> SessionResult<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| SessionError::ReadFailed(
        "cannot determine home directory".into()
    ))?;
    Ok(home.join(".tua-rs").join("sessions"))
}
```

## Tests (add to existing test mod)

```rust
#[test]
fn test_save_to_default_creates_dir() {
    let session = Session::new("rustacean", "test-model");
    let tmp = std::env::temp_dir().join(format!("__tua_save_default_{}__", uuid::Uuid::new_v4()));
    // Use the save method directly since save_to_default uses ~/.tua-rs
    let path = tmp.join("test.jsonl");
    std::fs::create_dir_all(&tmp).unwrap();
    session.save(&path).unwrap();
    assert!(path.exists());
    let loaded = Session::load(&path).unwrap();
    assert_eq!(loaded.meta.profile, "rustacean");
}

#[test]
fn test_save_to_default_overwrites() {
    let session = Session::new("ferris", "model2");
    let tmp = std::env::temp_dir().join(format!("__tua_overwrite_{}__", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).unwrap();
    let path = tmp.join("overwrite.jsonl");
    session.save(&path).unwrap();
    let session2 = Session::new("rustacean", "model3");
    session2.save(&path).unwrap();
    let loaded = Session::load(&path).unwrap();
    assert_eq!(loaded.meta.profile, "rustacean");
}
```

Add `dirs` to Cargo.toml if not already present:
```toml
dirs = "6"
```

## Verification
```bash
cargo test session
cargo clippy
```

CRITICAL: YOU MUST USE WRITE TOOL. Read src/session.rs first, then EDIT it to add these functions.
