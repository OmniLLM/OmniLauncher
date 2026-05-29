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
        if let Ok(output) = std::process::Command::new("gh")
            .args(["auth", "token", "--hostname", hostname])
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

    /// When true, capturing the text selected in the previously-focused window
    /// is automatically pre-filled into the launcher (prefixed with `__sel__:`)
    /// each time it's shown. Off by default — turning it on enables the
    /// "highlight text → invoke launcher → see contextual actions" workflow.
    #[serde(default)]
    pub capture_selection_on_open: bool,

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
            capture_selection_on_open: false,
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

/// Discover GitHub hostnames the user is authenticated to.
///
/// Priority:
///   1. `gh auth status` output (reads BOTH stdout and stderr — gh ≥2.40 writes
///      to stdout, older versions write to stderr).
///   2. Parse hostnames from `~/.config/gh/hosts.yml` (or `%APPDATA%\GitHub CLI\hosts.yml`
///      on Windows) — useful when `gh` isn't on PATH for the launched app.
pub fn detect_gh_hosts() -> Vec<GitHubServer> {
    let mut hostnames: Vec<String> = Vec::new();

    if let Ok(output) = std::process::Command::new("gh")
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
        assert_eq!(s.hotkey, "Alt+Space");
        assert_eq!(s.max_results, 10);
    }
}
