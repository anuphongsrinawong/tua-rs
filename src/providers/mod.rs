//! Multi-provider support for the Tua Agent.
//!
//! Providers implement the [`ModelProvider`](crate::agent::ModelProvider) trait
//! and are registered in a [`ProviderRegistry`] that the agent uses to
//! resolve the active provider by name.
//!
//! # Supported Providers
//!
//! | Provider             | CLI name       | Auth                     | Default base URL                  |
//! |----------------------|----------------|--------------------------|-----------------------------------|
//! | OpenAI-compatible    | `openai`       | `Authorization: Bearer`  | `https://api.openai.com/v1`       |
//! | Anthropic            | `anthropic`    | `x-api-key`              | `https://api.anthropic.com/v1`    |
//! | Ollama (local)       | `ollama`       | none                     | `http://localhost:11434/v1`       |

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::agent::ModelProvider;

pub mod anthropic;
pub mod mock;
pub mod ollama;
pub mod openai_compatible;

pub use anthropic::AnthropicProvider;
pub use mock::MockProvider;
pub use ollama::OllamaProvider;
pub use openai_compatible::OpenAiCompatibleProvider;

// ---------------------------------------------------------------------------
// ProviderConfig — shared configuration for all providers
// ---------------------------------------------------------------------------

/// Configuration for a single provider instance.
///
/// The `provider_type` field selects which backend to use:
/// - `"openai"` — OpenAI / any OpenAI-compatible API
/// - `"anthropic"` — Anthropic API (same SSE format, `x-api-key` auth)
/// - `"ollama"` — Local Ollama instance at `http://localhost:11434/v1`
///
/// # Example (TOML)
///
/// ```toml
/// [providers.default]
/// provider_type = "openai"
/// api_key = "sk-..."
/// model = "gpt-4o"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Provider backend: `"openai"`, `"anthropic"`, or `"ollama"`.
    pub provider_type: String,
    /// API key for authentication (optional for Ollama).
    #[serde(default)]
    pub api_key: String,
    /// Base URL for the API endpoint.
    #[serde(
        default = "default_base_url_for_type",
        skip_serializing_if = "Option::is_none"
    )]
    pub base_url: Option<String>,
    /// Model identifier (e.g. `"gpt-4o"`, `"claude-sonnet-4-20250514"`).
    pub model: String,
}

