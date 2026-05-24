use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Find files by glob pattern - inspired by codex/claude-code/opencode glob tool
pub struct GlobPlugin;

#[async_trait]
impl Plugin for GlobPlugin {
    fn name(&self) -> &str {
        "glob_files"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern"
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
                "name": "glob_files",
                "description": "Find files matching a glob pattern (e.g. **/*.rs, src/**/*.ts). Returns list of matching file paths.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob pattern (e.g. **/*.rs, src/*.py)" },
                        "path": { "type": "string", "description": "Base directory to search from (default: current directory)" }
                    },
                    "required": ["pattern"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let pattern = args["pattern"].as_str().unwrap_or("");
        let base_path = args["path"].as_str().unwrap_or(".");

        if pattern.is_empty() {
            return "Error: no pattern provided".to_string();
        }

        let full_pattern = if base_path == "." {
            pattern.to_string()
        } else {
            format!("{}/{}", base_path.trim_end_matches('/'), pattern)
        };

        match glob::glob(&full_pattern) {
            Ok(entries) => {
                let files: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .take(100)
                    .map(|p| p.display().to_string())
                    .collect();
                if files.is_empty() {
                    "No files found matching pattern".to_string()
                } else {
                    format!("Found {} files:\n{}", files.len(), files.join("\n"))
                }
            }
            Err(e) => format!("Invalid glob pattern: {}", e),
        }
    }
}
