use serde::{Deserialize, Serialize};

/// A single GitHub server connection (github.com or GHE instance).
///
/// Token resolution order:
///   1. `token` field (if set explicitly)
///   2. `gh auth token --hostname <hostname>` (gh CLI credential store)
///
/// `hostname` examples: "github.com", "github.mycompany.com"
/// `api_base` is auto-derived from hostname unless overridden:
///   - "github.com" → "https://api.github.com"
///   - other       → "https://<hostname>/api/v3"
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitHubServer {
    pub hostname: String,
    /// Optional explicit API base URL override.
    #[serde(default)]
    pub api_base: String,
    /// Optional explicit token (skips gh CLI lookup when set).
    #[serde(default)]
    pub token: String,
    /// Organizations/owners to show in dashboard and default AI queries.
    #[serde(default)]
    pub orgs: Vec<String>,
}

impl GitHubServer {
    pub fn effective_api_base(&self) -> String {
        if !self.api_base.is_empty() {
            return self.api_base.trim_end_matches('/').to_string();
        }
        if self.hostname == "github.com" || self.hostname.is_empty() {
            "https://api.github.com".to_string()
        } else {
            format!("https://{}/api/v3", self.hostname)
        }
    }

    /// Resolve a bearer token in this order:
    ///   1. explicit `token` field
    ///   2. `gh auth token --hostname <host>` (works for keyring + file storage)
    ///   3. `oauth_token` field in gh's hosts.yml (file-stored tokens only)
    pub fn resolve_token(&self) -> Option<String> {
        if !self.token.is_empty() {
            return Some(self.token.clone());
        }
        let hostname = if self.hostname.is_empty() {
            "github.com"
        } else {
            self.hostname.as_str()
        };
        if let Ok(output) = std::process::Command::new(crate::gh_helper::gh_program())
            .args(["auth", "token", "--hostname", hostname])
            .env_remove("GITHUB_TOKEN")
            .env_remove("GH_TOKEN")
            .output()
        {
            if output.status.success() {
                let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !tok.is_empty() {
                    return Some(tok);
                }
            }
        }
        read_gh_hosts_yml()
            .into_iter()
            .find(|h| h.hostname == hostname)
            .and_then(|h| h.oauth_token)
    }
}

pub fn default_ai_timeout_secs() -> u64 {
    120
}

pub fn default_ai_max_tool_iterations() -> usize {
    10
}

pub fn default_ai_max_retry_attempts() -> u32 {
    3
}

pub fn default_ai_retry_base_delay_ms() -> u64 {
    2_000
}

pub fn default_ai_loop_detector_enabled() -> bool {
    true
}

pub fn default_a2a_port() -> u16 {
    1423
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    /// Any OpenAI-compatible endpoint. Requests go to `{base_url}/v1/chat/completions`.
    #[default]
    Custom,
    /// GitHub Copilot Chat. Authentication is handled through the Copilot token flow.
    GithubCopilot,
    /// Azure AI Foundry OpenAI-compatible endpoint. Requests go to `{base_url}/chat/completions`.
    AzureFoundry,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Custom => "custom",
            Self::GithubCopilot => "github-copilot",
            Self::AzureFoundry => "azure-foundry",
        };
        f.write_str(s)
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "custom" | "custom-provider" | "openai" | "openai-compatible" => Ok(Self::Custom),
            "github-copilot" | "copilot" | "github" => Ok(Self::GithubCopilot),
            "azure-foundry" | "foundry" | "azure" => Ok(Self::AzureFoundry),
            other => Err(format!(
                "unknown provider kind '{other}' (expected custom, github-copilot, azure-foundry)"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCaps {
    pub uses_copilot_auth: bool,
    pub requires_api_key: bool,
    pub auto_list_models: bool,
    pub manual_models: bool,
}

pub fn provider_caps(kind: ProviderKind) -> ProviderCaps {
    match kind {
        ProviderKind::Custom => ProviderCaps {
            uses_copilot_auth: false,
            requires_api_key: true,
            auto_list_models: true,
            manual_models: false,
        },
        ProviderKind::GithubCopilot => ProviderCaps {
            uses_copilot_auth: true,
            requires_api_key: false,
            auto_list_models: true,
            manual_models: false,
        },
        ProviderKind::AzureFoundry => ProviderCaps {
            uses_copilot_auth: false,
            requires_api_key: true,
            auto_list_models: false,
            manual_models: true,
        },
    }
}


/// Split a top-level `ai_model` string into an optional provider id and a
/// model string. The provider id is only recognized when the substring
/// before the FIRST '/' satisfies `is_known_provider`; otherwise the whole
/// string is returned as the model with no provider. This keeps model names
/// that themselves contain '/' (e.g. "jzhu/gpt-5.6-luna") intact when no
/// provider is named "jzhu".
pub fn parse_model_selector(
    ai_model: &str,
    is_known_provider: impl Fn(&str) -> bool,
) -> (Option<&str>, &str) {
    if let Some((prefix, rest)) = ai_model.split_once('/') {
        if is_known_provider(prefix) {
            return (Some(prefix), rest);
        }
    }
    (None, ai_model)
}

/// Normalize a model string, treating empty and "auto" (case-insensitive,
/// trimmed) as "not concrete". Returns the trimmed model when concrete.
fn concrete_model(model: &str) -> Option<&str> {
    let t = model.trim();
    if t.is_empty() || t.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(t)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub copilot_github_token: String,
    #[serde(default)]
    pub copilot_token: String,
    #[serde(default)]
    pub copilot_token_expiry: i64,
    #[serde(default)]
    pub copilot_enterprise_url: String,
}

impl Default for Provider {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            name: "Default".to_string(),
            kind: ProviderKind::Custom,
            base_url: "http://127.0.0.1:5000".to_string(),
            api_key: String::new(),
            model: "auto".to_string(),
            models: vec![],
            copilot_github_token: String::new(),
            copilot_token: String::new(),
            copilot_token_expiry: 0,
            copilot_enterprise_url: String::new(),
        }
    }
}

impl Provider {
    pub fn from_legacy(ai_base_url: String, ai_model: String, ai_api_key: String) -> Self {
        Self {
            base_url: ai_base_url,
            model: ai_model,
            api_key: ai_api_key,
            ..Self::default()
        }
    }

