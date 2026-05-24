use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

/// List directory contents - inspired by codex/opencode ls tool
pub struct LsPlugin;

#[async_trait]
impl Plugin for LsPlugin {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List directory contents"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List files and directories in a given path. Returns names with type indicators (/ for dirs).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path to list (default: current directory)" },
                        "recursive": { "type": "boolean", "description": "List recursively (default: false)" }
                    },
                    "required": []
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let path = args["path"].as_str().unwrap_or(".");
        let recursive = args["recursive"].as_bool().unwrap_or(false);

        let dir = std::path::Path::new(path);
        if !dir.exists() {
            return format!("Error: path '{}' does not exist", path);
        }
        if !dir.is_dir() {
            return format!("Error: '{}' is not a directory", path);
        }

        if recursive {
            // Use system command for recursive listing
            let output = if cfg!(target_os = "windows") {
                Command::new("powershell")
                    .args(["-NoProfile", "-Command", &format!(
                        "Get-ChildItem -Path '{}' -Recurse -Name | Select-Object -First 200", path
                    )])
                    .output()
            } else {
                Command::new("find")
                    .args([path, "-maxdepth", "3", "-type", "f"])
                    .output()
            };
            match output {
                Ok(o) => {
                    let text = String::from_utf8_lossy(&o.stdout);
                    if text.len() > 6000 {
                        format!("{}\n... (truncated)", &text[..6000])
                    } else {
                        text.to_string()
                    }
                }
                Err(e) => format!("Error: {}", e),
            }
        } else {
            match std::fs::read_dir(path) {
                Ok(entries) => {
                    let mut items: Vec<String> = entries
                        .filter_map(|e| e.ok())
                        .take(100)
                        .map(|e| {
                            let name = e.file_name().to_string_lossy().to_string();
                            if e.path().is_dir() {
                                format!("{}/", name)
                            } else {
                                name
                            }
                        })
                        .collect();
                    items.sort();
                    if items.is_empty() {
                        "Empty directory".to_string()
                    } else {
                        items.join("\n")
                    }
                }
                Err(e) => format!("Error reading directory: {}", e),
            }
        }
    }
}
