//! Flow.Launcher plugin adapter.
//!
//! Detection: a directory is considered a Flow.Launcher plugin when its
//! `plugin.json` contains the Flow-specific PascalCase fields
//! `ExecuteFileName` AND `Language` AND (`ID` or `Name`).
//!
//! On install we:
//!   1. Rename Flow's `plugin.json` → `flow.plugin.json` (since OmniLauncher
//!      uses the same filename for its own manifest).
//!   2. Write an OmniLauncher `plugin.json` that points at our JS shim.
//!   3. Copy the shim file (`flow-shim.cjs`) into the plugin directory.
//!   4. Best-effort: `pip install -r requirements.txt` for Python plugins,
//!      or `npm install && npm run build` for JS/TS plugins.
//!
//! At runtime, the shim translates between OmniLauncher's stdin/stdout
//! protocol and Flow's JSON-RPC-over-argv protocol.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

const SHIM_JS: &str = include_str!("../../assets/flow-shim/shim.cjs");
const SHIM_FILENAME: &str = "flow-shim.cjs";
const FLOW_MANIFEST_FILENAME: &str = "flow.plugin.json";

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct FlowManifest {
    #[serde(default)]
    ID: Option<String>,
    #[serde(default)]
    Name: Option<String>,
    #[serde(default)]
    Description: Option<String>,
    #[serde(default)]
    Version: Option<String>,
    #[serde(default)]
    Language: Option<String>,
    #[serde(default)]
    ActionKeyword: Option<String>,
    #[serde(default)]
    ActionKeywords: Option<Vec<String>>,
    #[serde(default)]
    ExecuteFileName: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    IcoPath: Option<String>,
}

#[derive(Debug, Serialize)]
struct SyntheticManifest<'a> {
    name: &'a str,
    description: &'a str,
    version: &'a str,
    keyword: &'a str,
    icon: &'a str,
    entry: &'a str,
    entry_windows: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_schema: Option<serde_json::Value>,
}

/// Build an OpenAI-style function tool schema so the AI agent can invoke
/// the Flow.Launcher plugin. Flow plugins are a single search surface, so
/// the tool has just a `query` parameter; the shim will run a Flow `query`
/// and auto-invoke the first result's action.
fn build_tool_schema(plugin_name: &str, manifest: &FlowManifest) -> serde_json::Value {
    let description = manifest
        .Description
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|d| format!("{} (Flow.Launcher plugin)", d))
        .unwrap_or_else(|| format!("{} (Flow.Launcher plugin)", plugin_name));

    serde_json::json!({
        "type": "function",
        "function": {
            "name": plugin_name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search text to send to the Flow.Launcher plugin."
                    }
                },
                "required": ["query"]
            }
        }
    })
}

/// Read a Flow.Launcher manifest from `dir` if it exists and looks Flow-shaped.
/// Tries `plugin.json` first; if that's already been replaced with our
/// OmniLauncher manifest, falls back to the saved `flow.plugin.json`.
fn read_flow_manifest(dir: &Path) -> Option<FlowManifest> {
    let try_read = |name: &str| -> Option<FlowManifest> {
        let raw = std::fs::read_to_string(dir.join(name)).ok()?;
        let trimmed = raw.trim_start_matches('\u{feff}');
        let m: FlowManifest = match serde_json::from_str(trimmed) {
            Ok(m) => m,
            Err(e) => {
                log::debug!(
                    "read_flow_manifest: failed to parse {} at {}: {e}",
                    name,
                    dir.display()
                );
                return None;
            }
        };
        if m.ExecuteFileName.as_deref().unwrap_or("").is_empty() {
            return None;
        }
        if m.Language.as_deref().unwrap_or("").is_empty() {
            return None;
        }
        Some(m)
    };
    try_read("plugin.json").or_else(|| try_read(FLOW_MANIFEST_FILENAME))
}

/// Returns true when `dir` looks like a Flow.Launcher plugin source repo
/// (either pre-install or already adapted by us).
pub fn is_flow_plugin(dir: &Path) -> bool {
    read_flow_manifest(dir).is_some()
}

fn primary_keyword(m: &FlowManifest) -> String {
    if let Some(kw) = &m.ActionKeyword {
        let k = kw.trim();
        if !k.is_empty() && k != "*" {
            return k.to_string();
        }
    }
    if let Some(list) = &m.ActionKeywords {
        if let Some(first) = list.iter().find(|s| !s.trim().is_empty() && s.trim() != "*") {
            return first.trim().to_string();
        }
    }
    // Fall back to lowercase plugin name slug so the user can still trigger it
    let name = m.Name.clone().unwrap_or_else(|| "flow".to_string());
    name.to_lowercase().replace(|c: char| !c.is_alphanumeric(), "-")
}

