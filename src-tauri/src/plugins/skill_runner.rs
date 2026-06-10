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

fn skill_dir_path(skill_name: &str) -> Result<PathBuf, String> {
    if !is_safe_skill_name(skill_name) {
        return Err(
            "Invalid skill name. Use only letters, numbers, hyphens, and underscores.".to_string(),
        );
    }

    Ok(crate::skills::SkillManager::skill_dir().join(skill_name))
}

fn skill_script_path(skill_name: &str) -> Result<(PathBuf, PathBuf), String> {
    let skill_dir = skill_dir_path(skill_name)?;
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

fn format_python_output(output: std::process::Output) -> String {
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

async fn run_python_script(
    skill: &str,
    script: &Path,
    working_dir: &Path,
    args: &[String],
) -> String {
    let mut command = Command::new(python_executable());
    command
        .arg(script)
        .args(args)
        .current_dir(working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let credential_env = crate::plugins::skill_credentials::credential_env_for_skill(skill);
    for (key, value) in credential_env {
        command.env(key, value);
    }

    let output = match command.output().await {
        Ok(output) => output,
        Err(e) => return format!("Error starting skill script '{}': {e}", script.display()),
    };

    format_python_output(output)
}

async fn run_python_skill(
    skill: &str,
    script: &Path,
    working_dir: &Path,
    request: serde_json::Value,
) -> String {
    let input = match serde_json::to_vec(&request) {
        Ok(v) => v,
        Err(e) => return format!("Error serializing skill request: {e}"),
    };

    let mut command = Command::new(python_executable());
    command
        .arg(script)
        .current_dir(working_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let credential_env = crate::plugins::skill_credentials::credential_env_for_skill(skill);
    for (key, value) in credential_env {
        command.env(key, value);
    }

    let mut child = match command.spawn() {
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

    format_python_output(output)
}

fn resolve_skill_script(skill_dir: &Path, script: &str) -> Result<PathBuf, String> {
    let relative = Path::new(script);
    if relative.is_absolute() {
        return Err("Error: skill script path must be relative".to_string());
    }
    if relative
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Error: skill script path cannot contain '..'".to_string());
    }

    let path = skill_dir.join(relative);
    if !path.is_file() {
        return Err(format!("Skill script not found: {}", path.display()));
    }
    Ok(path)
}

fn parse_execute_script_action(
    skill_dir: &Path,
    action_data: &str,
) -> Result<Option<(PathBuf, Vec<String>)>, String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(action_data) else {
        return Ok(None);
    };
    let Some(script) = value.get("script").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let args = value
        .get("args")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| "Error: skill script args must be strings".to_string())
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(Some((resolve_skill_script(skill_dir, script)?, args)))
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
        let op = args
            .get("op")
            .and_then(|v| v.as_str())
            .unwrap_or("tool_call");

        if op == "execute" {
            let skill_dir = match skill_dir_path(skill) {
                Ok(path) => path,
                Err(e) => return e,
            };
            let action_data = args
                .get("action_data")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match parse_execute_script_action(&skill_dir, action_data) {
                Ok(Some((script, script_args))) => {
                    log::info!(
                        "execute_skill: skill={} op=execute script={}",
                        skill,
                        script.display()
                    );
                    return run_python_script(skill, &script, &skill_dir, &script_args).await;
                }
                Ok(None) => {}
                Err(e) => return e,
            }
        }

        let (skill_dir, script) = match skill_script_path(skill) {
            Ok(paths) => paths,
            Err(e) => return e,
        };

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
        run_python_skill(skill, &script, &skill_dir, request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::skill_credentials::{SKILLS_CONFIG_DIR_ENV, TEST_ENV_LOCK};
    use crate::plugins::Plugin;
    use std::fs;
    use tempfile::TempDir;

    fn write(path: &Path, body: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    fn install_skill_script(root: &TempDir, skill: &str, rel_script: &str, body: &str) {
        let skill_dir = root.path().join("skills").join(skill);
        write(
            &skill_dir.join("SKILL.md"),
            "---\nname: gcp\ndescription: test\n---\nbody",
        );
        write(&skill_dir.join(rel_script), body);
    }

    #[tokio::test]
    async fn execute_op_runs_script_from_action_data_with_args() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = TempDir::new().unwrap();
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", root.path());
        std::env::remove_var(SKILLS_CONFIG_DIR_ENV);

        install_skill_script(
            &root,
            "gcp",
            "scripts/core/cai_runner.py",
            r#"
import json
import sys
print(json.dumps(sys.argv[1:]))
"#,
        );

        let action_data = json!({
            "script": "scripts/core/cai_runner.py",
            "args": [
                "--scope",
                "organizations/563352322117",
                "--query",
                "SELECT COUNT(*) AS count FROM cloudresourcemanager_googleapis_com_Project"
            ]
        })
        .to_string();

        let output = SkillRunnerPlugin
            .execute_tool(json!({
                "skill": "gcp",
                "op": "execute",
                "action_data": action_data
            }))
            .await;

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");

        assert!(
            output.contains("organizations/563352322117"),
            "expected script argv in output, got: {output}"
        );
        assert!(
            output.contains("cloudresourcemanager_googleapis_com_Project"),
            "expected query arg in output, got: {output}"
        );
    }

    #[tokio::test]
    async fn execute_op_injects_gcp_adc_env_into_script() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = TempDir::new().unwrap();
        let skills_config = TempDir::new().unwrap();
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", root.path());
        std::env::set_var(SKILLS_CONFIG_DIR_ENV, skills_config.path());

        install_skill_script(
            &root,
            "gcp",
            "scripts/core/env_probe.py",
            r#"
import os
print(os.environ.get("GOOGLE_APPLICATION_CREDENTIALS", ""))
"#,
        );
        let profile = json!({
            "cloud": {
                "gcp": {
                    "gcp": {
                        "service_account_key": {
                            "type": "service_account",
                            "project_id": "example-project",
                            "private_key_id": "dummy",
                            "private_key": "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n",
                            "client_email": "example@example-project.iam.gserviceaccount.com",
                            "client_id": "1234567890"
                        }
                    }
                }
            }
        });
        write(
            &skills_config.path().join("credential.json"),
            &profile.to_string(),
        );

        let action_data = json!({
            "script": "scripts/core/env_probe.py",
            "args": []
        })
        .to_string();

        let output = SkillRunnerPlugin
            .execute_tool(json!({
                "skill": "gcp",
                "op": "execute",
                "action_data": action_data
            }))
            .await;

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
        std::env::remove_var(SKILLS_CONFIG_DIR_ENV);

        let adc = skills_config.path().join("gcp_sa_key.json");
        assert!(
            output.contains(&adc.to_string_lossy().into_owned()),
            "expected ADC path in script env, got: {output}"
        );
    }

    #[tokio::test]
    async fn run_py_receives_gcp_adc_env() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = TempDir::new().unwrap();
        let skills_config = TempDir::new().unwrap();
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", root.path());
        std::env::set_var(SKILLS_CONFIG_DIR_ENV, skills_config.path());

        install_skill_script(
            &root,
            "gcp",
            "run.py",
            r#"
import json
import os
import sys
json.loads(sys.stdin.read())
print(os.environ.get("GOOGLE_APPLICATION_CREDENTIALS", ""))
"#,
        );
        let profile = json!({
            "cloud": {
                "gcp": {
                    "gcp": {
                        "service_account_key": {
                            "type": "service_account",
                            "project_id": "example-project",
                            "private_key_id": "dummy",
                            "private_key": "-----BEGIN PRIVATE KEY-----\nMIIB\n-----END PRIVATE KEY-----\n",
                            "client_email": "example@example-project.iam.gserviceaccount.com",
                            "client_id": "1234567890"
                        }
                    }
                }
            }
        });
        write(
            &skills_config.path().join("credential.json"),
            &profile.to_string(),
        );

        let output = SkillRunnerPlugin
            .execute_tool(json!({
                "skill": "gcp",
                "op": "tool_call",
                "args": {}
            }))
            .await;

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
        std::env::remove_var(SKILLS_CONFIG_DIR_ENV);

        let adc = skills_config.path().join("gcp_sa_key.json");
        assert!(
            output.contains(&adc.to_string_lossy().into_owned()),
            "expected ADC path in run.py env, got: {output}"
        );
    }

    #[tokio::test]
    async fn tool_call_still_runs_skill_run_py_with_structured_stdin() {
        let _guard = TEST_ENV_LOCK.lock().await;
        let root = TempDir::new().unwrap();
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", root.path());
        std::env::remove_var(SKILLS_CONFIG_DIR_ENV);

        install_skill_script(
            &root,
            "demo",
            "run.py",
            r#"
import json
import sys
payload = json.loads(sys.stdin.read())
print(json.dumps({"output": payload["args"]["message"]}))
"#,
        );

        let output = SkillRunnerPlugin
            .execute_tool(json!({
                "skill": "demo",
                "op": "tool_call",
                "args": {"message": "run.py still works"}
            }))
            .await;

        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");

        assert_eq!(output, "run.py still works");
    }
}