impl ProviderConfig {
    /// Create a new provider configuration.
    pub fn new(
        provider_type: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            provider_type: provider_type.into(),
            api_key: api_key.into(),
            base_url,
            model: model.into(),
        }
    }

    /// Build a [`ProviderConfig`] from environment variables, falling back
    /// to the given CLI args and config-file overrides.
    ///
    /// # Env Vars
    ///
    /// | Provider   | Env var             |
    /// |------------|---------------------|
    /// | `openai`   | `OPENAI_API_KEY`    |
    /// | `anthropic`| `ANTHROPIC_API_KEY` |
    /// | `ollama`   | `OLLAMA_API_KEY`    |
    ///
    /// When `base_url` is `None`, the provider-specific default is used.
    pub fn from_env(
        provider_type: &str,
        base_url: Option<String>,
        model: String,
        config_overrides: Option<&ProviderSettings>,
    ) -> Self {
        // Determine the API key: env var → config file → empty (ollama may not need one).
        let api_key = Self::env_var_for_type(provider_type)
            .and_then(|var| std::env::var(var).ok())
            .or_else(|| {
                config_overrides
                    .and_then(|s| s.api_key.clone())
                    .filter(|k| !k.is_empty())
            })
            .unwrap_or_default();

        // Determine base URL: explicit arg → config file → default.
        let base_url = base_url.or_else(|| {
            config_overrides
                .and_then(|s| s.base_url.clone())
                .filter(|u| !u.is_empty())
        });

        // Determine model: explicit arg → config file → empty (must be provided).
        let model = if model.is_empty() {
            config_overrides
                .and_then(|s| s.model.clone())
                .unwrap_or_default()
        } else {
            model
        };

        Self {
            provider_type: provider_type.to_string(),
            api_key,
            base_url,
            model,
        }
    }

    /// Return the env-var name for the given provider type.
    fn env_var_for_type(provider_type: &str) -> Option<&'static str> {
        match provider_type {
            "openai" => Some("OPENAI_API_KEY"),
            "anthropic" => Some("ANTHROPIC_API_KEY"),
            "ollama" => Some("OLLAMA_API_KEY"),
            _ => None,
        }
    }

    /// Resolve the effective base URL, applying the provider default when
    /// `self.base_url` is `None`.
    pub fn resolved_base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or_else(|| default_base_url(&self.provider_type))
    }

    /// Validate the connection to the provider by sending a minimal probe.
    ///
    /// Returns `Ok(())` if the provider is reachable and credentials are
    /// accepted, or an error message explaining the failure.
    pub async fn validate_connection(&self) -> Result<(), String> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("failed to build HTTP client: {e}"))?;

        let base = self.resolved_base_url().trim_end_matches('/');
        let url = format!("{base}/models");

        let mut request = client.get(&url);

        match self.provider_type.as_str() {
            "openai" => {
                if !self.api_key.is_empty() {
                    let auth_val = format!("Bearer {}", self.api_key);
                    request = request.header(reqwest::header::AUTHORIZATION, auth_val);
                }
            }
            "anthropic" => {
                if !self.api_key.is_empty() {
                    request = request.header("x-api-key", &self.api_key);
                }
                // Anthropic also requires anthropic-version header.
                request = request.header("anthropic-version", "2023-06-01");
            }
            "ollama" => {
                // No auth needed for local Ollama.
            }
            _ => {
                // Unknown type — try Bearer anyway.
                if !self.api_key.is_empty() {
                    let auth_val = format!("Bearer {}", self.api_key);
                    request = request.header(reqwest::header::AUTHORIZATION, auth_val);
                }
            }
        }

        let response = request.send().await.map_err(|e| {
            format!(
                "cannot reach {} provider at `{}`: {e}",
                self.provider_type, url
            )
        })?;

        if response.status().is_success() || response.status().as_u16() == 404 {
            // 404 is fine — some providers don't expose `/models`.
            info!(
                provider = %self.provider_type,
                base_url = %base,
                model = %self.model,
                status = %response.status(),
                "provider connection validated"
            );
            Ok(())
        } else {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_else(|_| "<no body>".into());
            Err(format!(
                "{} provider returned HTTP {status}: {body}",
                self.provider_type
            ))
        }
    }
}

/// Return the default base URL for a given provider type.
pub fn default_base_url(provider_type: &str) -> &'static str {
    match provider_type {
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com/v1",
        "ollama" => "http://localhost:11434/v1",
        _ => "https://api.openai.com/v1",
    }
}

fn default_base_url_for_type() -> Option<String> {
    None
}

// ---------------------------------------------------------------------------
// ProviderSettings — per-provider overrides from TOML config
// ---------------------------------------------------------------------------

/// Per-provider settings that can be defined in the `[providers]` section
/// of `~/.tua-rs/config.toml`.
///
/// # Example
///
/// ```toml
/// [providers.openai]
/// api_key = "sk-..."
/// base_url = "https://api.openai.com/v1"
/// model = "gpt-4o"
///
/// [providers.anthropic]
/// api_key = "sk-ant-..."
/// model = "claude-sonnet-4-20250514"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ProviderSettings {
    /// Override API key for this provider.
    pub api_key: Option<String>,
    /// Override base URL for this provider.
    pub base_url: Option<String>,
    /// Override default model for this provider.
    pub model: Option<String>,
}

// ---------------------------------------------------------------------------
// ProviderRegistry
// ---------------------------------------------------------------------------

