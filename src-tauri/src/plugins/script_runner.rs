use crate::path_config;
/// Script Runner Plugin
///
/// Inspired by Sol (MIT) — watches a user scripts folder and surfaces any
/// shell scripts inside it as first-class launcher commands.
///
/// Drop a `.sh` script into ~/.omnilauncher/scripts/ and it immediately
/// appears in the launcher.  Metadata is read from leading comments:
///
///   #!/usr/bin/env bash
///   # name: My Script
///   # icon: 🚀
///   # desc: Does something useful
///
/// Usage:  type "scripts" or just "sc " to filter
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct ScriptRunnerPlugin;

struct ScriptMeta {
    name: String,
    icon: String,
    desc: String,
    path: PathBuf,
}

fn scripts_dir() -> PathBuf {
    path_config::data_dir().join("scripts")
}

/// Build the argv (program + arguments) to run `path` with the right interpreter.
/// Returning a structured vec instead of a single shell string keeps paths
/// containing spaces intact — splitting a shell string on whitespace would
/// destroy them.
/// - .ps1          → powershell -NoProfile -File <path>  (Windows)
///   pwsh -NoProfile -File <path>        (Linux/macOS — PowerShell Core)
/// - .bat / .cmd   → cmd /C <path>                       (Windows only; skipped on others)
/// - .py           → python3 <path>  (Linux/macOS) | python <path> (Windows)
/// - .sh / .bash   → bash <path>
fn shell_run_cmd(path: &std::path::Path) -> Vec<String> {
    let p = path.display().to_string();
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "ps1" => {
            let prog = if cfg!(target_os = "windows") {
                "powershell"
            } else {
                "pwsh"
            };
            vec![
                prog.to_string(),
                "-NoProfile".to_string(),
                "-File".to_string(),
                p,
            ]
        }
        "bat" | "cmd" => {
            if cfg!(target_os = "windows") {
                vec!["cmd".to_string(), "/C".to_string(), p]
            } else {
                // batch files are Windows-only; best effort
                vec![
                    "echo".to_string(),
                    format!("batch script not supported on this platform: {}", p),
                ]
            }
        }
        "py" => {
            let prog = if cfg!(target_os = "windows") {
                "python"
            } else {
                "python3"
            };
            vec![prog.to_string(), p]
        }
        _ => vec!["bash".to_string(), p], // .sh / .bash
    }
}

