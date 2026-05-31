use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::{Plugin, Query, QueryResult};

/// Number of consecutive failures after which an external plugin is quarantined.
/// Once quarantined, its `query` / `execute_tool` / `execute_action` calls
/// short-circuit (returning empty/None) until the next `reload_external_plugins`.
const QUARANTINE_THRESHOLD: u32 = 5;

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
    /// Optional per-plugin query timeout in milliseconds.
    /// Defaults to 3000ms, capped at 5000ms.
    #[serde(default)]
    pub query_timeout_ms: Option<u64>,
}

// ─── ExternalPlugin ───────────────────────────────────────────────────────────

pub struct ExternalPlugin {
    pub manifest: PluginManifest,
    pub dir: PathBuf,
    /// Count of consecutive failures (spawn failure, non-zero exit, timeout,
    /// invalid JSON). Reset to 0 on any success. When it reaches
    /// `QUARANTINE_THRESHOLD` the plugin is quarantined.
    consecutive_failures: AtomicU32,
}

impl ExternalPlugin {
    pub fn new(dir: PathBuf, manifest: PluginManifest) -> Self {
        Self {
            manifest,
            dir,
            consecutive_failures: AtomicU32::new(0),
        }
    }

    fn entry_path(&self) -> PathBuf {
        self.dir.join(platform_entry(&self.manifest))
    }

    /// True once this plugin has hit `QUARANTINE_THRESHOLD` consecutive failures.
    pub fn is_quarantined(&self) -> bool {
        self.consecutive_failures.load(Ordering::Relaxed) >= QUARANTINE_THRESHOLD
    }

