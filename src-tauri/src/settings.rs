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

    /// Resolve a bearer token: explicit field first, then `gh auth token --hostname`.
    pub fn resolve_token(&self) -> Option<String> {
        if !self.token.is_empty() {
            return Some(self.token.clone());
        }
        let hostname = if self.hostname.is_empty() {
            "github.com"
        } else {
            &self.hostname
        };
        let output = std::process::Command::new("gh")
            .args(["auth", "token", "--hostname", hostname])
            .output()
            .ok()?;
        if output.status.success() {
            let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !tok.is_empty() {
                return Some(tok);
            }
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,
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

    // ── legacy single-server fields (migrated on first load) ──────────────────
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_token: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub github_server: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub github_orgs: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_base_url: "http://localhost:5000".to_string(),
            ai_model: "auto".to_string(),
            ai_api_key: String::new(),
            theme: "system".to_string(),
            hotkey: "Alt+Space".to_string(),
            max_results: 10,
            background_url: String::new(),
            plugin_dirs: vec![],
            github_servers: vec![],
            github_token: String::new(),
            github_server: String::new(),
            github_orgs: vec![],
        }
    }
}

pub fn settings_path() -> std::path::PathBuf {
    let config_dir = dirs::home_dir().unwrap_or_default().join(".config");
    config_dir.join("omnilauncher").join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(mut s) = serde_json::from_str::<AppSettings>(&content) {
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
        }
    }
    let mut s = AppSettings::default();
    // Auto-detect gh CLI authenticated hosts for fresh installs
    s.github_servers = detect_gh_hosts();
    s
}

/// Parse `gh auth status` output to discover authenticated hostnames.
/// Each un-indented line is a hostname (e.g. "github.com", "github.mycompany.com").
pub fn detect_gh_hosts() -> Vec<GitHubServer> {
    let output = std::process::Command::new("gh")
        .args(["auth", "status"])
        .output();
    let output = match output {
        Ok(o) => o,
        Err(_) => return vec![],
    };
    // gh auth status writes to stderr
    let text = String::from_utf8_lossy(&output.stderr);
    let mut servers = Vec::new();
    for line in text.lines() {
        // Hostnames appear at the start of the line (no leading whitespace)
        if !line.starts_with(' ') && !line.starts_with('\t') && !line.trim().is_empty() {
            let hostname = line.trim().to_string();
            // Skip lines that look like error messages or status indicators
            if hostname.contains(' ') || hostname.starts_with('✓') || hostname.starts_with('✗') {
                continue;
            }
            servers.push(GitHubServer {
                hostname,
                api_base: String::new(),
                token: String::new(),
                orgs: vec![],
            });
        }
    }
    servers
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

