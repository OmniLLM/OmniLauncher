/// Script Runner Plugin
///
/// Inspired by Sol (MIT) — watches a user scripts folder and surfaces any
/// shell scripts inside it as first-class launcher commands.
///
/// Drop a `.sh` script into ~/.omnilauncher/scripts/ and it immediately
/// appears in the launcher.  Metadata is read from leading comments:
///
///   #!/usr/bin/env bash
///   # name: My Script
///   # icon: 🚀
///   # desc: Does something useful
///
/// Usage:  type "scripts" or just "sc " to filter
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use crate::path_config;
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct ScriptRunnerPlugin;

struct ScriptMeta {
    name: String,
    icon: String,
    desc: String,
    path: PathBuf,
}

fn scripts_dir() -> PathBuf {
    path_config::data_dir().join("scripts")
}

fn parse_meta(path: &PathBuf) -> Option<ScriptMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("script")
        .to_string();

    let mut name = stem.clone();
    let mut icon = "📜".to_string();
    let mut desc = String::new();

    for line in content.lines().take(10) {
        let line = line.trim();
        if !line.starts_with('#') {
            break;
        }
        let comment = line.trim_start_matches('#').trim();
        if let Some(v) = comment.strip_prefix("name:") {
            name = v.trim().to_string();
        } else if let Some(v) = comment.strip_prefix("icon:") {
            icon = v.trim().to_string();
        } else if let Some(v) = comment.strip_prefix("desc:") {
            desc = v.trim().to_string();
        }
    }

    Some(ScriptMeta {
        name,
        icon,
        desc,
        path: path.clone(),
    })
}

fn load_scripts() -> Vec<ScriptMeta> {
    let dir = scripts_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
        return vec![];
    }

    WalkDir::new(&dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|ext| ext == "sh" || ext == "bash")
                    .unwrap_or(false)
        })
        .filter_map(|e| parse_meta(&e.path().to_path_buf()))
        .collect()
}

#[async_trait]
impl Plugin for ScriptRunnerPlugin {
    fn name(&self) -> &str {
        "script_runner"
    }

    fn description(&self) -> &str {
        "Run scripts from ~/.omnilauncher/scripts/ (type 'sc ' to filter)"
    }

    fn keyword(&self) -> Option<&str> {
        None // participates in global search
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim().to_lowercase();

        // Only activate on "sc " prefix or if query matches script names
        let filter = if let Some(f) = raw.strip_prefix("sc ") {
            f.trim().to_string()
        } else if raw == "sc" || raw == "scripts" {
            String::new()
        } else if raw.len() < 2 {
            return vec![];
        } else {
            // global search: only show if there's a matching script
            raw.clone()
        };

        let scripts = load_scripts();
        if scripts.is_empty() {
            if raw == "sc" || raw == "scripts" {
                return vec![QueryResult {
                    id: "script_runner:empty".to_string(),
                    title: "No scripts yet".to_string(),
                    subtitle: Some(format!("Add .sh files to {}", scripts_dir().display())),
                    icon: Some("📂".to_string()),
                    score: 50,
                    action_type: "shell".to_string(),
                    action_data: format!("mkdir -p {}", scripts_dir().display()),
                }];
            }
            return vec![];
        }

        scripts
            .into_iter()
            .filter(|s| {
                filter.is_empty()
                    || s.name.to_lowercase().contains(&filter)
                    || s.path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_lowercase().contains(&filter))
                        .unwrap_or(false)
            })
            .map(|s| {
                let score = if filter.is_empty() {
                    60
                } else if s.name.to_lowercase().starts_with(&filter) {
                    95
                } else {
                    75
                };
                QueryResult {
                    id: format!("script:{}", s.path.display()),
                    title: format!("{} {}", s.icon, s.name),
                    subtitle: if s.desc.is_empty() {
                        Some(s.path.display().to_string())
                    } else {
                        Some(s.desc)
                    },
                    icon: None,
                    score,
                    action_type: "shell".to_string(),
                    action_data: format!("bash {}", s.path.display()),
                }
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "run_user_script",
                "description": "Run a user-defined shell script from the scripts folder",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "script_name": {
                            "type": "string",
                            "description": "Script name or filename (without .sh)"
                        }
                    },
                    "required": ["script_name"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let name = args["script_name"].as_str().unwrap_or("").to_lowercase();
        let scripts = load_scripts();
        let found = scripts.iter().find(|s| {
            s.name.to_lowercase().contains(&name)
                || s.path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .map(|n| n.to_lowercase().contains(&name))
                    .unwrap_or(false)
        });

        match found {
            Some(s) => match std::process::Command::new("bash").arg(&s.path).output() {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                    if out.status.success() {
                        if stdout.is_empty() {
                            format!("Script '{}' ran successfully", s.name)
                        } else {
                            stdout
                        }
                    } else {
                        format!("Script failed: {}", stderr)
                    }
                }
                Err(e) => format!("Failed to run script: {}", e),
            },
            None => format!("No script found matching '{}'", name),
        }
    }
}
