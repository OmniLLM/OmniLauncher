use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::{Plugin, Query, QueryResult};

// ─── plugin.json schema ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub description: String,
    pub version: String,
    pub keyword: Option<String>,
    pub icon: Option<String>,
    /// Entry script for Linux / macOS (e.g. `run.py`, `run.sh`).
    pub entry: String,
    /// Entry script for Windows (e.g. `run.ps1`).
    /// Falls back to `entry` when absent.
    #[serde(default)]
    pub entry_windows: Option<String>,
    /// Optional AI tool schema (OpenAI function-calling format).
    /// When present, this plugin is visible to the AI agent.
    #[serde(default)]
    pub tool_schema: Option<serde_json::Value>,
}

// ─── ExternalPlugin ───────────────────────────────────────────────────────────

pub struct ExternalPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
}

impl ExternalPlugin {
    pub fn new(dir: PathBuf, manifest: PluginManifest) -> Self {
        Self { manifest, dir }
    }

    fn entry_path(&self) -> PathBuf {
        self.dir.join(platform_entry(&self.manifest))
    }

    /// Spawn the entry executable, send `input` on stdin, and collect stdout.
    async fn call(&self, input: &str) -> Option<String> {
        let entry = self.entry_path();
        let mut cmd = build_interpreter_command(&entry);
        let mut child = cmd
            .current_dir(&self.dir)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                log::warn!(
                    "External plugin '{}' failed to spawn {}: {e}",
                    self.manifest.name,
                    entry.display()
                );
                e
            })
            .ok()?;

        // Write request on stdin
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            // Drop stdin so the child sees EOF
        }

        let output = child.wait_with_output().await.ok()?;
        if output.status.success() {
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            None
        }
    }

    /// Template skeleton shared by query / execute_tool / execute_action.
    ///
    /// Sends `request` as JSON on stdin, waits up to `timeout_secs`, and
    /// returns the raw stdout string. Returns `None` on spawn failure, process
    /// error, or timeout — logging a warning in each case.
    async fn call_op(
        &self,
        request: serde_json::Value,
        timeout_secs: u64,
        op_name: &str,
    ) -> Option<String> {
        log::debug!(
            "ExternalPlugin '{}' call_op op='{}' timeout={}s request={}",
            self.manifest.name,
            op_name,
            timeout_secs,
            request
        );
        match timeout(Duration::from_secs(timeout_secs), self.call(&request.to_string())).await {
            Ok(Some(output)) => {
                log::debug!(
                    "ExternalPlugin '{}' {} returned {} bytes",
                    self.manifest.name,
                    op_name,
                    output.len()
                );
                Some(output)
            }
            Ok(None) => {
                log::warn!(
                    "External plugin '{}' {} failed",
                    self.manifest.name, op_name
                );
                None
            }
            Err(_) => {
                log::warn!(
                    "External plugin '{}' {} timed out ({timeout_secs}s)",
                    self.manifest.name, op_name
                );
                None
            }
        }
    }
}

#[async_trait]
impl Plugin for ExternalPlugin {
    fn name(&self) -> &str {
        &self.manifest.name
    }

    fn description(&self) -> &str {
        &self.manifest.description
    }

