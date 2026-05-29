use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Instant;

pub struct AgentDelegatePlugin;

#[derive(Debug, Deserialize)]
struct SubTask {
    agent_name: String,
    prompt: String,
    context: Option<String>,
}

const DEFAULT_TIMEOUT_SECS: u64 = 300;
const MAX_TIMEOUT_SECS: u64 = 600;
const ALLOWED_AGENTS: &[&str] = &["claude", "codex", "omnicode", "opencode"];

fn is_allowed_agent(name: &str) -> bool {
    ALLOWED_AGENTS.iter().any(|a| *a == name)
}

fn build_prompt(prompt: &str, context: Option<&str>) -> String {
    match context {
        Some(ctx) if !ctx.is_empty() => format!(
            "[Context from parent agent]\n{}\n[End context]\n\nTask: {}",
            ctx, prompt
        ),
        _ => prompt.to_string(),
    }
}

/// Resolve the binary path for an AI agent, handling platform differences.
/// - Windows: npm-installed CLIs live as `.cmd` wrappers in `%APPDATA%\npm\`
/// - Linux: common install prefix `~/.npm-global/bin/`
/// - macOS: common Homebrew / npm prefix `~/.npm-global/bin/` or `/usr/local/bin/`
///   Falls back to bare name if the known path doesn't exist (PATH lookup).
fn resolve_agent_bin(agent_name: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        // npm global .cmd wrapper path on Windows
        if let Ok(appdata) = std::env::var("APPDATA") {
            let candidate = format!("{}\\npm\\{}.cmd", appdata, agent_name);
            if std::path::Path::new(&candidate).exists() {
                return candidate;
            }
        }
        // opencode may ship as a standalone exe in LOCALAPPDATA
        if agent_name == "opencode" {
            if let Ok(local) = std::env::var("LOCALAPPDATA") {
                let candidate = format!("{}\\opencode\\opencode.exe", local);
                if std::path::Path::new(&candidate).exists() {
                    return candidate;
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        // opencode is not in PATH by default on Linux/macOS
        if agent_name == "opencode" {
            let known = "/data/tools/opencode/packages/opencode/bin/opencode";
            if std::path::Path::new(known).exists() {
                return known.to_string();
            }
        }
        // npm-global bin (Linux / macOS)
        if let Ok(home) = std::env::var("HOME") {
            let candidate = format!("{}/.npm-global/bin/{}", home, agent_name);
            if std::path::Path::new(&candidate).exists() {
                return candidate;
            }
        }
    }
    // Fallback: rely on PATH
    agent_name.to_string()
}

// Helper to run agent and return (elapsed_secs, output)
async fn run_agent_timed(agent_name: String, prompt: String, timeout_secs: u64) -> (u64, String) {
    let start = Instant::now();
    // On Windows, claude/codex/opencode are installed as .cmd wrappers via npm.
    // Look them up by known Windows paths; fall back to bare name for PATH resolution.
    let resolved_bin = resolve_agent_bin(&agent_name);
    match tokio::time::timeout(
        std::time::Duration::from_secs(timeout_secs),
        tokio::process::Command::new(&resolved_bin)
            .args(["-p", &prompt])
            .output(),
    )
    .await
    {
        Ok(Ok(output)) => {
            let elapsed = start.elapsed().as_secs();
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let body = if !stdout.is_empty() {
                stdout.trim().to_string()
            } else if !stderr.is_empty() {
                format!("Error: {}", stderr.trim())
            } else {
                format!("Agent '{}' completed (no output)", agent_name)
            };
            (elapsed, body)
        }
        Ok(Err(e)) => (0, format!("Failed to run agent '{}': {}", agent_name, e)),
        Err(_) => (
            timeout_secs,
            format!("Agent '{}' timed out after {}s", agent_name, timeout_secs),
        ),
    }
}

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
        } else if raw.strip_prefix("@opencode ").is_some() {
            raw.strip_prefix("@opencode ")
                .map(|prompt| ("opencode", prompt.trim()))
        } else {
            None
        };

        if let Some((agent_name, prompt)) = agent {
            if prompt.is_empty() {
                return vec![];
            }

            let resolved = resolve_agent_bin(agent_name);
            let shell_cmd = if cfg!(target_os = "windows") {
                format!("\"{}\" -p \"{}\"", resolved, prompt.replace('"', "\\\""))
            } else {
                format!("{} -p \"{}\"", resolved, prompt.replace('"', "\\\""))
            };

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
                "description": "Delegate a task to an AI coding agent (claude, codex, omnicode, opencode). Supports single-agent and parallel multi-agent delegation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "agent_name": {
                            "type": "string",
                            "enum": ["claude", "codex", "omnicode", "opencode"],
                            "description": "Name of the agent to delegate to (for single-agent mode)"
                        },
                        "prompt": {
                            "type": "string",
                            "description": "The task prompt to send to the agent (for single-agent mode)"
                        },
                        "context": {
                            "type": "string",
                            "description": "Background context to prepend to the prompt for the sub-agent"
                        },
                        "timeout_secs": {
                            "type": "integer",
                            "description": "Timeout in seconds (default: 300, max: 600)"
                        },
                        "tasks": {
                            "type": "array",
                            "description": "Run multiple agents in parallel. Each item: {agent_name, prompt, context}",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "agent_name": {
                                        "type": "string",
                                        "enum": ["claude", "codex", "opencode"]
                                    },
                                    "prompt": {"type": "string"},
                                    "context": {
                                        "type": "string",
                                        "description": "Background context for this subtask"
                                    }
                                },
                                "required": ["agent_name", "prompt"]
                            }
                        }
                    }
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let timeout_secs = args["timeout_secs"]
            .as_u64()
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS);

        // Parallel tasks mode
        if let Some(tasks_val) = args.get("tasks") {
            if let Ok(tasks) = serde_json::from_value::<Vec<SubTask>>(tasks_val.clone()) {
                if !tasks.is_empty() {
                    if let Some(bad) = tasks.iter().find(|t| !is_allowed_agent(&t.agent_name)) {
                        return format!(
                            "Error: agent_name '{}' is not in the allowed list {:?}",
                            bad.agent_name, ALLOWED_AGENTS
                        );
                    }
                    return self.run_parallel_tasks(tasks, timeout_secs).await;
                }
            }
        }

        // Single-agent mode (backward compatible)
        let agent_name = args["agent_name"].as_str().unwrap_or("").to_string();
        let prompt_raw = args["prompt"].as_str().unwrap_or("").to_string();
        let context = args["context"].as_str().map(|s| s.to_string());

        if agent_name.is_empty() || prompt_raw.is_empty() {
            return "Both agent_name and prompt are required (or provide a tasks array)"
                .to_string();
        }

        if !is_allowed_agent(&agent_name) {
            return format!(
                "Error: agent_name '{}' is not in the allowed list {:?}",
                agent_name, ALLOWED_AGENTS
            );
        }

        let prompt = build_prompt(&prompt_raw, context.as_deref());
        let (elapsed, output) = run_agent_timed(agent_name.clone(), prompt, timeout_secs).await;

        format!(
            "## Delegation Results\n\n### @{} (completed in {}s)\n{}\n\n---\nAll 1 task completed.",
            agent_name, elapsed, output
        )
    }
}

impl AgentDelegatePlugin {
    async fn run_parallel_tasks(&self, tasks: Vec<SubTask>, timeout_secs: u64) -> String {
        let total = tasks.len();

        // Spawn all tasks concurrently
        let handles: Vec<_> = tasks
            .into_iter()
            .map(|task| {
                let agent = task.agent_name.clone();
                let prompt = build_prompt(&task.prompt, task.context.as_deref());
                tokio::spawn(async move {
                    let (elapsed, output) =
                        run_agent_timed(agent.clone(), prompt, timeout_secs).await;
                    (agent, elapsed, output)
                })
            })
            .collect();

        // Wait for all
        let mut results = Vec::with_capacity(total);
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(("unknown".to_string(), 0, format!("Task panicked: {}", e))),
            }
        }

        // Build structured summary
        let mut summary = String::from("## Delegation Results\n\n");
        for (i, (agent, elapsed, output)) in results.iter().enumerate() {
            summary.push_str(&format!(
                "### Task {} (@{}) (completed in {}s)\n{}\n\n",
                i + 1,
                agent,
                elapsed,
                output
            ));
        }
        summary.push_str("---\n");
        summary.push_str(&format!("All {} tasks completed.", total));
        summary
    }
}