    /// Record a successful call: reset the failure counter.
    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Record a failed call: increment the counter and, on crossing the
    /// quarantine threshold, log an error.
    fn record_failure(&self) {
        let prev = self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
        if prev + 1 == QUARANTINE_THRESHOLD {
            log::error!(
                "External plugin '{}' quarantined after {} consecutive failures",
                self.manifest.name,
                QUARANTINE_THRESHOLD
            );
        }
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

        // Write request on stdin, then drop it so the child sees EOF.
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes()).await;
            let _ = stdin.write_all(b"\n").await;
            drop(stdin);
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
    /// Sends `request` as JSON on stdin, waits up to `timeout_dur`, and returns
    /// the raw stdout string. Returns `None` on spawn failure, process error, or
    /// timeout — logging a warning in each case.
    async fn call_op(
        &self,
        request: serde_json::Value,
        timeout_dur: Duration,
        op_name: &str,
    ) -> Option<String> {
        log::debug!(
            "ExternalPlugin '{}' call_op op='{}' timeout={}ms request={}",
            self.manifest.name,
            op_name,
            timeout_dur.as_millis(),
            request
        );
        match timeout(timeout_dur, self.call(&request.to_string())).await {
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
                    self.manifest.name,
                    op_name
                );
                None
            }
            Err(_) => {
                log::warn!(
                    "External plugin '{}' {} timed out ({}ms)",
                    self.manifest.name,
                    op_name,
                    timeout_dur.as_millis()
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

    fn is_external(&self) -> bool {
        true
    }

    fn is_quarantined(&self) -> bool {
        ExternalPlugin::is_quarantined(self)
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        if self.is_quarantined() {
            return vec![];
        }
        let request = serde_json::json!({ "op": "query", "query": q.raw });
        let timeout_ms = self.manifest.query_timeout_ms.unwrap_or(3000).min(5000);
        let Some(output) = self
            .call_op(request, Duration::from_millis(timeout_ms), "query")
            .await
        else {
            self.record_failure();
            return vec![];
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => {
                self.record_success();
                val["results"]
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
                                    source: None,
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            }
            Err(e) => {
                log::warn!(
                    "External plugin '{}' returned invalid JSON: {e}",
                    self.manifest.name
                );
                self.record_failure();
                vec![]
            }
        }
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        if self.is_quarantined() {
            return String::new();
        }
        log::debug!(
            "ExternalPlugin '{}' execute_tool args={}",
            self.manifest.name,
            args
        );
        let request = serde_json::json!({ "op": "tool_call", "args": args });
        let Some(output) = self
            .call_op(request, Duration::from_secs(10), "execute_tool")
            .await
        else {
            self.record_failure();
            return String::new();
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => {
                self.record_success();
                val["output"].as_str().unwrap_or(&output).to_string()
            }
            Err(_) => {
                self.record_failure();
                output
            }
        }
    }

    async fn execute_action(&self, id: &str, action_data: &str) -> Option<String> {
        if self.is_quarantined() {
            return None;
        }
        log::debug!(
            "ExternalPlugin '{}' execute_action id='{}' action_data_len={}",
            self.manifest.name,
            id,
            action_data.len()
        );
        let request = serde_json::json!({ "op": "execute", "id": id, "action_data": action_data });
        let Some(output) = self
            .call_op(request, Duration::from_secs(10), "execute_action")
            .await
        else {
            self.record_failure();
            return None;
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => {
                self.record_success();
                Some(val["output"].as_str().unwrap_or("").to_string())
            }
            Err(_) => {
                self.record_failure();
                Some(output)
            }
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

/// Validate an optional AI tool schema (OpenAI function-calling format).
/// Required shape:
/// `{"type": "function", "function": {"name": <string>, "description": <string>, "parameters": <object>}}`
pub fn validate_tool_schema(schema: &serde_json::Value) -> Result<(), String> {
    if schema.get("type").and_then(|t| t.as_str()) != Some("function") {
        return Err("tool_schema.type must be \"function\"".to_string());
    }
    let function = schema
        .get("function")
        .ok_or_else(|| "tool_schema.function is missing".to_string())?;
    if function.get("name").and_then(|n| n.as_str()).is_none() {
        return Err("tool_schema.function.name must be a string".to_string());
    }
    if function.get("description").and_then(|d| d.as_str()).is_none() {
        return Err("tool_schema.function.description must be a string".to_string());
    }
    if !function.get("parameters").is_some_and(|p| p.is_object()) {
        return Err("tool_schema.function.parameters must be an object".to_string());
    }
    Ok(())
}

/// Validate a parsed manifest. Returns `Err(reason)` on the first problem.
fn validate_manifest(manifest: &PluginManifest) -> Result<(), String> {
    if manifest.name.is_empty() {
        return Err("name must be non-empty".to_string());
    }
    if manifest.name.contains(['/', '\\', '\0']) {
        return Err("name must not contain '/', '\\', or NUL".to_string());
    }
    if manifest.version.is_empty() {
        return Err("version must be non-empty".to_string());
    }
    if manifest.entry.is_empty() {
        return Err("entry must be non-empty".to_string());
    }
    if let Some(ref schema) = manifest.tool_schema {
        validate_tool_schema(schema)?;
    }
    Ok(())
}

/// Read and validate a `plugin.json` from the given directory.
pub fn load_manifest(dir: &Path) -> Option<PluginManifest> {
    let manifest_path = dir.join("plugin.json");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    let manifest = match serde_json::from_str::<PluginManifest>(&content) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("Invalid plugin.json in {}: {e}", dir.display());
            return None;
        }
    };
    if let Err(reason) = validate_manifest(&manifest) {
        log::warn!(
            "Invalid plugin.json in {}: {reason}",
            dir.display()
        );
        return None;
    }
    Some(manifest)
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
                let bundled_rel = if cfg!(windows) {
                    "python.exe"
                } else {
                    "bin/python3"
                };
                let bundled = dirs::home_dir()
                    .map(|h| h.join(".omnilauncher").join("python").join(bundled_rel))
                    .filter(|p| p.exists());
                match bundled {
                    Some(p) => p.to_string_lossy().into_owned().into(),
                    None => {
                        if cfg!(windows) {
                            "python.exe".into()
                        } else {
                            "python3".into()
                        }
                    }
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
    return manifest.entry_windows.as_deref().unwrap_or(&manifest.entry);
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

    // Refresh the Raycast shim files for any already-installed Raycast
    // extensions so shim improvements (bundled with the binary) propagate
    // without requiring a manual re-install. Cheap (a few file writes) and
    // a no-op for non-Raycast plugin directories.
    for base in &dirs {
        if base.exists() {
            let _ = super::raycast::synthesize_raycast_extensions_in(base);
        }
    }

    // Likewise refresh the Flow.Launcher shim (`flow-shim.cjs`) for any
    // already-installed Flow plugins. Synthesis is idempotent and overwrites
    // the shim, so fixes bundled with the binary (e.g. PYTHONPATH for the
    // plugin's bundled `lib/` deps) propagate without a manual re-install.
    for base in &dirs {
        if base.exists() {
            let _ = super::flow::synthesize_flow_plugins_in(base);
        }
    }

    // Collect candidate sub-directories first (cheap), then run
    // `discover_plugins_in_repo` (which does manifest read + JSON parse +
    // entry-file existence check) in parallel via rayon. Discovery is
    // independent per dir, so this is a clean fan-out that scales with the
    // number of installed plugins.
    use rayon::prelude::*;

    let mut candidate_dirs: Vec<PathBuf> = Vec::new();
    for base in &dirs {
        if !base.exists() {
            continue;
        }
        match std::fs::read_dir(base) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        candidate_dirs.push(path);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to scan plugin dir {}: {e}", base.display());
            }
        }
    }

    let plugins: Vec<ExternalPlugin> = candidate_dirs
        .par_iter()
        .flat_map_iter(|path| {
            discover_plugins_in_repo(path)
                .into_iter()
                .map(|(plugin_dir, manifest)| {
                    log::info!(
                        "Discovered external plugin '{}' from {}",
                        manifest.name,
                        plugin_dir.display()
                    );
                    ExternalPlugin::new(plugin_dir, manifest)
                })
                .collect::<Vec<_>>()
        })
        .collect();

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
                query_timeout_ms: None,
            },
        );
        let result = plugin
            .call_op(
                serde_json::json!({"op": "query", "query": "test"}),
                Duration::from_secs(1),
                "query",
            )
            .await;
        assert!(result.is_none());
    }

    fn write_manifest(dir: &std::path::Path, json: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.json"), json).unwrap();
    }

    #[test]
    fn load_manifest_rejects_missing_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"name":"","description":"d","version":"1.0.0","entry":"run.sh"}"#,
        );
        assert!(load_manifest(tmp.path()).is_none());
    }

    #[test]
    fn load_manifest_rejects_name_with_slash() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"name":"a/b","description":"d","version":"1.0.0","entry":"run.sh"}"#,
        );
        assert!(load_manifest(tmp.path()).is_none());
    }

    #[test]
    fn load_manifest_rejects_empty_version() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"name":"ok","description":"d","version":"","entry":"run.sh"}"#,
        );
        assert!(load_manifest(tmp.path()).is_none());
    }

    #[test]
    fn load_manifest_rejects_empty_entry() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"name":"ok","description":"d","version":"1.0.0","entry":""}"#,
        );
        assert!(load_manifest(tmp.path()).is_none());
    }

    #[test]
    fn load_manifest_rejects_malformed_tool_schema() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"name":"ok","description":"d","version":"1.0.0","entry":"run.sh",
                "tool_schema":{"type":"function","function":{"description":"x","parameters":{}}}}"#,
        );
        assert!(load_manifest(tmp.path()).is_none());
    }

    #[test]
    fn load_manifest_accepts_valid_manifest() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            r#"{"name":"ok","description":"d","version":"1.0.0","entry":"run.sh"}"#,
        );
        assert!(load_manifest(tmp.path()).is_some());
    }

    #[test]
    fn validate_tool_schema_accepts_well_formed() {
        let schema = serde_json::json!({
            "type": "function",
            "function": {
                "name": "do_thing",
                "description": "does a thing",
                "parameters": {"type": "object", "properties": {}}
            }
        });
        assert!(validate_tool_schema(&schema).is_ok());
    }

    #[test]
    fn validate_tool_schema_rejects_missing_function_name() {
        let schema = serde_json::json!({
            "type": "function",
            "function": {
                "description": "does a thing",
                "parameters": {"type": "object"}
            }
        });
        assert!(validate_tool_schema(&schema).is_err());
    }

    #[tokio::test]
    async fn quarantines_after_five_consecutive_failures() {
        // Plugin points at a nonexistent entry so every spawn fails.
        let plugin = ExternalPlugin::new(
            std::path::PathBuf::from("/nonexistent/dir"),
            PluginManifest {
                name: "flaky".to_string(),
                description: "always fails".to_string(),
                version: "0.1.0".to_string(),
                keyword: None,
                icon: None,
                entry: "run.sh".to_string(),
                entry_windows: None,
                tool_schema: None,
                query_timeout_ms: Some(500),
            },
        );

        let q = Query {
            raw: "x".to_string(),
            terms: vec!["x".to_string()],
        };

        assert!(!plugin.is_quarantined());
        for _ in 0..5 {
            assert!(plugin.query(&q).await.is_empty());
        }
        assert!(
            plugin.is_quarantined(),
            "expected quarantine after 5 failures"
        );

        // A 6th call short-circuits and stays quarantined.
        assert!(plugin.query(&q).await.is_empty());
        assert!(plugin.is_quarantined());
    }
}