/// A registry of named providers that the agent can select from.
///
/// Providers are constructed from a [`ProviderConfig`] and stored as
/// `Arc<dyn ModelProvider>` for thread-safe sharing across agent loops.
///
/// # Example
///
/// ```rust,ignore
/// let registry = ProviderRegistry::builder()
///     .register("default", ProviderConfig::new("openai", "sk-...", None, "gpt-4o"))?
///     .register_anthropic("claude", "sk-ant-...", "claude-sonnet-4-20250514")?
///     .register_ollama("local", "llama3")?
///     .build();
///
/// let provider = registry.get("default")?;
/// ```
#[derive(Clone)]
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn ModelProvider>>,
}

impl std::fmt::Debug for ProviderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderRegistry")
            .field("providers", &self.names())
            .field("count", &self.len())
            .finish()
    }
}

impl ProviderRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Register a provider under the given name.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        config: ProviderConfig,
    ) -> Result<Arc<dyn ModelProvider>, String> {
        let provider = build_provider(config)?;
        let name = name.into();
        let arc = Arc::clone(&provider);
        self.providers.insert(name, provider);
        Ok(arc)
    }

    /// Register an OpenAI-compatible provider.
    pub fn register_openai(
        &mut self,
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> Result<Arc<dyn ModelProvider>, String> {
        self.register(
            name,
            ProviderConfig::new("openai", api_key, base_url, model),
        )
    }

    /// Register an Anthropic provider.
    pub fn register_anthropic(
        &mut self,
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> Result<Arc<dyn ModelProvider>, String> {
        self.register(
            name,
            ProviderConfig::new("anthropic", api_key, base_url, model),
        )
    }

    /// Register an Ollama provider.
    pub fn register_ollama(
        &mut self,
        name: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> Result<Arc<dyn ModelProvider>, String> {
        self.register(name, ProviderConfig::new("ollama", "", base_url, model))
    }

    /// Get a registered provider by name.
    pub fn get(&self, name: &str) -> Option<&Arc<dyn ModelProvider>> {
        self.providers.get(name)
    }

    /// List all registered provider names.
    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.providers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Number of registered providers.
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Returns `true` if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// Return a [`ProviderRegistryBuilder`] for ergonomic construction.
    pub fn builder() -> ProviderRegistryBuilder {
        ProviderRegistryBuilder::new()
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::iter::FromIterator<(String, Arc<dyn ModelProvider>)> for ProviderRegistry {
    fn from_iter<I: IntoIterator<Item = (String, Arc<dyn ModelProvider>)>>(iter: I) -> Self {
        let mut registry = Self::new();
        for (name, provider) in iter {
            registry.providers.insert(name, provider);
        }
        registry
    }
}

// ---------------------------------------------------------------------------
// ProviderRegistryBuilder
// ---------------------------------------------------------------------------

/// Builder for ergonomic construction of a [`ProviderRegistry`].
#[derive(Debug)]
pub struct ProviderRegistryBuilder {
    configs: Vec<(String, ProviderConfig)>,
}

impl ProviderRegistryBuilder {
    fn new() -> Self {
        Self {
            configs: Vec::new(),
        }
    }

    /// Register a provider by its [`ProviderConfig`].
    pub fn register(
        mut self,
        name: impl Into<String>,
        config: ProviderConfig,
    ) -> ProviderRegistryBuilder {
        self.configs.push((name.into(), config));
        self
    }

    /// Convenience for registering an OpenAI-compatible provider.
    pub fn register_openai(
        self,
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> ProviderRegistryBuilder {
        self.register(
            name,
            ProviderConfig::new("openai", api_key, base_url, model),
        )
    }

    /// Convenience for registering an Anthropic provider.
    pub fn register_anthropic(
        self,
        name: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> ProviderRegistryBuilder {
        self.register(
            name,
            ProviderConfig::new("anthropic", api_key, base_url, model),
        )
    }

    /// Convenience for registering an Ollama provider.
    pub fn register_ollama(
        self,
        name: impl Into<String>,
        base_url: Option<String>,
        model: impl Into<String>,
    ) -> ProviderRegistryBuilder {
        self.register(name, ProviderConfig::new("ollama", "", base_url, model))
    }

    /// Build the registry, constructing all providers.
    ///
    /// Returns an error if any provider construction fails.
    pub fn build(self) -> Result<ProviderRegistry, String> {
        let mut registry = ProviderRegistry::new();
        for (name, config) in self.configs {
            registry.register(name, config)?;
        }
        Ok(registry)
    }
}

// ---------------------------------------------------------------------------
// Provider factory
// ---------------------------------------------------------------------------

/// Construct a [`ModelProvider`] from a [`ProviderConfig`].
///
/// This is the single point where provider type strings are mapped to
/// concrete provider implementations.
pub fn build_provider(config: ProviderConfig) -> Result<Arc<dyn ModelProvider>, String> {
    match config.provider_type.as_str() {
        "openai" => Ok(Arc::new(OpenAiCompatibleProvider::new(config))),
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(config))),
        "ollama" => Ok(Arc::new(OllamaProvider::new(config))),
        other => Err(format!(
            "unsupported provider type `{other}` — expected one of: openai, anthropic, ollama"
        )),
    }
}

/// Build the default provider from CLI arguments and optional config overrides.
///
/// This is the main entry point used by `main.rs` to create the active
/// provider at startup.
pub fn build_default_provider(
    provider_type: &str,
    base_url: Option<String>,
    model: String,
    config_overrides: Option<&ProviderSettings>,
    validate: bool,
) -> Result<Arc<dyn ModelProvider>, String> {
    let config = ProviderConfig::from_env(provider_type, base_url, model, config_overrides);

    if config.model.is_empty() {
        return Err(format!(
            "no model specified for provider `{provider_type}` — set it via --model, \
             ~/.tua-rs/config.toml [providers.{provider_type}].model, or the default profile"
        ));
    }

    if validate {
        let rt = tokio::runtime::Handle::try_current();

        if rt.is_err() {
            // No active runtime — create a temporary one just for validation.
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("failed to create runtime for validation: {e}"))?;
            rt.block_on(config.validate_connection())?;
        } else {
            // We have a runtime; spawn validation and block (not ideal).
            // For the CLI entry point we handle this in main().
        }
    }

    build_provider(config)
}

// ── Fallback Provider Chain ──────────────────────────────────────────

/// Try a list of provider configurations in order, returning the first
/// one that successfully validates its connection.
///
/// This enables automatic failover: if the primary provider times out
/// or is rate-limited, the agent falls back to the next in the chain.
pub fn fallback_chain(
    candidates: &[(&str, &str)],  // (provider_name, model_name)
    api_key: &str,
    _settings: Option<&ProviderSettings>,
    validate: bool,
) -> Result<(Arc<dyn ModelProvider>, String), String> {
    let mut last_error = String::new();
    
    for (i, (provider_name, model)) in candidates.iter().enumerate() {
        match build_default_provider(provider_name, Some(model.to_string()), api_key.to_string(), None, validate) {
            Ok(provider) => {
                if i > 0 {
                    eprintln!("⚠️  Fallback: switched to {provider_name}/{model}");
                }
                return Ok((provider, provider_name.to_string()));
            }
            Err(e) => {
                last_error = format!("{provider_name}: {e}");
            }
        }
    }
    
    Err(format!("All providers failed:\n{last_error}"))
}

#[cfg(test)]
mod fallback_tests {
    use super::*;

    #[test]
    fn test_fallback_chain_empty_returns_error() {
        let result = fallback_chain(&[], "key", None, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_fallback_chain_invalid_provider_returns_all_errors() {
        let result = fallback_chain(
            &[("invalid-provider", "model")],
            "key", None, false,
        );
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_base_urls() {
        assert_eq!(default_base_url("openai"), "https://api.openai.com/v1");
        assert_eq!(
            default_base_url("anthropic"),
            "https://api.anthropic.com/v1"
        );
        assert_eq!(default_base_url("ollama"), "http://localhost:11434/v1");
        assert_eq!(default_base_url("unknown"), "https://api.openai.com/v1");
    }

    #[test]
    fn test_provider_config_new() {
        let cfg = ProviderConfig::new("openai", "sk-test", None, "gpt-4o");
        assert_eq!(cfg.provider_type, "openai");
        assert_eq!(cfg.api_key, "sk-test");
        assert!(cfg.base_url.is_none());
        assert_eq!(cfg.model, "gpt-4o");
    }

    #[test]
    fn test_resolved_base_url_with_none() {
        let cfg = ProviderConfig::new("ollama", "", None, "llama3");
        assert_eq!(cfg.resolved_base_url(), "http://localhost:11434/v1");
    }

    #[test]
    fn test_resolved_base_url_with_explicit() {
        let cfg = ProviderConfig::new(
            "openai",
            "sk-test",
            Some("http://127.0.0.1:8080/v1".into()),
            "local-model",
        );
        assert_eq!(cfg.resolved_base_url(), "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn test_env_var_for_type() {
        assert_eq!(
            ProviderConfig::env_var_for_type("openai"),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            ProviderConfig::env_var_for_type("anthropic"),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            ProviderConfig::env_var_for_type("ollama"),
            Some("OLLAMA_API_KEY")
        );
        assert_eq!(ProviderConfig::env_var_for_type("unknown"), None);
    }

    #[test]
    fn test_provider_settings_default() {
        let settings = ProviderSettings::default();
        assert!(settings.api_key.is_none());
        assert!(settings.base_url.is_none());
        assert!(settings.model.is_none());
    }

    #[test]
    fn test_provider_registry_new_is_empty() {
        let registry = ProviderRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_provider_registry_register_get() {
        let mut registry = ProviderRegistry::new();
        let provider = registry
            .register_openai("default", "sk-test", None, "gpt-4o")
            .unwrap();
        assert!(registry.get("default").is_some());
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
    }

    #[test]
    fn test_provider_registry_names() {
        let mut registry = ProviderRegistry::new();
        registry
            .register_openai("z-openai", "sk-test", None, "gpt-4o")
            .unwrap();
        registry
            .register_anthropic("a-anthropic", "sk-ant-test", None, "claude")
            .unwrap();
        registry
            .register_ollama("m-ollama", None, "llama3")
            .unwrap();
        let names = registry.names();
        assert_eq!(names.len(), 3);
        // Should be sorted alphabetically.
        assert_eq!(names[0], "a-anthropic");
        assert_eq!(names[1], "m-ollama");
        assert_eq!(names[2], "z-openai");
    }

    #[test]
    fn test_build_provider_unknown_type() {
        let cfg = ProviderConfig::new("nonexistent", "", None, "model");
        let result = build_provider(cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unsupported provider"));
    }

    #[test]
    fn test_build_provider_openai() {
        let cfg = ProviderConfig::new("openai", "sk-test", None, "gpt-4o");
        assert!(cfg.provider_type == "openai");
        let provider = build_provider(cfg).unwrap();
        let _ = provider;
    }

    #[test]
    fn test_provider_config_from_env_falls_back_to_config() {
        // When env vars are absent, it should use config overrides.
        let cfg = ProviderConfig::from_env(
            "openai",
            None,
            "".to_string(),
            Some(&ProviderSettings {
                api_key: Some("cfg-key".into()),
                base_url: Some("http://cfg-url/v1".into()),
                model: Some("cfg-model".into()),
            }),
        );
        assert_eq!(cfg.api_key, "cfg-key");
        assert_eq!(cfg.base_url.as_deref(), Some("http://cfg-url/v1"));
        assert_eq!(cfg.model, "cfg-model");
    }

    #[test]
    fn test_provider_config_from_env_no_overrides() {
        // When nothing is set, it should use defaults (empty api_key, None base_url).
        let cfg = ProviderConfig::from_env("ollama", None, "llama3".into(), None);
        assert_eq!(cfg.api_key, "");
        assert!(cfg.base_url.is_none());
        assert_eq!(cfg.model, "llama3");
    }

    #[test]
    fn test_build_default_provider_empty_model_error() {
        let result = build_default_provider("openai", None, "".into(), None, false);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no model specified"));
    }

    #[test]
    fn test_registry_builder() {
        let registry = ProviderRegistry::builder()
            .register_openai("default", "sk-test", None, "gpt-4o")
            .register_anthropic("claude", "sk-ant-test", None, "claude-sonnet-4")
            .register_ollama("local", None, "llama3")
            .build()
            .unwrap();

        assert_eq!(registry.len(), 3);
        assert!(registry.get("default").is_some());
        assert!(registry.get("claude").is_some());
        assert!(registry.get("local").is_some());
    }

    #[test]
    fn test_registry_builder_error() {
        let result = ProviderRegistry::builder()
            .register("bad", ProviderConfig::new("nonexistent", "", None, "model"))
            .build();
        assert!(result.is_err());
    }

    #[test]
    fn test_from_iter() {
        let registry: ProviderRegistry = Vec::<(String, Arc<dyn ModelProvider>)>::new()
            .into_iter()
            .collect();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_provider_config_deserialize() {
        let toml_str = r#"
            provider_type = "openai"
            api_key = "sk-test"
            model = "gpt-4o"
        "#;
        let cfg: ProviderConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.provider_type, "openai");
        assert_eq!(cfg.api_key, "sk-test");
        assert!(cfg.base_url.is_none());
        assert_eq!(cfg.model, "gpt-4o");
    }

    #[test]
    fn test_provider_config_deserialize_with_base_url() {
        let toml_str = r#"
            provider_type = "ollama"
            api_key = ""
            base_url = "http://localhost:11434/v1"
            model = "llama3"
        "#;
        let cfg: ProviderConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.provider_type, "ollama");
        assert_eq!(cfg.resolved_base_url(), "http://localhost:11434/v1");
    }

    #[test]
    fn test_openai_provider_is_created() {
        let cfg = ProviderConfig::new("openai", "sk-test", None, "gpt-4o");
        let provider_type = cfg.provider_type.clone();
        let provider = build_provider(cfg).unwrap();
        let _ = provider;
        assert_eq!(provider_type, "openai");
    }

    #[test]
    fn test_anthropic_provider_is_created() {
        let cfg = ProviderConfig::new("anthropic", "sk-ant-test", None, "claude-sonnet-4");
        let provider_type = cfg.provider_type.clone();
        let provider = build_provider(cfg).unwrap();
        let _ = provider;
        assert_eq!(provider_type, "anthropic");
    }

    #[test]
    fn test_ollama_provider_is_created() {
        let cfg = ProviderConfig::new("ollama", "", None, "llama3");
        let provider_type = cfg.provider_type.clone();
        let provider = build_provider(cfg).unwrap();
        let _ = provider;
        assert_eq!(provider_type, "ollama");
    }

    #[test]
    fn test_provider_config_serialize_deserialize_roundtrip() {
        let cfg = ProviderConfig::new("anthropic", "sk-ant-test", None, "claude-sonnet-4");
        let json = serde_json::to_string(&cfg).unwrap();
        let deserialized: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.provider_type, "anthropic");
        assert_eq!(deserialized.api_key, "sk-ant-test");
        assert_eq!(deserialized.model, "claude-sonnet-4");
    }
}
