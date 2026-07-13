# Task: Add CLI Agent Loop to tua-rs

## Problem
`tua-rs --prompt "..."` just prints the prompt and exits. No agent loop runs.
Only the TUI (`tua-rs tui`) has an actual agent loop.

## Goal
When `--prompt` is provided without a subcommand, run the full agent loop:
1. Build provider from `~/.tau/catalog.toml` + `~/.tau/credentials.json`
2. Create AgentLoop with the Rust system prompt + tools
3. Stream agent events to stdout
4. Print final response

## Current Code

### main.rs (lines 107-119) — NO-OP
```rust
_ => {
    if let Some(ref prompt) = cli.prompt {
        println!("🦀 Tua Agent RS v1.0.0 | {} | {}", cli.profile, cli.provider);
        println!("💬 {}", prompt);
    } else {
        // ... help text
    }
}
```

### TUI already does this (tui.rs lines 813-848)
- `build_agent_loop()` — reads catalog/credentials, creates provider, builds AgentLoop
- `run()` — streams agent events and renders TUI

## Implementation Plan

### Step 1: Extract provider loading into shared function

Add `pub fn load_provider(provider_name: &str, model: &str) -> anyhow::Result<Arc<dyn ModelProvider>>` 
to `src/tui.rs` (or a new `src/provider_loader.rs`).

Logic:
```rust
pub fn load_provider(provider_name: &str, model: &str) -> anyhow::Result<Arc<dyn ModelProvider>> {
    // 1. Read ~/.tau/catalog.toml
    let catalog_path = dirs::home_dir().unwrap().join(".tau/catalog.toml");
    let catalog: CatalogFile = toml::from_str(&std::fs::read_to_string(&catalog_path)?)?;
    
    // 2. Find provider
    let info = catalog.providers.iter()
        .find(|p| p.name == provider_name)
        .ok_or_else(|| anyhow!("Provider '{provider_name}' not found in catalog"))?;
    
    // 3. Read ~/.tau/credentials.json
    let cred_path = dirs::home_dir().unwrap().join(".tau/credentials.json");
    let creds: HashMap<String, String> = serde_json::from_str(&std::fs::read_to_string(&cred_path)?)?;
    let api_key = creds.get(provider_name).cloned().unwrap_or_default();
    
    // 4. Build provider
    match info.kind.as_str() {
        "openai-compatible" | "openai" => {
            let cfg = ProviderConfig::new("openai", api_key, Some(info.base_url.clone()), model.to_string());
            Ok(Arc::new(OpenAiCompatibleProvider::new(cfg)))
        }
        other => anyhow::bail!("Provider kind '{other}' not wired yet"),
    }
}
```

### Step 2: Modify main.rs default branch

```rust
_ => {
    if let Some(ref prompt) = cli.prompt {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            // Load provider from config
            let provider = tua_rs::tui::load_provider(&cli.provider, &cli.model)
                .unwrap_or_else(|e| {
                    eprintln!("⚠️  Failed to load provider: {e}");
                    eprintln!("   Falling back to MockProvider");
                    Arc::new(tua_rs::tui::mock_greeting())
                });
            
            // Build agent loop
            let system = tua_rs::prompts::rust_system_prompt::RUST_SYSTEM_PROMPT;
            let tools = tua_rs::tools::rust_tools();
            let agent = AgentLoop::new(provider, system, tools);
            
            let messages = vec![AgentMessage::user(prompt)];
            let mut stream = agent.run(messages);
            
            use futures::StreamExt;
            let mut final_text = String::new();
            while let Some(event) = stream.next().await {
                match event {
                    AgentEvent::TextDelta(text) => {
                        print!("{}", text);
                        final_text.push_str(&text);
                    }
                    AgentEvent::ThinkingDelta(text) => {
                        eprint!("{}", text);  // thinking to stderr
                    }
                    AgentEvent::ToolCall { name, args } => {
                        eprintln!("\n🔧 {}({})", name, args);
                    }
                    AgentEvent::ToolResult { output, .. } => {
                        eprintln!("\n📋 {}", &output[..output.len().min(200)]);
                    }
                    AgentEvent::Done => break,
                    AgentEvent::Error(e) => {
                        eprintln!("\n❌ Error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }
            println!(); // final newline
        })?;
    } else {
        // ... existing help text
    }
}
```

### Step 3: Make `load_provider` and `mock_greeting` public

In `src/tui.rs`, ensure these are `pub`:
- `pub fn load_provider(...)` 
- `pub fn mock_greeting()`

### Step 4: Add imports to main.rs

```rust
use std::sync::Arc;
use tua_rs::agent::{AgentEvent, AgentLoop, AgentMessage, ModelProvider};
use tua_rs::tools::rust_tools;
use tua_rs::prompts::rust_system_prompt::RUST_SYSTEM_PROMPT;
```

## Verification
```bash
cd ~/tua-rs
cargo build
./target/debug/tua-rs --provider 9router --model "ds/deepseek-v4-pro" --prompt "Hello, who are you?"
# Should stream response from the agent
```

## Files to modify
- `src/main.rs` — add agent loop in default branch
- `src/tui.rs` — add `pub fn load_provider()` and ensure `mock_greeting` is pub
- `src/lib.rs` — ensure needed modules are `pub`

## CRITICAL
- NO new dependencies
- Works even when catalog/credentials are missing (fallback to MockProvider)
- Prints tool calls to stderr, final response to stdout
- Don't break existing subcommands
- Context guard already baked into AgentLoop::run()
