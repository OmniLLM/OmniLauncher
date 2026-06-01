use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

/// Git operations tool - inspired by codex/opencode git integration
pub struct GitPlugin;

#[async_trait]
impl Plugin for GitPlugin {
    fn name(&self) -> &str {
        "git_ops"
    }

    fn description(&self) -> &str {
        "Git operations: status, log, diff, branch"
    }

    fn keyword(&self) -> Option<&str> {
        Some("git ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q.raw.strip_prefix("git ").unwrap_or("").trim();
        let commands = vec![
            ("status", "📊", "Show git status"),
            ("log", "📜", "Show recent commits"),
            ("branch", "🌿", "List branches"),
            ("diff", "📝", "Show unstaged changes"),
            ("stash", "📦", "Stash changes"),
        ];

        commands
            .into_iter()
            .filter(|(name, _, _)| term.is_empty() || name.contains(term))
            .map(|(name, icon, desc)| QueryResult {
                id: format!("git:{}", name),
                title: desc.to_string(),
                subtitle: Some(format!("git {}", name)),
                icon: Some(icon.to_string()),
                score: 70,
                action_type: "shell".to_string(),
                action_data: format!("git {}", name),
                source: None,
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "git_ops",
                "description": "Run git commands. Supports status, log, diff, branch, add, commit, and any other git subcommand.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "subcommand": { "type": "string", "description": "Git subcommand (e.g. status, log --oneline -10, diff, branch -a)" },
                        "working_dir": { "type": "string", "description": "Repository directory (default: current directory)" }
                    },
                    "required": ["subcommand"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let subcommand = args["subcommand"].as_str().unwrap_or("status");
        let working_dir = args["working_dir"].as_str();

        let parts: Vec<&str> = subcommand.split_whitespace().collect();
        if parts.is_empty() {
            return "Error: no git subcommand".to_string();
        }

        let mut cmd = Command::new("git");
        cmd.args(&parts);
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut result = stdout.to_string();
                if !stderr.is_empty() && !output.status.success() {
                    result = format!("{}\n{}", result, stderr);
                }
                if result.len() > 6000 {
                    format!("{}\n... (truncated)", &result[..6000])
                } else if result.is_empty() {
                    format!("git {} completed (no output)", subcommand)
                } else {
                    result
                }
            }
            Err(e) => format!("Error running git: {}", e),
        }
    }
}
