//! 🔍 LSP Client — rust-analyzer integration
//!
//! Communicates with rust-analyzer via stdin/stdout using the
//! Language Server Protocol (LSP). The agent can query the
//! running analyzer for precise type information, function
//! definitions, and references — eliminating hallucination.
//!
//! ## Architecture
//! - Spawns `rust-analyzer` as a subprocess
//! - Sends JSON-RPC messages over stdin
//! - Reads responses from stdout
//! - Maintains a single long-lived session per project
//!
//! ## Supported Queries
//! - **Go to Definition**: find where a symbol is defined
//! - **Hover**: get type information and docs
//! - **References**: find all usages of a symbol
//! - **Completions**: get autocomplete suggestions

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

// ---------------------------------------------------------------------------
// LSP Client
// ---------------------------------------------------------------------------

/// A long-lived connection to a rust-analyzer process.
pub struct LspClient {
    process: Mutex<Option<Child>>,
    next_id: Mutex<u64>,
    root_path: String,
}

/// Result of an LSP query.
#[derive(Debug, Clone)]
pub struct LspResult {
    pub query_type: String,
    pub symbol: String,
    pub locations: Vec<SymbolLocation>,
    pub type_info: Option<String>,
    pub docs: Option<String>,
}

/// A location in source code (file + line + column).
#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub file: String,
    pub line: u64,
    pub column: u64,
    pub snippet: String,
}

impl LspClient {
    /// Start a new rust-analyzer process for the given project root.
    pub fn start(root_path: &str) -> Result<Self, String> {
        let process = Command::new("rust-analyzer")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to start rust-analyzer: {}", e))?;

        Ok(Self {
            process: Mutex::new(Some(process)),
            next_id: Mutex::new(1),
            root_path: root_path.to_string(),
        })
    }