    pub fn masked_for_display(&self) -> Self {
        let mut p = self.clone();
        if !p.api_key.is_empty() {
            p.api_key = "«redacted»".to_string();
        }
        if !p.copilot_github_token.is_empty() {
            p.copilot_github_token = "«redacted»".to_string();
        }
        if !p.copilot_token.is_empty() {
            p.copilot_token = "«redacted»".to_string();
        }
        p
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,
    #[serde(default)]
    pub providers: Vec<Provider>,
    #[serde(default)]
    pub active_provider_id: String,
    #[serde(default = "default_ai_timeout_secs")]
    pub ai_timeout_secs: u64,
    #[serde(default = "default_ai_max_tool_iterations")]
    pub ai_max_tool_iterations: usize,
    #[serde(default = "default_ai_max_retry_attempts")]
    pub ai_max_retry_attempts: u32,
    #[serde(default = "default_ai_retry_base_delay_ms")]
    pub ai_retry_base_delay_ms: u64,
    /// When true (default), the agentic tool loop halts after detecting
    /// three identical (request, result) iterations in a row. Disable for
    /// advanced debugging of long multi-step skills — the iteration cap
    /// (`ai_max_tool_iterations`) is still the upper bound. See
    /// `openspec/specs/ai-chat/spec.md`, "User override for loop detector".
    #[serde(default = "default_ai_loop_detector_enabled")]
    pub ai_loop_detector_enabled: bool,
    pub theme: String,
    pub hotkey: String,
    pub max_results: usize,
    /// Custom background image URL shown in dark mode.
    #[serde(default)]
    pub background_url: String,
    /// Extra plugin directories to scan in addition to the default
    /// `~/.omnilauncher/plugins/`.  Each entry is an absolute path string.
    #[serde(default)]
    pub plugin_dirs: Vec<String>,
    /// GitHub server connections. Supports multiple servers (github.com + GHE).
    /// Each entry can have its own hostname, orgs, and optional explicit token.
    /// Tokens are resolved via `gh auth token --hostname` when not set explicitly.
    #[serde(default)]
    pub github_servers: Vec<GitHubServer>,

    /// When true, capturing the text selected in the previously-focused window
    /// is automatically pre-filled into the launcher (prefixed with `__sel__:`)
    /// each time it's shown. Off by default — turning it on enables the
    /// "highlight text → invoke launcher → see contextual actions" workflow.
    #[serde(default)]
    pub capture_selection_on_open: bool,

    /// Base URL of the separated backend the desktop shell connects to.
    /// Empty = use the `OMNILAUNCHER_BACKEND_URL` env override or the built-in
    /// default (`http://127.0.0.1:1422`).
    #[serde(default)]
    pub backend_url: String,

    // ── A2A server settings ─────────────────────────────────────────────────
    /// Enable the A2A (Agent-to-Agent) HTTP server. Off by default.
    #[serde(default)]
    pub a2a_enabled: bool,
    /// When true the A2A server binds `0.0.0.0` (LAN-accessible) instead of
    /// `127.0.0.1` (local-only). Advanced setting — off by default.
    #[serde(default)]
    pub a2a_bind_lan: bool,
    /// TCP port for the A2A server. Default 1423.
    #[serde(default = "default_a2a_port")]
    pub a2a_port: u16,
    /// Bearer token for A2A authentication. Auto-generated the first time the
    /// A2A server is enabled if absent. Stored in `settings.json` alongside
    /// other config (unlike the backend token which lives in a separate file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a2a_token: Option<String>,

    /// Optional externally reachable URL advertised in this A2A agent's card.
    /// Empty means `http://127.0.0.1:{a2a_port}` for same-machine clients.
    #[serde(default)]
    pub a2a_public_url: String,

    // ── legacy single-server fields (migrated on first load) ──────────────────
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_server: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub github_orgs: Vec<String>,
}

impl AppSettings {
    /// Effective AI API key. After `apply_env_overrides`, this is already the
    /// resolved value (env wins over settings.json). Kept as a method so
    /// callers don't need to change.
    pub fn resolve_ai_api_key(&self) -> String {
        self.ai_api_key.clone()
    }

    /// Active LLM provider. Falls back to the first provider if the active id is
    /// stale, then to the legacy flat `ai_*` fields for old settings files.
    pub fn active_provider(&self) -> Provider {
        self.providers
            .iter()
            .find(|p| p.id == self.active_provider_id)
            .or_else(|| self.providers.first())
            .cloned()
            .unwrap_or_else(|| {
                Provider::from_legacy(
                    self.ai_base_url.clone(),
                    self.ai_model.clone(),
                    self.resolve_ai_api_key(),
                )
            })
    }

    /// Find a provider by exact id without cloning. Shared by the active
    /// provider and selector resolution paths so id-matching lives in one place.
    fn provider_by_id(&self, id: &str) -> Option<&Provider> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Resolve the effective provider and concrete model to send, honoring a
    /// `provider-id/model` prefix in the top-level `ai_model`. A valid prefix
    /// overrides `active_provider_id`; the model falls back from the
    /// provider's own `model` to the selector model to "auto".
    pub fn resolve_active_selection(&self) -> (Provider, String) {
        let (prefix, selector_model) =
            parse_model_selector(&self.ai_model, |id| self.provider_by_id(id).is_some());

        let provider = prefix
            .and_then(|id| self.provider_by_id(id).cloned())
            .unwrap_or_else(|| self.active_provider());

        let effective = concrete_model(&provider.model)
            .or_else(|| concrete_model(selector_model))
            .unwrap_or("auto")
            .to_string();
        (provider, effective)
    }

    /// Keep legacy flat fields synchronized with the selected provider so old
    /// clients and endpoints that still read `ai_base_url` / `ai_model` keep
    /// working during the backend-only transition.
    pub fn sync_legacy_ai_fields_from_active_provider(&mut self) {
        let provider = self.active_provider();
        self.ai_base_url = provider.base_url;
        self.ai_model = provider.model;
        self.ai_api_key = provider.api_key;
        if self.active_provider_id.is_empty() {
            self.active_provider_id = provider.id;
        }
    }

    pub fn set_active_provider_base_url(&mut self, base_url: String) {
        self.ai_base_url = base_url.clone();
        self.ensure_provider_registry_without_sync();
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.id == self.active_provider_id)
        {
            provider.base_url = base_url;
        }
    }

    pub fn set_active_provider_model(&mut self, model: String) {
        self.ai_model = model.clone();
        self.ensure_provider_registry_without_sync();
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.id == self.active_provider_id)
        {
            provider.model = model;
        }
    }

    pub fn set_active_provider_api_key(&mut self, api_key: String) {
        self.ai_api_key = api_key.clone();
        self.ensure_provider_registry_without_sync();
        if let Some(provider) = self
            .providers
            .iter_mut()
            .find(|p| p.id == self.active_provider_id)
        {
            provider.api_key = api_key;
        }
    }

    fn ensure_provider_registry_without_sync(&mut self) {
        if self.providers.is_empty() {
            let provider = Provider::from_legacy(
                self.ai_base_url.clone(),
                self.ai_model.clone(),
                self.ai_api_key.clone(),
            );
            self.active_provider_id = provider.id.clone();
            self.providers.push(provider);
        } else if self.active_provider_id.is_empty()
            || !self
                .providers
                .iter()
                .any(|p| p.id == self.active_provider_id)
        {
            self.active_provider_id = self.providers[0].id.clone();
        }
    }

    pub fn ensure_provider_registry(&mut self) {
        self.ensure_provider_registry_without_sync();
        self.sync_legacy_ai_fields_from_active_provider();
    }
}