/// Synthesize OmniLauncher manifest + shim for a single Flow.Launcher
/// plugin directory. Idempotent — re-running overwrites the shim and the
/// flow.plugin.json copy, but keeps a user-authored OmniLauncher plugin.json
/// that doesn't reference our shim.
pub fn synthesize_plugin_files(dir: &Path) -> Result<String, String> {
    let manifest = read_flow_manifest(dir).ok_or_else(|| {
        format!(
            "{} is not a Flow.Launcher plugin (missing Language/ExecuteFileName).",
            dir.display()
        )
    })?;

    let language = manifest
        .Language
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(language.as_str(), "csharp" | "fsharp") {
        return Err(format!(
            "Flow.Launcher {} plugins (C#/F#) are not supported by OmniLauncher.",
            language
        ));
    }

    // Copy plugin.json → flow.plugin.json (preserve original for the shim).
    // On re-synthesis, plugin.json may already be the OmniLauncher manifest;
    // in that case the existing flow.plugin.json is already our source of
    // truth and we don't overwrite it.
    let original_path = dir.join("plugin.json");
    let original_is_flow = std::fs::read_to_string(&original_path)
        .map(|s| s.contains("ExecuteFileName"))
        .unwrap_or(false);
    if original_is_flow {
        let original = std::fs::read(&original_path)
            .map_err(|e| format!("Failed to read original plugin.json: {e}"))?;
        std::fs::write(dir.join(FLOW_MANIFEST_FILENAME), &original)
            .map_err(|e| format!("Failed to write {FLOW_MANIFEST_FILENAME}: {e}"))?;
    } else if !dir.join(FLOW_MANIFEST_FILENAME).is_file() {
        return Err(format!(
            "{} has no Flow manifest to adapt.",
            dir.display()
        ));
    }

    // Write the shim file.
    std::fs::write(dir.join(SHIM_FILENAME), SHIM_JS)
        .map_err(|e| format!("Failed to write {SHIM_FILENAME}: {e}"))?;

    // Decide whether to overwrite the OmniLauncher plugin.json:
    //   - If it currently contains Flow fields (ID/ExecuteFileName) it's
    //     the original Flow manifest — replace it.
    //   - If it contains our shim filename — re-synthesize.
    //   - Otherwise (user-authored OmniLauncher manifest) — leave alone.
    let plugin_json_path = dir.join("plugin.json");
    let should_write_manifest = match std::fs::read_to_string(&plugin_json_path) {
        Ok(s) => s.contains("ExecuteFileName") || s.contains(SHIM_FILENAME),
        Err(_) => true,
    };

    if should_write_manifest {
        let name = manifest
            .ID
            .clone()
            .or_else(|| manifest.Name.clone())
            .unwrap_or_else(|| {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("flow-plugin")
                    .to_string()
            });
        let description = manifest
            .Description
            .as_deref()
            .unwrap_or("Flow.Launcher plugin (OmniLauncher adapter)");
        let version = manifest.Version.as_deref().unwrap_or("0.0.0");
        let keyword = primary_keyword(&manifest);
        let icon = "🚀";

        let synthetic = SyntheticManifest {
            name: &name,
            description,
            version,
            keyword: &keyword,
            icon,
            entry: SHIM_FILENAME,
            entry_windows: SHIM_FILENAME,
            tool_schema: Some(build_tool_schema(&name, &manifest)),
        };
        let json = serde_json::to_string_pretty(&synthetic)
            .map_err(|e| format!("Failed to serialize plugin.json: {e}"))?;
        std::fs::write(&plugin_json_path, json)
            .map_err(|e| format!("Failed to write plugin.json: {e}"))?;
    }

    log::info!(
        "Synthesized Flow.Launcher adapter for '{}' ({} plugin) at {}",
        manifest.Name.as_deref().unwrap_or("?"),
        language,
        dir.display()
    );
    Ok(manifest
        .ID
        .or(manifest.Name)
        .unwrap_or_else(|| "flow-plugin".into()))
}

/// Walk one level deep under `repo_dir` and synthesize plugin files for
/// every Flow.Launcher plugin found. Top-level matches take priority.
pub fn synthesize_flow_plugins_in(repo_dir: &Path) -> Vec<String> {
    log::debug!(
        "synthesize_flow_plugins_in: scanning '{}'",
        repo_dir.display()
    );
    let mut synthesized = Vec::new();

    if is_flow_plugin(repo_dir) {
        match synthesize_plugin_files(repo_dir) {
            Ok(name) => synthesized.push(name),
            Err(e) => log::warn!(
                "Failed to synthesize Flow.Launcher adapter for {}: {e}",
                repo_dir.display()
            ),
        }
        return synthesized;
    }

    let Ok(entries) = std::fs::read_dir(repo_dir) else {
        return synthesized;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_flow_plugin(&path) {
            match synthesize_plugin_files(&path) {
                Ok(name) => synthesized.push(name),
                Err(e) => log::warn!(
                    "Failed to synthesize Flow.Launcher adapter for {}: {e}",
                    path.display()
                ),
            }
        }
    }
    synthesized
}

