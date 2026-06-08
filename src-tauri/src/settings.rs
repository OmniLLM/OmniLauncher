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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,
    #[serde(default = "default_ai_timeout_secs")]
    pub ai_timeout_secs: u64,
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

    /// Bearer/auth token the desktop shell sends to the separated backend.
    /// Used when the backend runs on a different machine (e.g. WSL backend +
    /// Windows shell) and the per-launch token file under `~/.config` is not
    /// readable by the shell. Resolution order on the shell side:
    ///   1. `OMNILAUNCHER_AUTH_TOKEN` env override
    ///   2. this field
    ///   3. `~/.config/omnilauncher/server-token` (legacy same-machine path)
    /// On the backend side, when `OMNILAUNCHER_AUTH_TOKEN` is set it pins the
    /// per-launch token to that value (instead of a fresh random one), so both
    /// ends can agree on a stable, user-configured token.
    #[serde(default)]
    pub backend_token: String,

    // ── legacy single-server fields (migrated on first load) ──────────────────
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_server: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub github_orgs: Vec<String>,
}

impl AppSettings {
    /// Effective AI API key: the value stored in settings, or — when empty —
    /// the `OMNILLM_API_KEY` env var. Returns an empty string when neither is
    /// set. The env var is read on every call so updates take effect without
    /// a restart and we never persist it to disk.
    pub fn resolve_ai_api_key(&self) -> String {
        if !self.ai_api_key.is_empty() {
            return self.ai_api_key.clone();
        }
        std::env::var("OMNILLM_API_KEY").unwrap_or_default()
    }
}

/// Resolve the backend auth token in the same order used by the desktop shell
/// and the HTTP server:
///   1. `OMNILAUNCHER_AUTH_TOKEN` env override
///   2. `settings.backend_token`
///   3. `~/.config/omnilauncher/server-token` (same-machine fallback)
///
/// The helper is shared by the shell and server so a settings save can rotate
/// the live token without duplicating precedence logic.
pub fn resolve_backend_auth_token(settings: &AppSettings) -> String {
    if let Ok(token) = std::env::var("OMNILAUNCHER_AUTH_TOKEN") {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let from_settings = settings.backend_token.trim();
    if !from_settings.is_empty() {
        return from_settings.to_string();
    }
    settings_path()
        .with_file_name("server-token")
        .to_str()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_base_url: "http://127.0.0.1:5000".to_string(),
            ai_model: "auto".to_string(),
            ai_api_key: String::new(),
            ai_timeout_secs: default_ai_timeout_secs(),
            theme: "system".to_string(),
            hotkey: "Ctrl+Shift+O".to_string(),
            max_results: 10,
            background_url: String::new(),
            plugin_dirs: vec![],
            github_servers: vec![],
            capture_selection_on_open: false,
            backend_url: String::new(),
            backend_token: String::new(),
            github_token: String::new(),
            github_server: String::new(),
            github_orgs: vec![],
        }
    }
}

pub fn settings_path() -> std::path::PathBuf {
    let config_dir = dirs::home_dir()
        .unwrap_or_else(|| {
            log::warn!("Could not determine home directory; using current directory for settings");
            std::path::PathBuf::from(".")
        })
        .join(".config");
    config_dir.join("omnilauncher").join("settings.json")
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
                    return s;
                }
                Err(err) => {
                    log::warn!("Failed to parse settings from {}: {err}", path.display());
                }
            },
            Err(err) => {
                log::warn!("Failed to read settings from {}: {err}", path.display());
            }
        }
    } else {
        log::info!(
            "Settings file does not exist at {}; using defaults",
            path.display()
        );
    }
    // Auto-detect gh CLI authenticated hosts for fresh installs
    AppSettings {
        github_servers: detect_gh_hosts(),
        ..AppSettings::default()
    }
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
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    match serde_json::to_string_pretty(settings) {
        Ok(json) => std::fs::write(&path, json).is_ok(),
        Err(_) => false,
    }
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
    fn test_default_settings_values() {
        let s = AppSettings::default();
        assert_eq!(s.theme, "system");
        assert_eq!(s.hotkey, "Ctrl+Shift+O");
        assert_eq!(s.max_results, 10);
        assert_eq!(s.ai_timeout_secs, 120);
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
}