/// Resolve the backend auth token in the same order used by the desktop shell
/// and the HTTP server:
///   1. `OMNILAUNCHER_AUTH_TOKEN` env override
///   2. `~/.config/omnilauncher/server-token` (same-machine fallback)
///
/// User-entered frontend connection tokens are intentionally stored outside
/// `settings.json` and are not part of backend settings.
pub fn resolve_backend_auth_token(_settings: &AppSettings) -> String {
    if let Ok(token) = std::env::var("OMNILAUNCHER_AUTH_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    settings_path()
        .with_file_name("server-token")
        .to_str()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

// ── Universal settings overrides ────────────────────────────────────────────
//
// Precedence (highest → lowest):
//   1. CLI args   (`--ai-model=gpt-4`)
//   2. Env vars   (`OMNILAUNCHER_AI_MODEL=gpt-4`)
//   3. settings.json on disk
//   4. Hardcoded defaults
//
// Callers should apply in reverse order of priority so higher wins:
//   let mut s = load_settings();         // disk + defaults
//   apply_env_overrides(&mut s);         // env wins over disk
//   apply_cli_overrides(&mut s, &args);  // CLI wins over env

/// Mapping entry: env var name → field setter.
struct EnvOverride {
    /// Primary env var name, e.g. `OMNILAUNCHER_AI_MODEL`.
    var: &'static str,
    /// Legacy alias (checked only if primary is absent), e.g. `OMNILLM_API_KEY`.
    alias: Option<&'static str>,
    /// Apply the raw string value to the given settings.
    apply: fn(&mut AppSettings, &str),
}

/// Read the first non-empty value from `var`, then `alias` (if any).
fn read_env(var: &str, alias: Option<&str>) -> Option<String> {
    for name in std::iter::once(var).chain(alias) {
        if let Ok(val) = std::env::var(name) {
            let trimmed = val.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

/// All supported env var → field mappings.
///
/// Fields deliberately excluded:
/// - `github_servers` (`Vec<GitHubServer>`) — complex nested type, configured
///   via the UI or `gh` CLI auto-detect.
/// - `github_token`, `github_server`, `github_orgs` — legacy migration-only
///   fields superseded by `github_servers`.
fn env_overrides() -> Vec<EnvOverride> {
    vec![
        EnvOverride {
            var: "OMNILAUNCHER_AI_BASE_URL",
            alias: None,
            apply: |s, v| s.set_active_provider_base_url(v.to_string()),
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_MODEL",
            alias: None,
            apply: |s, v| s.set_active_provider_model(v.to_string()),
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_API_KEY",
            alias: Some("OMNILLM_API_KEY"),
            apply: |s, v| s.set_active_provider_api_key(v.to_string()),
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_TIMEOUT_SECS",
            alias: None,
            apply: |s, v| {
                if let Ok(n) = v.parse::<u64>() {
                    s.ai_timeout_secs = n;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_MAX_TOOL_ITERATIONS",
            alias: None,
            apply: |s, v| {
                if let Ok(n) = v.parse::<usize>() {
                    s.ai_max_tool_iterations = n;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_MAX_RETRY_ATTEMPTS",
            alias: None,
            apply: |s, v| {
                if let Ok(n) = v.parse::<u32>() {
                    s.ai_max_retry_attempts = n;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_RETRY_BASE_DELAY_MS",
            alias: None,
            apply: |s, v| {
                if let Ok(n) = v.parse::<u64>() {
                    s.ai_retry_base_delay_ms = n;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_AI_LOOP_DETECTOR_ENABLED",
            alias: None,
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.ai_loop_detector_enabled = b;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_THEME",
            alias: None,
            apply: |s, v| s.theme = v.to_string(),
        },
        EnvOverride {
            var: "OMNILAUNCHER_HOTKEY",
            alias: None,
            apply: |s, v| s.hotkey = v.to_string(),
        },
        EnvOverride {
            var: "OMNILAUNCHER_MAX_RESULTS",
            alias: None,
            apply: |s, v| {
                if let Ok(n) = v.parse::<usize>() {
                    s.max_results = n;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_BACKGROUND_URL",
            alias: None,
            apply: |s, v| s.background_url = v.to_string(),
        },
        EnvOverride {
            var: "OMNILAUNCHER_PLUGIN_DIRS",
            alias: None,
            apply: |s, v| {
                s.plugin_dirs = v
                    .split(':')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_CAPTURE_SELECTION_ON_OPEN",
            alias: None,
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.capture_selection_on_open = b;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_BACKEND_URL",
            alias: None,
            apply: |s, v| s.backend_url = v.to_string(),
        },
        EnvOverride {
            var: "OMNILAUNCHER_A2A_ENABLED",
            alias: None,
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.a2a_enabled = b;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_A2A_BIND_LAN",
            alias: None,
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.a2a_bind_lan = b;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_A2A_PORT",
            alias: None,
            apply: |s, v| {
                if let Ok(n) = v.parse::<u16>() {
                    s.a2a_port = n;
                }
            },
        },
        EnvOverride {
            var: "OMNILAUNCHER_A2A_TOKEN",
            alias: None,
            apply: |s, v| s.a2a_token = Some(v.to_string()),
        },
        EnvOverride {
            var: "OMNILAUNCHER_A2A_PUBLIC_URL",
            alias: None,
            apply: |s, v| s.a2a_public_url = v.to_string(),
        },
    ]
}

/// Apply environment variable overrides to settings. Fields for which the
/// corresponding env var is not set (or empty) are left untouched.
pub fn apply_env_overrides(settings: &mut AppSettings) {
    for entry in env_overrides() {
        if let Some(val) = read_env(entry.var, entry.alias) {
            log::info!("settings override: {} from env", entry.var);
            (entry.apply)(settings, &val);
        }
    }
}

/// CLI override mapping: `--flag-name` → field setter.
struct CliOverride {
    /// Long flag name without leading `--`, e.g. `ai-model`.
    flag: &'static str,
    /// Apply the raw string value to the given settings.
    apply: fn(&mut AppSettings, &str),
}

fn cli_overrides() -> Vec<CliOverride> {
    vec![
        CliOverride {
            flag: "ai-base-url",
            apply: |s, v| s.set_active_provider_base_url(v.to_string()),
        },
        CliOverride {
            flag: "ai-model",
            apply: |s, v| s.set_active_provider_model(v.to_string()),
        },
        CliOverride {
            flag: "ai-api-key",
            apply: |s, v| s.set_active_provider_api_key(v.to_string()),
        },
        CliOverride {
            flag: "ai-timeout-secs",
            apply: |s, v| {
                if let Ok(n) = v.parse::<u64>() {
                    s.ai_timeout_secs = n;
                }
            },
        },
        CliOverride {
            flag: "ai-max-tool-iterations",
            apply: |s, v| {
                if let Ok(n) = v.parse::<usize>() {
                    s.ai_max_tool_iterations = n;
                }
            },
        },
        CliOverride {
            flag: "ai-max-retry-attempts",
            apply: |s, v| {
                if let Ok(n) = v.parse::<u32>() {
                    s.ai_max_retry_attempts = n;
                }
            },
        },
        CliOverride {
            flag: "ai-retry-base-delay-ms",
            apply: |s, v| {
                if let Ok(n) = v.parse::<u64>() {
                    s.ai_retry_base_delay_ms = n;
                }
            },
        },
        CliOverride {
            flag: "ai-loop-detector-enabled",
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.ai_loop_detector_enabled = b;
                }
            },
        },
        CliOverride {
            flag: "theme",
            apply: |s, v| s.theme = v.to_string(),
        },
        CliOverride {
            flag: "hotkey",
            apply: |s, v| s.hotkey = v.to_string(),
        },
        CliOverride {
            flag: "max-results",
            apply: |s, v| {
                if let Ok(n) = v.parse::<usize>() {
                    s.max_results = n;
                }
            },
        },
        CliOverride {
            flag: "background-url",
            apply: |s, v| s.background_url = v.to_string(),
        },
        CliOverride {
            flag: "plugin-dirs",
            apply: |s, v| {
                s.plugin_dirs = v
                    .split(':')
                    .map(|p| p.trim().to_string())
                    .filter(|p| !p.is_empty())
                    .collect();
            },
        },
        CliOverride {
            flag: "capture-selection-on-open",
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.capture_selection_on_open = b;
                }
            },
        },
        CliOverride {
            flag: "backend-url",
            apply: |s, v| s.backend_url = v.to_string(),
        },
        CliOverride {
            flag: "a2a-enabled",
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.a2a_enabled = b;
                }
            },
        },
        CliOverride {
            flag: "a2a-bind-lan",
            apply: |s, v| {
                if let Ok(b) = parse_bool(v) {
                    s.a2a_bind_lan = b;
                }
            },
        },
        CliOverride {
            flag: "a2a-port",
            apply: |s, v| {
                if let Ok(n) = v.parse::<u16>() {
                    s.a2a_port = n;
                }
            },
        },
        CliOverride {
            flag: "a2a-token",
            apply: |s, v| s.a2a_token = Some(v.to_string()),
        },
        CliOverride {
            flag: "a2a-public-url",
            apply: |s, v| s.a2a_public_url = v.to_string(),
        },
    ]
}

/// Apply CLI argument overrides to settings. Accepts `--flag=value` or
/// `--flag value` syntax. Unknown flags are silently ignored (they may be
/// consumed by Tauri or other subsystems).
pub fn apply_cli_overrides(settings: &mut AppSettings, args: &[String]) {
    let overrides = cli_overrides();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !arg.starts_with("--") {
            i += 1;
            continue;
        }
        let stripped = &arg[2..]; // remove leading --

        // Try `--flag=value` first
        if let Some(eq_pos) = stripped.find('=') {
            let flag = &stripped[..eq_pos];
            let val = &stripped[eq_pos + 1..];
            if let Some(entry) = overrides.iter().find(|o| o.flag == flag) {
                log::info!("settings override: --{} from CLI", entry.flag);
                (entry.apply)(settings, val);
            }
            i += 1;
            continue;
        }

        // Try `--flag value` (next arg is the value)
        if let Some(entry) = overrides.iter().find(|o| o.flag == stripped) {
            if i + 1 < args.len() && !args[i + 1].starts_with("--") {
                log::info!("settings override: --{} from CLI", entry.flag);
                (entry.apply)(settings, &args[i + 1]);
                i += 2;
                continue;
            }
        }

        i += 1;
    }
}

/// Parse a string as a boolean. Accepts `true/false`, `1/0`, `yes/no`,
/// `on/off` (case-insensitive).
fn parse_bool(s: &str) -> Result<bool, ()> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(()),
    }
}

/// Load settings from disk and apply env + CLI overrides in precedence order.
/// This is the primary entry point for all startup paths.
pub fn load_settings_with_overrides(args: &[String]) -> AppSettings {
    let mut settings = load_settings();
    apply_env_overrides(&mut settings);
    apply_cli_overrides(&mut settings, args);
    settings
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_base_url: "http://127.0.0.1:5000".to_string(),
            ai_model: "auto".to_string(),
            ai_api_key: String::new(),
            providers: vec![Provider::default()],
            active_provider_id: "default".to_string(),
            ai_timeout_secs: default_ai_timeout_secs(),
            ai_max_tool_iterations: default_ai_max_tool_iterations(),
            ai_max_retry_attempts: default_ai_max_retry_attempts(),
            ai_retry_base_delay_ms: default_ai_retry_base_delay_ms(),
            ai_loop_detector_enabled: default_ai_loop_detector_enabled(),
            theme: "system".to_string(),
            hotkey: "Ctrl+Shift+O".to_string(),
            max_results: 10,
            background_url: String::new(),
            plugin_dirs: vec![],
            github_servers: vec![],
            capture_selection_on_open: false,
            backend_url: String::new(),
            a2a_enabled: false,
            a2a_bind_lan: false,
            a2a_port: default_a2a_port(),
            a2a_token: None,
            a2a_public_url: String::new(),
            github_token: String::new(),
            github_server: String::new(),
            github_orgs: vec![],
        }
    }
}

pub fn settings_path() -> std::path::PathBuf {
    crate::path_config::config_dir().join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    log::info!("Loading settings from {}", path.display());
    if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<AppSettings>(&content) {
                Ok(mut s) => {
                    // Migrate legacy single-server fields into github_servers
                    if s.github_servers.is_empty()
                        && (!s.github_token.is_empty()
                            || !s.github_server.is_empty()
                            || !s.github_orgs.is_empty())
                    {
                        let hostname = if s.github_server.is_empty() {
                            "github.com".to_string()
                        } else {
                            // strip scheme + /api/v3 to get hostname
                            s.github_server
                                .trim_start_matches("https://")
                                .trim_start_matches("http://")
                                .trim_end_matches("/api/v3")
                                .trim_end_matches('/')
                                .to_string()
                        };
                        s.github_servers.push(GitHubServer {
                            hostname,
                            api_base: String::new(),
                            token: s.github_token.clone(),
                            orgs: s.github_orgs.clone(),
                        });
                    }
                    // Auto-detect gh CLI authenticated hosts when no servers configured
                    if s.github_servers.is_empty() {
                        s.github_servers = detect_gh_hosts();
                    }
                    s.ensure_provider_registry();
                    log::info!("Settings loaded from existing file at {}", path.display());
                    return s;
                }
                Err(err) => {
                    log::warn!("Failed to parse settings from {}: {err}", path.display());
                    // Do NOT overwrite a malformed file — the user may want to fix
                    // it manually. Return defaults in memory only.
                }
            },
            Err(err) => {
                log::warn!("Failed to read settings from {}: {err}", path.display());
                // Do NOT overwrite an unreadable file — could be a transient
                // permissions issue. Return defaults in memory only.
            }
        }
    } else {
        log::info!(
            "Settings file does not exist at {}; creating with defaults",
            path.display()
        );
        // Create the file with defaults so the user has something to edit.
        // This is the ONLY path that creates settings.json automatically.
        let mut defaults = AppSettings {
            github_servers: detect_gh_hosts(),
            ..AppSettings::default()
        };
        defaults.ensure_provider_registry();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&defaults) {
            match std::fs::write(&path, &json) {
                Ok(()) => log::info!("Created default settings file at {}", path.display()),
                Err(err) => log::warn!(
                    "Failed to create default settings file at {}: {err}",
                    path.display()
                ),
            }
        }
        return defaults;
    }
    // Fallback: file exists but failed to read/parse — use defaults in memory
    // without touching the file on disk.
    let mut fallback = AppSettings {
        github_servers: detect_gh_hosts(),
        ..AppSettings::default()
    };
    fallback.ensure_provider_registry();
    fallback
}

/// Discover GitHub hostnames the user is authenticated to.
///
/// First checks an on-disk cache at `<data_dir>/cache/gh_hosts.json` —
/// `gh auth status` shells out to a child process and adds 100–500ms to
/// every cold start, so we cache its result for 24h. Pass through to the
/// live discovery path on cache miss / expiry / parse error.
///
/// Priority on cache miss:
///   1. `gh auth status` output (reads BOTH stdout and stderr — gh ≥2.40 writes
///      to stdout, older versions write to stderr).
///   2. Parse hostnames from `~/.config/gh/hosts.yml` (or `%APPDATA%\GitHub CLI\hosts.yml`
///      on Windows) — useful when `gh` isn't on PATH for the launched app.
pub fn detect_gh_hosts() -> Vec<GitHubServer> {
    if let Some(cached) = read_gh_hosts_cache() {
        return cached;
    }
    let fresh = detect_gh_hosts_uncached();
    write_gh_hosts_cache(&fresh);
    fresh
}

const GH_HOSTS_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

fn gh_hosts_cache_path() -> std::path::PathBuf {
    crate::path_config::data_dir()
        .join("cache")
        .join("gh_hosts.json")
}

fn read_gh_hosts_cache() -> Option<Vec<GitHubServer>> {
    let path = gh_hosts_cache_path();
    let meta = std::fs::metadata(&path).ok()?;
    let modified = meta.modified().ok()?;
    let age = std::time::SystemTime::now().duration_since(modified).ok()?;
    if age.as_secs() > GH_HOSTS_CACHE_TTL_SECS {
        return None;
    }
    let body = std::fs::read_to_string(&path).ok()?;
    let hostnames: Vec<String> = serde_json::from_str(&body).ok()?;
    Some(
        hostnames
            .into_iter()
            .map(|hostname| GitHubServer {
                hostname,
                api_base: String::new(),
                token: String::new(),
                orgs: vec![],
            })
            .collect(),
    )
}

fn write_gh_hosts_cache(servers: &[GitHubServer]) {
    let path = gh_hosts_cache_path();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let hostnames: Vec<&str> = servers.iter().map(|s| s.hostname.as_str()).collect();
    if let Ok(json) = serde_json::to_string(&hostnames) {
        let _ = std::fs::write(&path, json);
    }
}

fn detect_gh_hosts_uncached() -> Vec<GitHubServer> {
    let mut hostnames: Vec<String> = Vec::new();

    if let Ok(output) = std::process::Command::new(crate::gh_helper::gh_program())
        .args(["auth", "status"])
        .output()
    {
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for line in combined.lines() {
            // Hostnames appear at column 0 (no leading whitespace) and contain a dot.
            if line.starts_with(' ') || line.starts_with('\t') {
                continue;
            }
            let trimmed = line.trim().trim_end_matches(':');
            if trimmed.is_empty() || trimmed.contains(' ') {
                continue;
            }
            // Skip decorative glyphs (✓ ✗ ! etc) and obvious non-hostnames.
            let first = trimmed.chars().next().unwrap_or(' ');
            if !first.is_ascii_alphanumeric() {
                continue;
            }
            if !trimmed.contains('.') {
                continue;
            }
            if !hostnames.iter().any(|h| h == trimmed) {
                hostnames.push(trimmed.to_string());
            }
        }
    }

    // Fallback: read hostnames from gh's hosts.yml (top-level keys).
    if hostnames.is_empty() {
        for entry in read_gh_hosts_yml() {
            if !hostnames.iter().any(|h| h == &entry.hostname) {
                hostnames.push(entry.hostname);
            }
        }
    }

    hostnames
        .into_iter()
        .map(|hostname| GitHubServer {
            hostname,
            api_base: String::new(),
            token: String::new(),
            orgs: vec![],
        })
        .collect()
}

/// One entry parsed from gh's `hosts.yml` config file.
#[derive(Debug, Clone)]
pub struct GhHostEntry {
    pub hostname: String,
    pub oauth_token: Option<String>,
    #[allow(dead_code)]
    pub user: Option<String>,
}

/// Locate gh's hosts.yml across platforms. Honors `GH_CONFIG_DIR` when set.
fn gh_hosts_yml_path() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("GH_CONFIG_DIR") {
        let p = std::path::PathBuf::from(dir).join("hosts.yml");
        if p.exists() {
            return Some(p);
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let p = std::path::PathBuf::from(appdata)
                .join("GitHub CLI")
                .join("hosts.yml");
            if p.exists() {
                return Some(p);
            }
        }
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        let p = std::path::PathBuf::from(xdg).join("gh").join("hosts.yml");
        if p.exists() {
            return Some(p);
        }
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".config").join("gh").join("hosts.yml");
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Minimal hosts.yml parser. The file format is a flat map of `hostname:` blocks
/// containing 2-space-indented `key: value` pairs. We only care about
/// `oauth_token` and `user`; tokens stored in the OS keyring are absent here.
pub fn read_gh_hosts_yml() -> Vec<GhHostEntry> {
    let Some(path) = gh_hosts_yml_path() else {
        return vec![];
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return vec![];
    };
    let mut entries: Vec<GhHostEntry> = vec![];
    let mut current: Option<GhHostEntry> = None;
    for raw in content.lines() {
        let line = raw.trim_end();
        if line.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let is_top_level = !line.starts_with(' ') && !line.starts_with('\t');
        if is_top_level {
            if let Some(prev) = current.take() {
                entries.push(prev);
            }
            let host = line.trim_end_matches(':').trim().to_string();
            if !host.is_empty() {
                current = Some(GhHostEntry {
                    hostname: host,
                    oauth_token: None,
                    user: None,
                });
            }
        } else if let Some(entry) = current.as_mut() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("oauth_token:") {
                let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
                if !v.is_empty() {
                    entry.oauth_token = Some(v);
                }
            } else if let Some(rest) = trimmed.strip_prefix("user:") {
                let v = rest.trim().trim_matches('"').trim_matches('\'').to_string();
                if !v.is_empty() {
                    entry.user = Some(v);
                }
            }
        }
    }
    if let Some(prev) = current {
        entries.push(prev);
    }
    entries
}

pub fn save_settings(settings: &AppSettings) -> bool {
    let path = settings_path();
    // Defense-in-depth for the test suite: many tests mutate the process-global
    // `OMNILAUNCHER_CONFIG_DIR` to redirect writes to a temp dir, guarded by
    // `CONFIG_DIR_ENV_LOCK`. A single test that forgets the lock (or a race in
    // the set/remove window) would otherwise let `save_settings` overwrite the
    // user's real `~/.config/omnilauncher/settings.json`. In test builds we
    // therefore refuse to write unless the override is set, so no test can ever
    // clobber real user config regardless of lock discipline.
    #[cfg(test)]
    {
        if std::env::var_os("OMNILAUNCHER_CONFIG_DIR").is_none() {
            log::warn!(
                "save_settings refused: OMNILAUNCHER_CONFIG_DIR unset in test build \
                 (would have written real config at {})",
                path.display()
            );
            return false;
        }
    }
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    // Defense-in-depth: write a sibling `.bak` of whatever is currently on
    // disk before we overwrite. Cheap (one extra copy per Save click) and
    // recovers the user instantly from any future bug that writes hardcoded
    // defaults on top of real settings — historically the no.1 way users
    // lost configuration (frontend silent-fallback → POST defaults). The
    // backup is best-effort; a copy failure does NOT block the save.
    if path.exists() {
        let bak_path = path.with_extension("json.bak");
        if let Err(err) = std::fs::copy(&path, &bak_path) {
            log::warn!(
                "settings backup failed (continuing with save): {} -> {} ({})",
                path.display(),
                bak_path.display(),
                err
            );
        }
    }
    let mut normalized = settings.clone();
    normalized.ensure_provider_registry();
    match serde_json::to_string_pretty(&normalized) {
        Ok(json) => write_private_file(&path, json.as_bytes()),
        Err(_) => false,
    }
}

/// Write a secret-bearing file with owner-only permissions where the platform
/// supports POSIX modes. Settings can contain provider API keys and A2A bearer
/// tokens; the backend token file is also written through this helper.
pub fn write_private_file(path: &std::path::Path, bytes: &[u8]) -> bool {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let result = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .and_then(|mut f| f.write_all(bytes));
        if result.is_ok() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
            return true;
        }
        false
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes).is_ok()
    }
}