/// Best-effort setup of the plugin's runtime dependencies. Silent on
/// failure — runtime errors will show up as result subtitles.
pub fn try_setup_dependencies(dir: &Path) {
    let Some(manifest) = read_flow_manifest(dir) else {
        return;
    };
    let language = manifest
        .Language
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();

    match language.as_str() {
        "python" => {
            let req = dir.join("requirements.txt");
            if !req.is_file() {
                return;
            }
            let py = pick_python_executable();
            log::info!(
                "Running '{} -m pip install -r requirements.txt' for Flow plugin at {}",
                py,
                dir.display()
            );
            let out = Command::new(&py)
                .args(["-m", "pip", "install", "-r", "requirements.txt", "-t", "lib"])
                .current_dir(dir)
                .output();
            match out {
                Ok(o) if o.status.success() => {
                    log::info!("Installed Python deps for Flow plugin at {}", dir.display());
                }
                Ok(o) => log::warn!(
                    "pip install failed for {}: {}",
                    dir.display(),
                    String::from_utf8_lossy(&o.stderr).trim()
                ),
                Err(e) => log::warn!("pip install failed for {}: {e}", dir.display()),
            }
        }
        "javascript" | "typescript" => {
            if !dir.join("package.json").is_file() {
                return;
            }
            if which("npm").is_none() {
                log::info!(
                    "npm not found; skipping setup for Flow JS plugin at {}",
                    dir.display()
                );
                return;
            }
            log::info!("Running 'npm install' for Flow JS plugin at {}", dir.display());
            let _ = Command::new(npm_exe())
                .arg("install")
                .current_dir(dir)
                .output();
            if language == "typescript" {
                let _ = Command::new(npm_exe())
                    .args(["run", "build"])
                    .current_dir(dir)
                    .output();
            }
        }
        _ => {}
    }
}

fn pick_python_executable() -> String {
    if let Some(home) = dirs::home_dir() {
        let bundled = if cfg!(windows) {
            home.join(".omnilauncher").join("python").join("python.exe")
        } else {
            home.join(".omnilauncher").join("python").join("bin").join("python3")
        };
        if bundled.is_file() {
            return bundled.to_string_lossy().into_owned();
        }
    }
    if cfg!(windows) {
        "python.exe".into()
    } else {
        "python3".into()
    }
}

fn npm_exe() -> &'static str {
    if cfg!(windows) { "npm.cmd" } else { "npm" }
}

fn which(cmd: &str) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = dir.join(format!("{cmd}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ol-flow-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_flow_plugin() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{
                "ID": "abc123",
                "Name": "Hello",
                "ActionKeyword": "hi",
                "Language": "python",
                "ExecuteFileName": "main.py",
                "Version": "1.0.0"
            }"#,
        )
        .unwrap();
        assert!(is_flow_plugin(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_omnilauncher_manifest() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{"name":"x","description":"y","version":"1","entry":"r.sh"}"#,
        )
        .unwrap();
        assert!(!is_flow_plugin(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn synthesizes_manifest_and_preserves_flow_json() {
        let dir = tmpdir();
        let flow_json = r#"{
            "ID": "abc123",
            "Name": "Hello",
            "Description": "Hello plugin",
            "ActionKeyword": "hi",
            "Language": "python",
            "ExecuteFileName": "main.py",
            "Version": "2.5.0",
            "IcoPath": "Images/app.png"
        }"#;
        std::fs::write(dir.join("plugin.json"), flow_json).unwrap();
        std::fs::write(dir.join("main.py"), "# stub").unwrap();

        let id = synthesize_plugin_files(&dir).unwrap();
        assert_eq!(id, "abc123");

        // Original kept as flow.plugin.json
        let kept = std::fs::read_to_string(dir.join(FLOW_MANIFEST_FILENAME)).unwrap();
        assert!(kept.contains("ExecuteFileName"));
        assert!(kept.contains("main.py"));

        // OmniLauncher plugin.json points at our shim
        let new_manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(new_manifest["entry"], SHIM_FILENAME);
        assert_eq!(new_manifest["keyword"], "hi");
        assert_eq!(new_manifest["version"], "2.5.0");
        assert_eq!(new_manifest["name"], "abc123");

        // Shim file present
        assert!(dir.join(SHIM_FILENAME).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_csharp_plugin() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{
                "ID":"x","Name":"x","ActionKeyword":"*",
                "Language":"csharp","ExecuteFileName":"x.dll","Version":"1"
            }"#,
        )
        .unwrap();
        let err = synthesize_plugin_files(&dir).unwrap_err();
        assert!(err.contains("not supported"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn idempotent_resynthesis() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{
                "ID":"y","Name":"Y","ActionKeyword":"y",
                "Language":"python","ExecuteFileName":"main.py","Version":"1"
            }"#,
        )
        .unwrap();
        synthesize_plugin_files(&dir).unwrap();
        // Run again — should not error and the OmniLauncher manifest should
        // still point at the shim.
        synthesize_plugin_files(&dir).unwrap();
        let m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(m["entry"], SHIM_FILENAME);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn keyword_falls_back_when_star() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("plugin.json"),
            r#"{
                "ID":"z","Name":"My Cool Plugin","ActionKeyword":"*",
                "Language":"python","ExecuteFileName":"main.py","Version":"1"
            }"#,
        )
        .unwrap();
        synthesize_plugin_files(&dir).unwrap();
        let m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(m["keyword"], "my-cool-plugin");
        std::fs::remove_dir_all(&dir).ok();
    }
}
