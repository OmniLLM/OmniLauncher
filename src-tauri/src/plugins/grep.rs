use crate::plugins::{truncate_on_char_boundary, Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

/// Search file contents with regex - inspired by codex/claude-code/opencode grep tool
pub struct GrepPlugin;

#[async_trait]
impl Plugin for GrepPlugin {
    fn name(&self) -> &str {
        "grep_search"
    }

    fn description(&self) -> &str {
        "Search file contents using regex patterns"
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
                "name": "grep_search",
                "description": "Search for a pattern in files using regex. Returns matching lines with file paths and line numbers. Similar to ripgrep/grep.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Regex pattern to search for" },
                        "path": { "type": "string", "description": "Directory or file to search in (default: current directory)" },
                        "include": { "type": "string", "description": "File glob pattern to include (e.g. *.rs, *.py)" },
                        "case_insensitive": { "type": "boolean", "description": "Case insensitive search" }
                    },
                    "required": ["pattern"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let pattern = args["pattern"].as_str().unwrap_or("");
        let path = args["path"].as_str().unwrap_or(".");
        let include = args["include"].as_str();
        let case_insensitive = args["case_insensitive"].as_bool().unwrap_or(false);

        if pattern.is_empty() {
            return "Error: no pattern provided".to_string();
        }

        // Try ripgrep first, fall back to findstr/grep
        let mut cmd = if which_exists("rg") {
            let mut c = Command::new("rg");
            c.args(["--line-number", "--no-heading", "--max-count", "50"]);
            if case_insensitive {
                c.arg("-i");
            }
            if let Some(glob) = include {
                c.args(["--glob", glob]);
            }
            c.arg(pattern);
            c.arg(path);
            c
        } else if cfg!(target_os = "windows") {
            let mut c = Command::new("findstr");
            if case_insensitive {
                c.arg("/I");
            }
            c.args(["/S", "/N", "/R", pattern, &format!("{}\\*", path)]);
            c
        } else {
            let mut c = Command::new("grep");
            c.args(["-rn", "--max-count=50"]);
            if case_insensitive {
                c.arg("-i");
            }
            if let Some(glob) = include {
                c.args(["--include", glob]);
            }
            c.args([pattern, path]);
            c
        };

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if stdout.is_empty() {
                    "No matches found".to_string()
                } else if stdout.len() > 6000 {
                    format!(
                        "{}\n... (truncated)",
                        truncate_on_char_boundary(&stdout, 6000)
                    )
                } else {
                    stdout.to_string()
                }
            }
            Err(e) => format!("Error running search: {}", e),
        }
    }
}

fn which_exists(name: &str) -> bool {
    if cfg!(target_os = "windows") {
        Command::new("where")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    } else {
        Command::new("which")
            .arg(name)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}
