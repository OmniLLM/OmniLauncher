use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use serde_json::json;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

pub struct SkillRunnerPlugin;

const MAX_OUTPUT_BYTES: usize = 64_000;

fn is_safe_skill_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn python_executable() -> String {
    let bundled_rel = if cfg!(windows) {
        "python.exe"
    } else {
        "bin/python3"
    };

    dirs::home_dir()
        .map(|h| h.join(".omnilauncher").join("python").join(bundled_rel))
        .filter(|p| p.exists())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "python.exe".to_string()
            } else {
                "python3".to_string()
            }
        })
}

fn truncate_output(mut s: String) -> String {
    if s.len() > MAX_OUTPUT_BYTES {
        let boundary = crate::plugins::truncate_on_char_boundary(&s, MAX_OUTPUT_BYTES).len();
        s.truncate(boundary);
        s.push_str("\n... (truncated)");
    }
    s
}

fn skill_script_path(skill_name: &str) -> Result<(PathBuf, PathBuf), String> {
    if !is_safe_skill_name(skill_name) {
        return Err(
            "Invalid skill name. Use only letters, numbers, hyphens, and underscores.".to_string(),
        );
    }

    let skill_dir = crate::skills::SkillManager::skill_dir().join(skill_name);
    let script = skill_dir.join("run.py");
    if !script.is_file() {
        return Err(format!(
            "Skill '{}' does not have a run.py entrypoint at {}",
            skill_name,
            script.display()
        ));
    }
    Ok((skill_dir, script))
}

async fn run_python_skill(script: &Path, working_dir: &Path, request: serde_json::Value) -> String {
    let input = match serde_json::to_vec(&request) {
        Ok(v) => v,
        Err(e) => return format!("Error serializing skill request: {e}"),
    };

    let mut child = match Command::new(python_executable())
        .arg(script)
        .current_dir(working_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => return format!("Error starting skill script '{}': {e}", script.display()),
    };

    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(&input).await {
            let _ = child.kill().await;
            return format!("Error writing skill request to stdin: {e}");
        }
    }

    let output = match child.wait_with_output().await {
        Ok(output) => output,
        Err(e) => return format!("Error waiting for skill script: {e}"),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Most skill scripts return `{ "output": "...markdown...", "results": [...] }`.
    // Hand the model the already-rendered `output` field instead of the full JSON
    // blob: it avoids duplicating large result arrays and eliminates another
    // parse step where a model could misread valid results as empty.
    if stderr.is_empty() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&stdout) {
            if let Some(rendered) = value.get("output").and_then(|v| v.as_str()) {
                return truncate_output(rendered.to_string());
            }
        }
    }

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
        result = format!(
            "Skill script completed with exit code: {}",
            output.status.code().unwrap_or(-1)
        );
    }

    truncate_output(result)
}

#[async_trait]
impl Plugin for SkillRunnerPlugin {
    fn name(&self) -> &str {
        "skill_runner"
    }

    fn description(&self) -> &str {
        "Run installed skill scripts with structured JSON stdin (no shell quoting)"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    fn cheap_prefix_match(&self, _raw: &str) -> bool {
        false
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(json!({
            "type": "function",
            "function": {
                "name": "execute_skill",
                "description": "Execute an installed OmniLauncher skill's run.py entrypoint by serializing structured JSON in the app and piping it directly to Python stdin. Prefer this over shell_exec for skills that document stdin JSON, especially on Windows, because it avoids PowerShell quoting/escaping failures.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "skill": {
                            "type": "string",
                            "description": "Installed skill name, e.g. openstack"
                        },
                        "op": {
                            "type": "string",
                            "description": "Operation to pass to the skill runner, e.g. tool_call, query, or execute. Defaults to tool_call."
                        },
                        "args": {
                            "type": "object",
                            "description": "Structured args object for op=tool_call. Do not stringify this JSON; pass it as an object."
                        },
                        "query": {
                            "type": "string",
                            "description": "Query string for op=query."
                        },
                        "action_data": {
                            "type": "string",
                            "description": "Action data for op=execute."
                        }
                    },
                    "required": ["skill"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let skill = args.get("skill").and_then(|v| v.as_str()).unwrap_or("");
        let (skill_dir, script) = match skill_script_path(skill) {
            Ok(paths) => paths,
            Err(e) => return e,
        };

        let op = args
            .get("op")
            .and_then(|v| v.as_str())
            .unwrap_or("tool_call");

        let request = match op {
            "query" => json!({
                "op": "query",
                "query": args.get("query").and_then(|v| v.as_str()).unwrap_or("")
            }),
            "execute" => json!({
                "op": "execute",
                "action_data": args.get("action_data").and_then(|v| v.as_str()).unwrap_or("")
            }),
            _ => json!({
                "op": "tool_call",
                "args": args.get("args").cloned().unwrap_or_else(|| json!({}))
            }),
        };

        log::info!(
            "execute_skill: skill={} op={} script={}",
            skill,
            op,
            script.display()
        );
        run_python_skill(&script, &skill_dir, request).await
    }
}
