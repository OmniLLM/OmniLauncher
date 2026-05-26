use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
    pub entry: String,
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
        self.dir.join(&self.manifest.entry)
    }

    /// Spawn the entry executable, send `input` on stdin, and collect stdout.
    async fn call(&self, input: &str) -> Option<String> {
        let entry = self.entry_path();
        let mut child = Command::new(&entry)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
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

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let request = serde_json::json!({
            "op": "query",
            "query": q.raw,
        });
        let input = request.to_string();

        let result = timeout(Duration::from_secs(3), self.call(&input)).await;

        match result {
            Ok(Some(output)) => {
                // Parse {"results": [...]}
                match serde_json::from_str::<serde_json::Value>(&output) {
                    Ok(val) => {
                        val["results"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|item| {
                                        Some(QueryResult {
                                            id: item["id"].as_str()?.to_string(),
                                            title: item["title"].as_str()?.to_string(),
                                            subtitle: item["subtitle"]
                                                .as_str()
                                                .map(|s| s.to_string()),
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
                            .unwrap_or_default()
                    }
                    Err(e) => {
                        log::warn!(
                            "External plugin '{}' returned invalid JSON: {e}",
                            self.manifest.name
                        );
                        vec![]
                    }
                }
            }
            Ok(None) => {
                log::warn!("External plugin '{}' query failed", self.manifest.name);
                vec![]
            }
            Err(_) => {
                log::warn!(
                    "External plugin '{}' query timed out (3 s)",
                    self.manifest.name
                );
                vec![]
            }
        }
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let id = args["id"].as_str().unwrap_or("").to_string();
        let action_data = args["action_data"].as_str().unwrap_or("").to_string();

        let request = serde_json::json!({
            "op": "execute",
            "id": id,
            "action_data": action_data,
        });
        let input = request.to_string();

        match timeout(Duration::from_secs(10), self.call(&input)).await {
            Ok(Some(output)) => {
                // Parse {"output": "..."}
                match serde_json::from_str::<serde_json::Value>(&output) {
                    Ok(val) => val["output"]
                        .as_str()
                        .unwrap_or(&output)
                        .to_string(),
                    Err(_) => output,
                }
            }
            Ok(None) => {
                log::warn!(
                    "External plugin '{}' execute failed",
                    self.manifest.name
                );
                String::new()
            }
            Err(_) => {
                log::warn!(
                    "External plugin '{}' execute timed out (10 s)",
                    self.manifest.name
                );
                String::new()
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

/// Read and validate a `plugin.json` from the given directory.
pub fn load_manifest(dir: &PathBuf) -> Option<PluginManifest> {
    let manifest_path = dir.join("plugin.json");
    let content = std::fs::read_to_string(&manifest_path).ok()?;
    match serde_json::from_str::<PluginManifest>(&content) {
        Ok(m) => Some(m),
        Err(e) => {
            log::warn!(
                "Invalid plugin.json in {}: {e}",
                dir.display()
            );
            None
        }
    }
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
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for base in &dirs {
        if !base.exists() {
            continue;
        }
        match std::fs::read_dir(base) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(manifest) = load_manifest(&path) {
                            if seen_names.contains(&manifest.name) {
                                log::info!(
                                    "Skipping duplicate plugin '{}' from {}",
                                    manifest.name,
                                    path.display()
                                );
                                continue;
                            }
                            let entry_path = path.join(&manifest.entry);
                            if entry_path.exists() {
                                log::info!(
                                    "Loaded external plugin '{}' from {}",
                                    manifest.name,
                                    path.display()
                                );
                                seen_names.insert(manifest.name.clone());
                                plugins.push(ExternalPlugin::new(path, manifest));
                            } else {
                                log::warn!(
                                    "External plugin '{}': entry '{}' not found",
                                    manifest.name,
                                    manifest.entry
                                );
                            }
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