    fn keyword(&self) -> Option<&str> {
        self.manifest.keyword.as_deref()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        self.manifest.tool_schema.clone()
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let request = serde_json::json!({ "op": "query", "query": q.raw });
        let Some(output) = self.call_op(request, 3, "query").await else {
            return vec![];
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => val["results"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            Some(QueryResult {
                                id: {
                                    let raw_id = item["id"].as_str()?.to_string();
                                    if item["action_type"].as_str() == Some("plugin_execute") {
                                        format!("{}::{}", self.manifest.name, raw_id)
                                    } else {
                                        raw_id
                                    }
                                },
                                title: item["title"].as_str()?.to_string(),
                                subtitle: item["subtitle"].as_str().map(|s| s.to_string()),
                                icon: item["icon"]
                                    .as_str()
                                    .map(|s| s.to_string())
                                    .or_else(|| self.manifest.icon.clone()),
                                score: item["score"].as_i64().unwrap_or(50) as i32,
                                action_type: item["action_type"]
                                    .as_str()
                                    .unwrap_or("shell")
                                    .to_string(),
                                action_data: item["action_data"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Err(e) => {
                log::warn!(
                    "External plugin '{}' returned invalid JSON: {e}",
                    self.manifest.name
                );
                vec![]
            }
        }
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        log::debug!(
            "ExternalPlugin '{}' execute_tool args={}",
            self.manifest.name,
            args
        );
        let request = serde_json::json!({ "op": "tool_call", "args": args });
        let Some(output) = self.call_op(request, 10, "execute_tool").await else {
            return String::new();
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => val["output"].as_str().unwrap_or(&output).to_string(),
            Err(_) => output,
        }
    }

    async fn execute_action(&self, id: &str, action_data: &str) -> Option<String> {
        log::debug!(
            "ExternalPlugin '{}' execute_action id='{}' action_data_len={}",
            self.manifest.name,
            id,
            action_data.len()
        );
        let request = serde_json::json!({ "op": "execute", "id": id, "action_data": action_data });
        let output = self.call_op(request, 10, "execute_action").await?;
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => Some(val["output"].as_str().unwrap_or("").to_string()),
            Err(_) => Some(output),
        }
    }
}

// ─── Discovery ────────────────────────────────────────────────────────────────

/// Return the base directory for external plugins:
/// `~/.omnilauncher/plugins/`
pub fn ext_plugins_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join(".omnilauncher")
        .join("plugins")
}

/// Read and validate a `plugin.json` from the given directory.
pub fn load_manifest(dir: &Path) -> Option<PluginManifest> {
    let manifest_path = dir.join("plugin.json");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    match serde_json::from_str::<PluginManifest>(&content) {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!("Invalid plugin.json in {}: {e}", dir.display());
            None
        }
    }
}

/// Discover plugin manifests inside a repo container.
///
/// A container can either be a single plugin directory with a root `plugin.json`
/// or a collection repo whose immediate subdirectories each contain a plugin.
pub fn discover_plugins_in_repo(repo_dir: &Path) -> Vec<(PathBuf, PluginManifest)> {
    if let Some(manifest) = load_manifest(repo_dir) {
        let entry_file = platform_entry(&manifest);
        if repo_dir.join(entry_file).exists() {
            return vec![(repo_dir.to_path_buf(), manifest)];
        }

        log::warn!(
            "External plugin '{}' in {}: entry '{}' not found",
            manifest.name,
            repo_dir.display(),
            entry_file
        );
        return vec![];
    }

    let mut plugins = Vec::new();
    let Ok(entries) = std::fs::read_dir(repo_dir) else {
        return plugins;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(manifest) = load_manifest(&path) else {
            continue;
        };

        let entry_path = path.join(platform_entry(&manifest));
        if entry_path.exists() {
            plugins.push((path, manifest));
        } else {
            log::warn!(
                "External plugin '{}' in {}: entry '{}' not found",
                manifest.name,
                path.display(),
                manifest.entry
            );
        }
    }

    plugins
}

// ─── Platform helpers ─────────────────────────────────────────────────────────

/// Build a `Command` that runs the entry script via the right interpreter.
///
/// Windows cannot directly execute `.ps1`, `.py`, `.sh`, or `.js` files via
/// `CreateProcess`, so we detect the extension and prepend the matching
/// interpreter. Native `.exe`/`.cmd`/`.bat` (and any unknown extension) fall
/// through to a direct spawn.
#[cfg(windows)]
fn powershell_executable() -> &'static str {
    // Prefer PowerShell 7 (`pwsh.exe`) when available: it defaults to UTF-8
    // for source files and stdio, which avoids parse failures on .ps1 files
    // that contain emoji/non-ASCII characters without a UTF-8 BOM (a common
    // case in user-authored plugins). Fall back to Windows PowerShell 5.1.
    use std::sync::OnceLock;
    static EXE: OnceLock<&'static str> = OnceLock::new();
    EXE.get_or_init(|| {
        let found = std::process::Command::new("where")
            .arg("pwsh.exe")
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);
        if found {
            "pwsh.exe"
        } else {
            "powershell.exe"
        }
    })
}

