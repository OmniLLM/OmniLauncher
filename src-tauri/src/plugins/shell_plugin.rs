use crate::guardrails::{GuardrailAction, Guardrails};
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

pub struct ShellPlugin;

#[async_trait]
impl Plugin for ShellPlugin {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run shell commands with > prefix"
    }

    fn keyword(&self) -> Option<&str> {
        Some(">")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let cmd = q.raw.strip_prefix('>').unwrap_or("").trim();
        if cmd.is_empty() {
            return vec![];
        }

        vec![QueryResult {
            id: format!("shell:{}", cmd),
            title: format!("Run: {}", cmd),
            subtitle: Some("Press Enter to execute in shell".to_string()),
            icon: Some("💻".to_string()),
            score: 95,
            action_type: "shell".to_string(),
            action_data: cmd.to_string(),
        }]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "shell",
                "description": "Run a shell command and return its output (> prefix plugin)",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to execute" }
                    },
                    "required": ["command"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let cmd = args["command"].as_str().unwrap_or("");
        if cmd.is_empty() {
            return "No command specified".to_string();
        }
        // Guardrails check
        match Guardrails::check_shell_command(cmd) {
            GuardrailAction::Deny(reason) => {
                return format!("Blocked by guardrails: {}", reason);
            }
            GuardrailAction::Warn(reason) => {
                eprintln!("[guardrails] WARNING: {} — command: {}", reason, cmd);
            }
            GuardrailAction::Allow => {}
        }
        run_shell(cmd)
    }
}

pub fn run_shell(cmd: &str) -> String {
    #[cfg(target_os = "windows")]
    let output = Command::new("cmd").args(["/C", cmd]).output();
    #[cfg(not(target_os = "windows"))]
    let output = Command::new("sh").args(["-c", cmd]).output();

    match output {
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            if !stdout.is_empty() {
                stdout
            } else if !stderr.is_empty() {
                stderr
            } else {
                format!("Exit code: {}", o.status.code().unwrap_or(-1))
            }
        }
        Err(e) => format!("Error: {}", e),
    }
}
