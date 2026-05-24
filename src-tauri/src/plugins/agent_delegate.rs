use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct AgentDelegatePlugin;

#[async_trait]
impl Plugin for AgentDelegatePlugin {
    fn name(&self) -> &str {
        "agent_delegate"
    }

    fn description(&self) -> &str {
        "Delegate tasks to AI coding agents (@claude, @codex, @omnicode, @opencode)"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();

        // Match @agent_name queries
        let agent = if let Some(prompt) = raw.strip_prefix("@claude ") {
            Some(("claude", prompt.trim()))
        } else if let Some(prompt) = raw.strip_prefix("@codex ") {
            Some(("codex", prompt.trim()))
        } else if let Some(prompt) = raw.strip_prefix("@omnicode ") {
            Some(("omnicode", prompt.trim()))
        } else if let Some(prompt) = raw.strip_prefix("@opencode ") {
            Some(("opencode", prompt.trim()))
        } else {
            None
        };

        if let Some((agent_name, prompt)) = agent {
            if prompt.is_empty() {
                return vec![];
            }

            let shell_cmd = format!(
                "{} -p \"{}\"",
                agent_name,
                prompt.replace('"', "\\\"")
            );

            return vec![QueryResult {
                id: format!("agent:{}:{}", agent_name, prompt),
                title: format!("@{}: {}", agent_name, prompt),
                subtitle: Some(format!("Delegate to {} agent", agent_name)),
                icon: Some("🤖".to_string()),
                score: 100,
                action_type: "shell".to_string(),
                action_data: shell_cmd,
            }];
        }

        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "delegate_to_agent",
                "description": "Delegate a task to an AI coding agent (claude, codex, omnicode, opencode)",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent_name": {
                            "type": "string",
                            "enum": ["claude", "codex", "omnicode", "opencode"],
                            "description": "Name of the agent to delegate to"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The task prompt to send to the agent"
                        }
                    },
                    "required": ["agent_name", "prompt"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let agent_name = args["agent_name"].as_str().unwrap_or("");
        let prompt = args["prompt"].as_str().unwrap_or("");

        if agent_name.is_empty() || prompt.is_empty() {
            return "Both agent_name and prompt are required".to_string();
        }

        // Run the command with a 60-second timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            tokio::process::Command::new(agent_name)
                .args(["-p", prompt])
                .output(),
        )
        .await
        {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if !stdout.is_empty() {
                    stdout.trim().to_string()
                } else if !stderr.is_empty() {
                    format!("Error: {}", stderr.trim())
                } else {
                    format!("Agent '{}' completed (no output)", agent_name)
                }
            }
            Ok(Err(e)) => format!("Failed to run agent '{}': {}", agent_name, e),
            Err(_) => format!("Agent '{}' timed out after 60 seconds", agent_name),
        }
    }
}