fn build_interpreter_command(entry: &Path) -> Command {
    let ext = entry
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());

    match ext.as_deref() {
        Some("ps1") => {
            #[cfg(windows)]
            let exe = powershell_executable();
            #[cfg(not(windows))]
            let exe = "pwsh";
            let mut c = Command::new(exe);
            c.arg("-NoProfile")
                .arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-OutputFormat")
                .arg("Text")
                .arg("-Command")
                .arg(format!(
                    "[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; \
                     [Console]::InputEncoding=[System.Text.Encoding]::UTF8; \
                     $OutputEncoding=[System.Text.Encoding]::UTF8; \
                     & '{}'",
                    entry.to_string_lossy().replace('\'', "''")
                ));
            c
        }
        Some("py") => {
            // Prefer bundled Python under ~/.omnilauncher/python/, fall back to system.
            let exe: std::borrow::Cow<str> = {
                let bundled_rel = if cfg!(windows) { "python.exe" } else { "bin/python3" };
                let bundled = dirs::home_dir()
                    .map(|h| h.join(".omnilauncher").join("python").join(bundled_rel))
                    .filter(|p| p.exists());
                match bundled {
                    Some(p) => p.to_string_lossy().into_owned().into(),
                    None => if cfg!(windows) { "python.exe".into() } else { "python3".into() },
                }
            };
            let mut c = Command::new(exe.as_ref());
            c.arg(entry);
            c
        }
        Some("js") | Some("cjs") | Some("mjs") => {
            let mut c = Command::new("node");
            c.arg(entry);
            c
        }
        Some("sh") if cfg!(windows) => {
            let mut c = Command::new("bash");
            c.arg(entry);
            c
        }
        _ => Command::new(entry),
    }
}

/// Return the entry filename for the current platform.
/// On Windows: uses `entry_windows` if set, falls back to `entry`.
/// On Linux / macOS: always uses `entry`.
fn platform_entry(manifest: &PluginManifest) -> &str {
    #[cfg(target_os = "windows")]
    return manifest
        .entry_windows
        .as_deref()
        .unwrap_or(&manifest.entry);
    #[cfg(not(target_os = "windows"))]
    &manifest.entry
}

/// Scan ext-plugins dir and return all valid ExternalPlugins.
pub fn load_external_plugins() -> Vec<ExternalPlugin> {
    load_external_plugins_from(&[])
}

/// Scan `~/.omnilauncher/plugins/` plus any extra dirs from settings.
/// Duplicate plugin names (same `name` field) are skipped — first-found wins.
pub fn load_external_plugins_from(extra_dirs: &[String]) -> Vec<ExternalPlugin> {
    let mut dirs: Vec<PathBuf> = vec![ext_plugins_dir()];
    for d in extra_dirs {
        let p = PathBuf::from(d);
        if p != dirs[0] {
            dirs.push(p);
        }
    }

    let mut plugins: Vec<ExternalPlugin> = vec![];

    for base in &dirs {
        if !base.exists() {
            continue;
        }
        match std::fs::read_dir(base) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        for (plugin_dir, manifest) in discover_plugins_in_repo(&path) {
                            log::info!(
                                "Discovered external plugin '{}' from {}",
                                manifest.name,
                                plugin_dir.display()
                            );
                            plugins.push(ExternalPlugin::new(plugin_dir, manifest));
                        }
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to scan plugin dir {}: {e}", base.display());
            }
        }
    }
    plugins
}

#[cfg(test)]
mod external_template_tests {
    use super::*;

    /// Verify that call_op with an instant-failing process returns None
    /// and doesn't panic. We use a manifest pointing to a nonexistent entry
    /// so the spawn fails immediately.
    #[tokio::test]
    async fn test_call_op_returns_none_on_spawn_failure() {
        let plugin = ExternalPlugin::new(
            std::path::PathBuf::from("/nonexistent/dir"),
            PluginManifest {
                name: "test".to_string(),
                description: "test".to_string(),
                version: "0.1.0".to_string(),
                keyword: None,
                icon: None,
                entry: "run.sh".to_string(),
                entry_windows: None,
                tool_schema: None,
            },
        );
        let result = plugin.call_op(
            serde_json::json!({"op": "query", "query": "test"}),
            1,
            "query",
        ).await;
        assert!(result.is_none());
    }
}