/// Returns true when `candidate` looks like the factory-default settings —
/// the exact shape the frontend's old silent-fallback used to produce when
/// `get_settings` failed. Used by the POST /api/settings handler to refuse
/// to overwrite a customized in-memory state with what is almost certainly
/// a UI-bug payload rather than an intentional save.
///
/// We intentionally do NOT use `*candidate == AppSettings::default()` because
/// a legitimate user CAN save a payload that matches defaults exactly (e.g.
/// they reset every field on purpose). The check below combines the default
/// shape with empty API key + empty backend URL + no plugin dirs + no github
/// servers — the combination the old silent-fallback path emitted.
///
/// The base URL check accepts both `http://localhost:5000` (the old
/// frontend silent-fallback value) and `http://127.0.0.1:5000` (the
/// AppSettings::default() value); either signals "default shape".
pub fn looks_like_factory_defaults(candidate: &AppSettings) -> bool {
    candidate.ai_api_key.is_empty()
        && candidate.backend_url.is_empty()
        && candidate.plugin_dirs.is_empty()
        && candidate.github_servers.is_empty()
        && candidate.background_url.is_empty()
        && (candidate.ai_base_url == "http://localhost:5000"
            || candidate.ai_base_url == "http://127.0.0.1:5000")
        && candidate.ai_model == "auto"
        && candidate.ai_timeout_secs == 120
        && candidate.ai_max_tool_iterations == 10
        && candidate.ai_max_retry_attempts == 3
        && candidate.ai_retry_base_delay_ms == 2_000
        && candidate.ai_loop_detector_enabled
        && candidate.theme == "system"
        && candidate.hotkey == "Ctrl+Shift+O"
        && candidate.max_results == 10
}

