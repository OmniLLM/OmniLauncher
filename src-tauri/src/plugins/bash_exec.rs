use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

/// Execute shell commands - uses PowerShell on Windows, bash on Linux/macOS
pub struct ShellExecPlugin;

#[async_trait]
impl Plugin for ShellExecPlugin {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        if cfg!(target_os = "windows") {
            "Execute PowerShell commands and return output"
        } else {
            "Execute bash commands and return output"
        }
    }

    fn keyword(&self) -> Option<&str> {
        Some(">")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let cmd = q.raw.strip_prefix('>').unwrap_or("").trim();
        if cmd.is_empty() {
            return vec![QueryResult {
                id: "shell:help".to_string(),
                title: "Execute command".to_string(),
                subtitle: Some("Type a shell command to execute".to_string()),
                icon: Some("💻".to_string()),
                score: 50,
                action_type: "shell".to_string(),
                action_data: String::new(),
            }];
        }
        vec![QueryResult {
            id: format!("shell:{}", cmd),
            title: format!("Run: {}", cmd),
            subtitle: Some("Press Enter to execute".to_string()),
            icon: Some("💻".to_string()),
            score: 90,
            action_type: "shell".to_string(),
            action_data: cmd.to_string(),
        }]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        let description = if cfg!(target_os = "windows") {
            "Execute a PowerShell command and return its output. This system runs Windows with PowerShell. \
             Use PowerShell syntax: Get-ChildItem (not ls), Get-Process (not ps), Select-String (not grep), \
             Get-Content (not cat), Remove-Item (not rm). Use $env:VAR for environment variables. \
             Paths use backslash (C:\\Users\\...)."
        } else if cfg!(target_os = "macos") {
            "Execute a bash/zsh command and return its output. This system runs macOS. \
             Use standard Unix commands: ls, grep, cat, rm, find, etc. Use 'open' to open files/URLs."
        } else {
            "Execute a bash command and return its output. This system runs Linux. \
             Use standard Unix commands: ls, grep, cat, rm, find, etc. Use 'xdg-open' to open files/URLs."
        };

        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "shell_exec",
                "description": description,
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "The shell command to execute (use PowerShell syntax on Windows, bash on Linux/macOS)" },
                        "working_dir": { "type": "string", "description": "Optional working directory" }
                    },
                    "required": ["command"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let command = args["command"].as_str().unwrap_or("");
        let working_dir = args["working_dir"].as_str();

        if command.is_empty() {
            return "Error: no command provided".to_string();
        }

        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("powershell");
            c.args(["-NoProfile", "-Command", command]);
            c
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push_str("\n--- stderr ---\n");
                    }
                    result.push_str(&stderr);
                }
                if result.is_empty() {
                    format!(
                        "Command completed with exit code: {}",
                        output.status.code().unwrap_or(-1)
                    )
                } else {
                    // Truncate very long output
                    if result.len() > 4000 {
                        result.truncate(4000);
                        result.push_str("\n... (truncated)");
                    }
                    result
                }
            }
            Err(e) => format!("Error executing command: {}", e),
        }
    }
}