/// Render an argv as a shell command string with each argument quoted, for use
/// as a "shell" action that is later passed to a shell. Keeps spaces in paths
/// from being misinterpreted as argument boundaries.
fn argv_to_shell_string(argv: &[String]) -> String {
    argv.iter()
        .map(|arg| {
            if arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.' | ':' | '='))
            {
                arg.clone()
            } else {
                // Single-quote and escape embedded single quotes.
                format!("'{}'", arg.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_meta(path: &PathBuf) -> Option<ScriptMeta> {
    let content = std::fs::read_to_string(path).ok()?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("script")
        .to_string();

    let mut name = stem.clone();
    let mut icon = "📜".to_string();
    let mut desc = String::new();

    for line in content.lines().take(10) {
        let line = line.trim();
        // Allow # comments (sh/bash/py) and REM / :: comments (bat/cmd)
        let is_comment = line.starts_with('#')
            || line.to_uppercase().starts_with("REM ")
            || line.starts_with("::");
        if !is_comment {
            break;
        }
        let comment = if line.starts_with('#') {
            line.trim_start_matches('#').trim()
        } else if line.to_uppercase().starts_with("REM ") {
            line[4..].trim()
        } else {
            line.trim_start_matches(':').trim()
        };
        if let Some(v) = comment.strip_prefix("name:") {
            name = v.trim().to_string();
        } else if let Some(v) = comment.strip_prefix("icon:") {
            icon = v.trim().to_string();
        } else if let Some(v) = comment.strip_prefix("desc:") {
            desc = v.trim().to_string();
        }
    }

    Some(ScriptMeta {
        name,
        icon,
        desc,
        path: path.clone(),
    })
}

fn load_scripts() -> Vec<ScriptMeta> {
    let dir = scripts_dir();
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
        return vec![];
    }

    WalkDir::new(&dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|ext| {
                        ext == "sh"
                            || ext == "bash"
                            || ext == "ps1"
                            || ext == "bat"
                            || ext == "cmd"
                            || ext == "py"
                    })
                    .unwrap_or(false)
        })
        .filter_map(|e| parse_meta(&e.path().to_path_buf()))
        .collect()
}

#[async_trait]
impl Plugin for ScriptRunnerPlugin {
    fn name(&self) -> &str {
        "script_runner"
    }

    fn description(&self) -> &str {
        "Run scripts from ~/.omnilauncher/scripts/ (type 'sc ' to filter)"
    }

    fn keyword(&self) -> Option<&str> {
        None // participates in global search
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim().to_lowercase();

        // Only activate on "sc " prefix or if query matches script names
        let filter = if let Some(f) = raw.strip_prefix("sc ") {
            f.trim().to_string()
        } else if raw == "sc" || raw == "scripts" {
            String::new()
        } else if raw.len() < 2 {
            return vec![];
        } else {
            // global search: only show if there's a matching script
            raw.clone()
        };

        let scripts = load_scripts();
        if scripts.is_empty() {
            if raw == "sc" || raw == "scripts" {
                return vec![QueryResult {
                    id: "script_runner:empty".to_string(),
                    title: "No scripts yet".to_string(),
                    subtitle: Some(format!("Add .sh files to {}", scripts_dir().display())),
                    icon: Some("📂".to_string()),
                    score: 50,
                    action_type: "shell".to_string(),
                    action_data: format!("mkdir -p {}", scripts_dir().display()),
                }];
            }
            return vec![];
        }

        scripts
            .into_iter()
            .filter(|s| {
                filter.is_empty()
                    || s.name.to_lowercase().contains(&filter)
                    || s.path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_lowercase().contains(&filter))
                        .unwrap_or(false)
            })
            .map(|s| {
                let score = if filter.is_empty() {
                    60
                } else if s.name.to_lowercase().starts_with(&filter) {
                    95
                } else {
                    75
                };
                QueryResult {
                    id: format!("script:{}", s.path.display()),
                    title: format!("{} {}", s.icon, s.name),
                    subtitle: if s.desc.is_empty() {
                        Some(s.path.display().to_string())
                    } else {
                        Some(s.desc)
                    },
                    icon: None,
                    score,
                    action_type: "shell".to_string(),
                    action_data: argv_to_shell_string(&shell_run_cmd(&s.path)),
                }
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "run_user_script",
                "description": "Run a user-defined shell script from the scripts folder",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "script_name": {
                            "type": "string",
                            "description": "Script name or filename (without .sh)"
                        }
                    },
                    "required": ["script_name"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let name = args["script_name"].as_str().unwrap_or("").to_lowercase();
        let scripts = load_scripts();
        let found = scripts.iter().find(|s| {
            s.name.to_lowercase().contains(&name)
                || s.path
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .map(|n| n.to_lowercase().contains(&name))
                    .unwrap_or(false)
        });
        match found {
            Some(s) => {
                let argv = shell_run_cmd(&s.path);
                let (prog, rest) = match argv.split_first() {
                    Some((p, r)) => (p.as_str(), r),
                    None => return "Failed to run script: empty command".to_string(),
                };
                match std::process::Command::new(prog).args(rest).output() {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                        if out.status.success() {
                            if stdout.is_empty() {
                                format!("Script '{}' ran successfully", s.name)
                            } else {
                                stdout
                            }
                        } else {
                            format!("Script failed: {}", stderr)
                        }
                    }
                    Err(e) => format!("Failed to run script: {}", e),
                }
            }
            None => format!("No script found matching '{}'", name),
        }
    }
}