/// Returns true when `existing` clearly carries user customization — used to
/// gate the "refuse default-overwrite" guard. We err on the side of letting
/// a save through: only flag obvious customization (non-empty API key, custom
/// background, GitHub servers, plugin dirs, or backend URL) so legitimate
/// "reset everything" saves still work for users who never customized.
pub fn appears_customized(existing: &AppSettings) -> bool {
    !existing.ai_api_key.is_empty()
        || !existing.backend_url.is_empty()
        || !existing.background_url.is_empty()
        || !existing.plugin_dirs.is_empty()
        || !existing.github_servers.is_empty()
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn test_settings_path_is_non_empty() {
        let p = settings_path();
        assert!(!p.as_os_str().is_empty());
        assert!(p.to_string_lossy().contains("omnilauncher"));
    }

    #[test]
    fn settings_path_honors_omnilauncher_config_dir() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        let tmp = std::env::temp_dir().join(format!(
            "omnilauncher-settings-path-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", tmp.to_string_lossy().as_ref());

        assert_eq!(settings_path(), tmp.join("settings.json"));

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    }

    #[test]
    fn test_default_settings_values() {
        let s = AppSettings::default();
        assert_eq!(s.theme, "system");
        assert_eq!(s.hotkey, "Ctrl+Shift+O");
        assert_eq!(s.max_results, 10);
        assert_eq!(s.ai_timeout_secs, 120);
        assert_eq!(s.ai_max_tool_iterations, 10);
        assert_eq!(s.ai_max_retry_attempts, 3);
        assert_eq!(s.ai_retry_base_delay_ms, 2_000);
        assert!(s.ai_loop_detector_enabled);
    }

    #[test]
    fn test_serialized_settings_do_not_include_backend_token() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(!json.contains("backend_token"));
    }

    #[test]
    fn test_deserializes_missing_ai_timeout_to_default() {
        let json = r#"{
            "ai_base_url": "http://localhost:5000",
            "ai_model": "auto",
            "ai_api_key": "",
            "theme": "system",
            "hotkey": "Ctrl+Shift+O",
            "max_results": 10,
            "background_url": ""
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.ai_timeout_secs, 120);
        assert_eq!(s.ai_max_tool_iterations, 10);
        assert_eq!(s.ai_max_retry_attempts, 3);
        assert_eq!(s.ai_retry_base_delay_ms, 2_000);
        assert!(
            s.ai_loop_detector_enabled,
            "missing ai_loop_detector_enabled defaults to true (regression: would silently \
             disable safety guard on older settings files)"
        );
    }

    #[test]
    fn test_preserves_custom_ai_timeout() {
        let json = r#"{
            "ai_base_url": "http://localhost:5000",
            "ai_model": "auto",
            "ai_api_key": "",
            "ai_timeout_secs": 300,
            "theme": "system",
            "hotkey": "Ctrl+Shift+O",
            "max_results": 10,
            "background_url": ""
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.ai_timeout_secs, 300);
    }

    #[test]
    fn test_preserves_custom_ai_max_tool_iterations() {
        let json = r#"{
            "ai_base_url": "http://localhost:5000",
            "ai_model": "auto",
            "ai_api_key": "",
            "ai_timeout_secs": 300,
            "ai_max_tool_iterations": 25,
            "theme": "system",
            "hotkey": "Ctrl+Shift+O",
            "max_results": 10,
            "background_url": ""
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.ai_max_tool_iterations, 25);
    }

    #[test]
    fn test_preserves_custom_ai_retry_fields() {
        let json = r#"{
            "ai_base_url": "http://localhost:5000",
            "ai_model": "auto",
            "ai_api_key": "",
            "ai_max_retry_attempts": 5,
            "ai_retry_base_delay_ms": 500,
            "theme": "system",
            "hotkey": "Ctrl+Shift+O",
            "max_results": 10,
            "background_url": ""
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.ai_max_retry_attempts, 5);
        assert_eq!(s.ai_retry_base_delay_ms, 500);
    }

    #[test]
    fn test_preserves_custom_ai_loop_detector_field() {
        // Round-trip a settings JSON that explicitly disables the loop
        // detector, confirming the override is honoured and the rest of
        // the file is loaded unchanged. See
        // openspec/specs/ai-chat/spec.md, "User override for loop detector".
        let json = r#"{
            "ai_base_url": "http://localhost:5000",
            "ai_model": "auto",
            "ai_api_key": "",
            "ai_loop_detector_enabled": false,
            "theme": "system",
            "hotkey": "Ctrl+Shift+O",
            "max_results": 10,
            "background_url": ""
        }"#;
        let s: AppSettings = serde_json::from_str(json).unwrap();
        assert!(!s.ai_loop_detector_enabled, "explicit false honoured");
        // Sanity: other defaults still apply.
        assert_eq!(s.ai_timeout_secs, 120);
        assert_eq!(s.ai_max_tool_iterations, 10);
    }

    // ── Default-overwrite guard ────────────────────────────────────────
    // Regression coverage for the silent-fallback bug: the old frontend
    // catch path would substitute hardcoded defaults when get_settings
    // failed, then a subsequent Save would POST those defaults and wipe
    // a user's customized settings. These helpers gate that path.

    fn default_shaped_settings() -> AppSettings {
        // Matches the exact shape the old SettingsWindow.tsx silent-fallback
        // produced (and the AppSettings::default values otherwise).
        AppSettings {
            ai_base_url: "http://localhost:5000".to_string(),
            ai_model: "auto".to_string(),
            ai_api_key: String::new(),
            providers: vec![],
            active_provider_id: String::new(),
            ai_timeout_secs: 120,
            ai_max_tool_iterations: 10,
            ai_max_retry_attempts: 3,
            ai_retry_base_delay_ms: 2_000,
            ai_loop_detector_enabled: true,
            theme: "system".to_string(),
            hotkey: "Ctrl+Shift+O".to_string(),
            max_results: 10,
            background_url: String::new(),
            plugin_dirs: vec![],
            github_servers: vec![],
            capture_selection_on_open: false,
            backend_url: String::new(),
            a2a_enabled: false,
            a2a_bind_lan: false,
            a2a_port: default_a2a_port(),
            a2a_token: None,
            a2a_public_url: String::new(),
            github_token: String::new(),
            github_server: String::new(),
            github_orgs: vec![],
        }
    }

    #[test]
    fn looks_like_factory_defaults_matches_silent_fallback_shape() {
        assert!(looks_like_factory_defaults(&default_shaped_settings()));
    }

    #[test]
    fn looks_like_factory_defaults_rejects_customized_payload() {
        let mut s = default_shaped_settings();
        s.ai_api_key = "sk-real".to_string();
        assert!(!looks_like_factory_defaults(&s));

        let mut s = default_shaped_settings();
        s.background_url = "https://example.com/bg.png".to_string();
        assert!(!looks_like_factory_defaults(&s));

        let mut s = default_shaped_settings();
        s.backend_url = "http://10.0.0.5:1422".to_string();
        assert!(!looks_like_factory_defaults(&s));

        let mut s = default_shaped_settings();
        s.github_servers.push(GitHubServer {
            hostname: "github.com".to_string(),
            ..Default::default()
        });
        assert!(!looks_like_factory_defaults(&s));
    }

    #[test]
    fn looks_like_factory_defaults_rejects_tweaked_scalar_fields() {
        let mut s = default_shaped_settings();
        s.ai_timeout_secs = 300;
        assert!(!looks_like_factory_defaults(&s));

        let mut s = default_shaped_settings();
        s.max_results = 25;
        assert!(!looks_like_factory_defaults(&s));

        let mut s = default_shaped_settings();
        s.theme = "dark".to_string();
        assert!(!looks_like_factory_defaults(&s));

        let mut s = default_shaped_settings();
        s.hotkey = "Alt+Space".to_string();
        assert!(!looks_like_factory_defaults(&s));
    }

    #[test]
    fn appears_customized_detects_user_state() {
        let mut s = default_shaped_settings();
        assert!(!appears_customized(&s));

        s.ai_api_key = "sk-test".to_string();
        assert!(appears_customized(&s));

        let mut s = default_shaped_settings();
        s.backend_url = "http://backend.local:1422".to_string();
        assert!(appears_customized(&s));

        let mut s = default_shaped_settings();
        s.background_url = "https://example.com/bg.png".to_string();
        assert!(appears_customized(&s));

        let mut s = default_shaped_settings();
        s.plugin_dirs.push("/opt/plugins".to_string());
        assert!(appears_customized(&s));

        let mut s = default_shaped_settings();
        s.github_servers.push(GitHubServer {
            hostname: "github.com".to_string(),
            ..Default::default()
        });
        assert!(appears_customized(&s));
    }

    #[test]
    fn env_overrides_use_primary_ai_api_key_before_legacy_alias() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        std::env::set_var("OMNILAUNCHER_AI_API_KEY", " primary-key ");
        std::env::set_var("OMNILLM_API_KEY", "legacy-key");

        let mut s = default_shaped_settings();
        apply_env_overrides(&mut s);

        assert_eq!(s.ai_api_key, "primary-key");

        std::env::remove_var("OMNILAUNCHER_AI_API_KEY");
        std::env::remove_var("OMNILLM_API_KEY");
    }

    #[test]
    fn cli_overrides_win_over_env_overrides() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        std::env::set_var("OMNILAUNCHER_AI_MODEL", "env-model");
        std::env::set_var("OMNILAUNCHER_MAX_RESULTS", "12");

        let mut s = default_shaped_settings();
        apply_env_overrides(&mut s);
        apply_cli_overrides(
            &mut s,
            &[
                "omnilauncher".to_string(),
                "--ai-model=cli-model".to_string(),
                "--max-results".to_string(),
                "25".to_string(),
            ],
        );

        assert_eq!(s.ai_model, "cli-model");
        assert_eq!(s.max_results, 25);

        std::env::remove_var("OMNILAUNCHER_AI_MODEL");
        std::env::remove_var("OMNILAUNCHER_MAX_RESULTS");
    }

    #[test]
    fn cli_overrides_parse_booleans_ports_tokens_and_plugin_dirs() {
        let mut s = default_shaped_settings();

        apply_cli_overrides(
            &mut s,
            &[
                "omnilauncher".to_string(),
                "--ai-loop-detector-enabled=off".to_string(),
                "--capture-selection-on-open".to_string(),
                "yes".to_string(),
                "--a2a-enabled=1".to_string(),
                "--a2a-bind-lan".to_string(),
                "true".to_string(),
                "--a2a-port=1555".to_string(),
                "--a2a-token".to_string(),
                "cli-token".to_string(),
                "--plugin-dirs=/one:/two: :/three".to_string(),
            ],
        );

        assert!(!s.ai_loop_detector_enabled);
        assert!(s.capture_selection_on_open);
        assert!(s.a2a_enabled);
        assert!(s.a2a_bind_lan);
        assert_eq!(s.a2a_port, 1555);
        assert_eq!(s.a2a_token.as_deref(), Some("cli-token"));
        assert_eq!(s.plugin_dirs, vec!["/one", "/two", "/three"]);
    }

    #[test]
    fn load_settings_with_overrides_applies_disk_env_then_cli_precedence() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        let tmp = std::env::temp_dir().join(format!(
            "omnilauncher-overrides-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("test tmp dir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", tmp.to_string_lossy().as_ref());
        std::env::set_var("OMNILAUNCHER_AI_MODEL", "env-model");
        std::env::set_var("OMNILAUNCHER_A2A_TOKEN", "env-token");

        let mut disk = default_shaped_settings();
        disk.ai_model = "disk-model".to_string();
        disk.max_results = 7;
        std::fs::write(
            settings_path(),
            serde_json::to_string_pretty(&disk).unwrap(),
        )
        .unwrap();

        let s = load_settings_with_overrides(&[
            "omnilauncher".to_string(),
            "--max-results=33".to_string(),
            "--a2a-token".to_string(),
            "cli-token".to_string(),
        ]);

        assert_eq!(s.ai_model, "env-model");
        assert_eq!(s.max_results, 33);
        assert_eq!(s.a2a_token.as_deref(), Some("cli-token"));

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
        std::env::remove_var("OMNILAUNCHER_AI_MODEL");
        std::env::remove_var("OMNILAUNCHER_A2A_TOKEN");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Regression test for the "rebuild backend wipes settings.json" bug:
    /// save_settings must copy the existing file to `settings.json.bak` BEFORE
    /// overwriting, so the user can always recover the previous configuration
    /// if a bad payload (UI bug, accidental save) lands on disk.
    #[test]
    fn save_settings_creates_backup_of_previous_file() {
        // Override settings location to a temp dir so we don't touch the user's
        // real config. Hold the global env lock for the duration to keep
        // parallel tests from racing on OMNILAUNCHER_CONFIG_DIR.
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        // Use a unique dir per test invocation so leftover state from a prior
        // crashed run cannot make this test pass-then-fail spuriously.
        let tmp = std::env::temp_dir().join(format!(
            "omnilauncher-save-bak-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("test tmp dir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", tmp.to_string_lossy().as_ref());

        // Ensure no stale settings.json or .bak from prior runs.
        let path = settings_path();
        let bak_path = path.with_extension("json.bak");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&bak_path);

        // First save: nothing to back up yet.
        let mut s = default_shaped_settings();
        s.ai_api_key = "first-key".to_string();
        assert!(save_settings(&s));
        assert!(path.exists(), "settings.json should exist after first save");
        assert!(
            !bak_path.exists(),
            "no backup expected on first save (path={})",
            bak_path.display()
        );

        // Second save: the previous file must be copied to .bak first.
        let mut s2 = default_shaped_settings();
        s2.ai_api_key = "second-key".to_string();
        assert!(save_settings(&s2));
        assert!(
            bak_path.exists(),
            "settings.json.bak must exist after second save"
        );

        // The .bak content must reflect the FIRST save (so it's a true backup
        // of what was about to be overwritten, not a copy of the new value).
        let bak_body = std::fs::read_to_string(&bak_path).unwrap();
        assert!(
            bak_body.contains("first-key"),
            "backup must carry the previous value, got: {}",
            bak_body
        );
        assert!(
            !bak_body.contains("second-key"),
            "backup must NOT contain the just-saved value"
        );

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Regression test for the "restart overwrites config" bug:
    /// When settings.json already exists on disk, load_settings must read it
    /// and NOT overwrite it. Only when the file is absent should it create one.
    #[test]
    fn load_settings_reads_existing_file_without_overwriting() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        let tmp = std::env::temp_dir().join(format!(
            "omnilauncher-no-overwrite-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("test tmp dir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", tmp.to_string_lossy().as_ref());

        // Write a customized settings file to disk.
        let mut custom = default_shaped_settings();
        custom.ai_api_key = "user-secret-key".to_string();
        custom.ai_model = "my-custom-model".to_string();
        custom.max_results = 42;
        let custom_json = serde_json::to_string_pretty(&custom).unwrap();
        std::fs::write(settings_path(), &custom_json).unwrap();

        // load_settings must return the values from disk.
        let loaded = load_settings();
        assert_eq!(loaded.ai_api_key, "user-secret-key");
        assert_eq!(loaded.ai_model, "my-custom-model");
        assert_eq!(loaded.max_results, 42);

        // Verify the file on disk was NOT modified.
        let after = std::fs::read_to_string(settings_path()).unwrap();
        assert_eq!(
            after, custom_json,
            "settings.json on disk must not be modified by load_settings"
        );

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// When settings.json does not exist, load_settings must create it with
    /// defaults so the user has a file to edit.
    #[test]
    fn load_settings_creates_file_when_absent() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        let tmp = std::env::temp_dir().join(format!(
            "omnilauncher-create-missing-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("test tmp dir");
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", tmp.to_string_lossy().as_ref());

        let path = settings_path();
        assert!(!path.exists(), "precondition: file must not exist");

        let loaded = load_settings();
        // Should return defaults.
        assert_eq!(loaded.theme, "system");
        assert_eq!(loaded.hotkey, "Ctrl+Shift+O");
        assert_eq!(loaded.max_results, 10);

        // File must now exist on disk.
        assert!(
            path.exists(),
            "load_settings must create settings.json when it is absent"
        );

        // The file must be valid JSON that round-trips.
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: AppSettings = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.theme, "system");
        assert_eq!(parsed.max_results, 10);

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// Guard against the "test wiped my real config" incident: in test builds,
    /// `save_settings` must refuse to write when `OMNILAUNCHER_CONFIG_DIR` is
    /// unset, so no test — regardless of env-lock discipline — can ever
    /// overwrite the user's real `~/.config/omnilauncher/settings.json`.
    #[test]
    fn save_settings_refuses_real_config_path_in_tests() {
        let _guard = crate::path_config::CONFIG_DIR_ENV_LOCK.blocking_lock();
        // Ensure the override is absent for this check, then restore it after.
        let prev = std::env::var_os("OMNILAUNCHER_CONFIG_DIR");
        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");

        let saved = save_settings(&default_shaped_settings());

        if let Some(v) = prev {
            std::env::set_var("OMNILAUNCHER_CONFIG_DIR", v);
        }
        assert!(
            !saved,
            "save_settings must refuse to write the real config path when \
             OMNILAUNCHER_CONFIG_DIR is unset in test builds"
        );
    }


    #[test]
    fn selector_recognizes_known_provider_prefix() {
        let ids = ["copilot", "default"];
        let known = |id: &str| ids.contains(&id);
        assert_eq!(
            super::parse_model_selector("copilot/gpt-5.6-sol", known),
            (Some("copilot"), "gpt-5.6-sol")
        );
    }

    #[test]
    fn selector_keeps_slash_model_when_prefix_unknown() {
        let ids = ["copilot", "default"];
        let known = |id: &str| ids.contains(&id);
        assert_eq!(
            super::parse_model_selector("jzhu/gpt-5.6-luna", known),
            (None, "jzhu/gpt-5.6-luna")
        );
    }

    #[test]
    fn selector_bare_model_and_empty() {
        let known = |id: &str| id == "copilot";
        assert_eq!(super::parse_model_selector("gpt-5.6-sol", known), (None, "gpt-5.6-sol"));
        assert_eq!(super::parse_model_selector("", known), (None, ""));
    }

    #[test]
    fn selector_prefix_with_empty_model() {
        let known = |id: &str| id == "copilot";
        assert_eq!(super::parse_model_selector("copilot/", known), (Some("copilot"), ""));
    }

    fn provider_named(id: &str, model: &str) -> super::Provider {
        super::Provider {
            id: id.to_string(),
            name: id.to_string(),
            model: model.to_string(),
            ..super::Provider::default()
        }
    }

    fn settings_with(
        providers: Vec<super::Provider>,
        active: &str,
        ai_model: &str,
    ) -> super::AppSettings {
        let mut s = default_shaped_settings();
        s.providers = providers;
        s.active_provider_id = active.to_string();
        s.ai_model = ai_model.to_string();
        s
    }

    #[test]
    fn resolve_prefix_overrides_active_provider() {
        let s = settings_with(
            vec![
                provider_named("default", "m-default"),
                provider_named("copilot", "m-copilot"),
            ],
            "default",
            "copilot/gpt-5.6-sol",
        );
        let (p, model) = s.resolve_active_selection();
        assert_eq!(p.id, "copilot");
        assert_eq!(model, "m-copilot");
    }

    #[test]
    fn resolve_falls_back_to_selector_model_when_provider_auto() {
        let s = settings_with(
            vec![provider_named("copilot", "auto")],
            "copilot",
            "copilot/gpt-5.6-sol",
        );
        let (p, model) = s.resolve_active_selection();
        assert_eq!(p.id, "copilot");
        assert_eq!(model, "gpt-5.6-sol");
    }

    #[test]
    fn resolve_bare_model_used_as_fallback_on_active_provider() {
        let s = settings_with(
            vec![provider_named("copilot", "")],
            "copilot",
            "gpt-5.6-sol",
        );
        let (p, model) = s.resolve_active_selection();
        assert_eq!(p.id, "copilot");
        assert_eq!(model, "gpt-5.6-sol");
    }

    #[test]
    fn resolve_defaults_to_auto_when_nothing_concrete() {
        let s = settings_with(vec![provider_named("copilot", "auto")], "copilot", "auto");
        let (_p, model) = s.resolve_active_selection();
        assert_eq!(model, "auto");
    }
}
