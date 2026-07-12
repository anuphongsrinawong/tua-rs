# CLAUDE_TASK.md — Interactive Provider/Model Picker in TUI

## Project: /tmp/tua-rs (Rust, v1.1.0)

## Current State

The TUI hardcodes `MockProvider` and `/model` shows a static message:
```
"🤖 Model: deepseek/deepseek-v4-flash (configurable in config)"
```

## Goal

When user presses `/model` in the TUI:
1. A popup appears listing available providers from config files
2. User can navigate with ↑↓ arrows, press Enter to select
3. After selecting provider → sub-list of models for that provider
4. After selecting model → agent rebuilds with real provider

## Config Files to Parse

### `~/.tau/providers.json`
```json
{
  "default_provider": "9router",
  "provider_preferences": {
    "9router": { "default_model": "glm/glm-5.2", ... },
    "openai-codex": { "default_model": "gpt-5.5", ... }
  }
}
```

### `~/.tau/catalog.toml`
```toml
[[providers]]
name = "9router"
kind = "openai-compatible"
base_url = "http://127.0.0.1:20128/v1"
models = ["glm/glm-5.2", "glm/glm-4.7", "deepseek/deepseek-v4-flash"]
default_model = "glm/glm-5.2"

[[providers]]
name = "openai-codex"
kind = "openai-codex"
base_url = "https://chatgpt.com/backend-api"
models = ["gpt-5.5", "gpt-5.4", ...]
default_model = "gpt-5.5"
```

### `~/.tau/credentials.json`
```json
{ "9router": "free" }
```

## Implementation Plan

### Step 1: Add provider/model state to `App` struct (src/tui.rs)

```rust
pub struct App {
    // ... existing fields ...
    /// Available providers loaded from config.
    pub providers: Vec<ProviderInfo>,
    /// Currently selected provider name.
    pub selected_provider: String,
    /// Currently selected model name.
    pub selected_model: String,
    /// Whether the provider/model picker is open.
    pub mode: AppMode,  // add ProviderPicker variant
}

struct ProviderInfo {
    name: String,
    kind: String,
    base_url: String,
    models: Vec<String>,
    default_model: String,
}
```

### Step 2: Parse config files on startup (src/tui.rs)

Add a function `load_provider_config() -> Vec<ProviderInfo>` that:
1. Reads `~/.tau/catalog.toml` using the `toml` crate
2. Reads `~/.tau/providers.json` using `serde_json`
3. Returns Vec<ProviderInfo> with all providers and their models

Use `dirs::home_dir()` or `std::env::var("HOME")` to find home directory.

### Step 3: Add `AppMode::ProviderPicker`

```rust
pub enum AppMode {
    Normal,
    CommandPalette,
    ProviderPicker,     // NEW: selecting provider
    ModelPicker,        // NEW: selecting model for chosen provider
}
```

### Step 4: Create selection popup UI

When in ProviderPicker mode:
- Show a centered list of provider names with their default models
- Use ↑↓ to navigate, Enter to select
- After selecting provider → switch to ModelPicker mode
- In ModelPicker: show list of models for that provider, ↑↓/Enter to select
- Esc to cancel/close

Use existing render patterns from `CommandPalette` (already in tui.rs).

### Step 5: Wire real provider

When a provider+model is selected:
1. Read API key from `~/.tau/credentials.json`
2. Build `ProviderConfig` with base_url, api_key, model
3. Create `OpenAiCompatibleProvider` (for openai-compatible) or appropriate provider
4. Replace `agent_loop` in App
5. Status bar shows current provider/model

### Step 6: Update the `/model` command handler

Change from static text to triggering the ProviderPicker popup.

### Step 7: Add `toml` and `dirs` dependencies

In Cargo.toml:
```toml
toml = "0.8"
dirs = "6"
```

## Verification
```bash
cargo build          # must compile with new deps
cargo test --lib     # all 282 tests still pass
cargo clippy         # zero warnings
```

## CRITICAL
- Keep MockProvider as fallback when no config files found
- Provider/model info is per-tab (each tab can have different provider)
- Store provider name, model name, and API key in Tab struct
- Error handling: if API key missing, show "⚠️ No API key for {provider}" instead of crashing
- Read ALL existing source files first before modifying