    /// Send an "initialize" request to rust-analyzer.
    pub fn initialize(&self) -> Result<(), String> {
        let id = self.next_id();
        let init = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": format!("file://{}", self.root_path),
                "capabilities": {}
            }
        });
        self.send_request(&init.to_string())?;
        // Read the initialize response
        let _response = self.read_response()?;

        // Send "initialized" notification
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        self.send_request(&notif.to_string())?;

        // Wait a moment for the server to index
        std::thread::sleep(std::time::Duration::from_millis(500));
        Ok(())
    }

    /// Go to Definition: find where a symbol at (file, line, col) is defined.
    pub fn go_to_definition(&self, file: &str, line: u64, col: u64) -> Result<LspResult, String> {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/definition",
            "params": {
                "textDocument": { "uri": format!("file://{}", file) },
                "position": { "line": line - 1, "character": col - 1 }
            }
        });
        self.send_request(&req.to_string())?;
        let resp = self.read_response()?;

        Ok(LspResult {
            query_type: "definition".into(),
            symbol: format!("{}:{}:{}", file, line, col),
            locations: parse_locations(&resp),
            type_info: None,
            docs: None,
        })
    }

    /// Hover: get type information at a position.
    pub fn hover(&self, file: &str, line: u64, col: u64) -> Result<LspResult, String> {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/hover",
            "params": {
                "textDocument": { "uri": format!("file://{}", file) },
                "position": { "line": line - 1, "character": col - 1 }
            }
        });
        self.send_request(&req.to_string())?;
        let resp = self.read_response()?;

        Ok(LspResult {
            query_type: "hover".into(),
            symbol: format!("{}:{}:{}", file, line, col),
            locations: vec![],
            type_info: extract_hover_text(&resp),
            docs: extract_hover_docs(&resp),
        })
    }

    /// Find all references to a symbol.
    pub fn references(&self, file: &str, line: u64, col: u64) -> Result<LspResult, String> {
        let id = self.next_id();
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/references",
            "params": {
                "textDocument": { "uri": format!("file://{}", file) },
                "position": { "line": line - 1, "character": col - 1 },
                "context": { "includeDeclaration": true }
            }
        });
        self.send_request(&req.to_string())?;
        let resp = self.read_response()?;

        Ok(LspResult {
            query_type: "references".into(),
            symbol: format!("{}:{}:{}", file, line, col),
            locations: parse_locations(&resp),
            type_info: None,
            docs: None,
        })
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock().unwrap();
        let current = *id;
        *id += 1;
        current
    }

    fn send_request(&self, json: &str) -> Result<(), String> {
        let mut guard = self.process.lock().unwrap();
        if let Some(ref mut child) = *guard {
            let header = format!("Content-Length: {}\r\n\r\n", json.len());
            let stdin = child.stdin.as_mut().ok_or("stdin not available")?;
            stdin
                .write_all(header.as_bytes())
                .map_err(|e| format!("LSP write error: {}", e))?;
            stdin
                .write_all(json.as_bytes())
                .map_err(|e| format!("LSP write error: {}", e))?;
            stdin
                .flush()
                .map_err(|e| format!("LSP flush error: {}", e))?;
            Ok(())
        } else {
            Err("LSP process not running".into())
        }
    }

    fn read_response(&self) -> Result<String, String> {
        let mut guard = self.process.lock().unwrap();
        if let Some(ref mut child) = *guard {
            let stdout = child.stdout.as_mut().ok_or("stdout not available")?;
            let mut reader = BufReader::new(stdout);

            // Read Content-Length header
            let mut header = String::new();
            reader
                .read_line(&mut header)
                .map_err(|e| format!("LSP read error: {}", e))?;

            if !header.starts_with("Content-Length:") {
                return Ok(String::new());
            }

            let len: usize = header
                .trim_start_matches("Content-Length: ")
                .trim()
                .parse()
                .unwrap_or(0);

            // Skip empty line after header
            let mut blank = String::new();
            reader.read_line(&mut blank).ok();

            // Read body
            let mut body = vec![0u8; len];
            use std::io::Read;
            reader
                .read_exact(&mut body)
                .map_err(|e| format!("LSP read body error: {}", e))?;

            Ok(String::from_utf8_lossy(&body).to_string())
        } else {
            Err("LSP process not running".into())
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(ref mut child) = *guard {
                // Send shutdown
                let shutdown = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 9999,
                    "method": "shutdown",
                    "params": null
                });
                if let Some(stdin) = child.stdin.as_mut() {
                    let json = shutdown.to_string();
                    let header = format!("Content-Length: {}\r\n\r\n", json.len());
                    let _ = stdin.write_all(header.as_bytes());
                    let _ = stdin.write_all(json.as_bytes());
                }
                let _ = child.kill();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Response parsing helpers
// ---------------------------------------------------------------------------

fn parse_locations(json: &str) -> Vec<SymbolLocation> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        let result = &value["result"];
        match result {
            serde_json::Value::Array(arr) => arr
                .iter()
                .filter_map(|loc| {
                    Some(SymbolLocation {
                        file: loc["uri"].as_str()?.strip_prefix("file://")?.to_string(),
                        line: loc["range"]["start"]["line"].as_u64()? + 1,
                        column: loc["range"]["start"]["character"].as_u64()? + 1,
                        snippet: String::new(),
                    })
                })
                .collect(),
            serde_json::Value::Object(_) => {
                // Single location
                vec![SymbolLocation {
                    file: result["uri"]
                        .as_str()
                        .unwrap_or("")
                        .strip_prefix("file://")
                        .unwrap_or("")
                        .to_string(),
                    line: result["range"]["start"]["line"].as_u64().unwrap_or(0) + 1,
                    column: result["range"]["start"]["character"].as_u64().unwrap_or(0) + 1,
                    snippet: String::new(),
                }]
            }
            _ => vec![],
        }
    } else {
        vec![]
    }
}

fn extract_hover_text(json: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        let contents = &value["result"]["contents"];
        match contents {
            serde_json::Value::Object(obj) => {
                obj.get("value").and_then(|v| v.as_str().map(String::from))
            }
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None,
        }
    } else {
        None
    }
}

fn extract_hover_docs(json: &str) -> Option<String> {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(json) {
        let doc = &value["result"]["contents"]["documentation"];
        match doc {
            serde_json::Value::String(s) => Some(s.clone()),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_locations_empty() {
        let result = parse_locations("{}");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_locations_array() {
        let json = r#"{"result":[{"uri":"file:///src/main.rs","range":{"start":{"line":9,"character":4}}}]}"#;
        let locs = parse_locations(json);
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].file, "/src/main.rs");
        assert_eq!(locs[0].line, 10); // 1-indexed
    }

    #[test]
    fn test_extract_hover_text() {
        let json = r#"{"result":{"contents":{"value":"fn hello() -> String"}}}"#;
        let text = extract_hover_text(json);
        assert_eq!(text, Some("fn hello() -> String".into()));
    }
}
