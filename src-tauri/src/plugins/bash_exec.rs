use crate::guardrails::{GuardrailAction, Guardrails};
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;
#[cfg(target_os = "windows")]
use std::time::{SystemTime, UNIX_EPOCH};

/// Write `command` to a temp `.ps1` script and return its path. Using a script
/// file with `-File` sidesteps PowerShell's `-Command` parser, which otherwise
/// mangles embedded quotes in JSON / here-strings (the cause of skills like
/// openstack receiving an empty stdin).
///
/// A UTF-8 BOM is prepended so PowerShell reads non-ASCII content correctly on
/// Windows (without it, the default code page may misinterpret bytes).
#[cfg(target_os = "windows")]
fn write_temp_ps1(command: &str) -> std::io::Result<std::path::PathBuf> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "omnilauncher-shell-{}-{}.ps1",
        std::process::id(),
        ts
    ));
    let mut bytes = Vec::with_capacity(command.len() + 3);
    bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]); // UTF-8 BOM
    bytes.extend_from_slice(command.as_bytes());
    std::fs::write(&path, bytes)?;
    Ok(path)
}

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
        // This plugin is an AI tool only — no UI keyword to avoid duplicate `>` with ShellPlugin
        None
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
                source: None,
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
            source: None,
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

        // Guardrails check — same protection as ShellPlugin (user-facing `>` prefix)
        match Guardrails::check_shell_command(command) {
            GuardrailAction::Deny(reason) => {
                return format!("Blocked by guardrails: {}", reason);
            }
            GuardrailAction::Warn(reason) => {
                log::warn!("[guardrails] shell_exec WARN: {}", reason);
            }
            GuardrailAction::Allow => {}
        }

        // Log the literal command we're about to execute so failures can be
        // diagnosed (e.g. AI-generated bash-style quoting that PowerShell
        // mangles, or empty-stdin issues from skills that pipe JSON in).
        log::info!("shell_exec: command={:?}", command);

        // On Windows, route the command through a temp `.ps1` invoked with
        // `-File` rather than `-Command`. PowerShell's `-Command` parser
        // re-tokenises the argument and mangles embedded quotes — for example
        // a JSON payload piped via `echo '{"op":...}'` arrives at the child
        // process with the inner quotes stripped, so a Python script reading
        // stdin sees an empty string. `-File` runs the script as-is.
        #[cfg(target_os = "windows")]
        let temp_script = match write_temp_ps1(command) {
            Ok(p) => Some(p),
            Err(e) => return format!("Error preparing PowerShell script: {}", e),
        };

        let mut cmd = if cfg!(target_os = "windows") {
            #[cfg(target_os = "windows")]
            {
                let script_path = temp_script
                    .as_ref()
                    .expect("temp_script set on windows")
                    .to_string_lossy()
                    .into_owned();
                let mut c = Command::new("powershell");
                c.args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-File",
                    &script_path,
                ]);
                c
            }
            #[cfg(not(target_os = "windows"))]
            unreachable!()
        } else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let output_result = cmd.output();

        // Best-effort cleanup of the temp script regardless of success.
        #[cfg(target_os = "windows")]
        if let Some(p) = &temp_script {
            let _ = std::fs::remove_file(p);
        }

        match output_result {
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
                    // Truncate very long output. The cap must be high enough
                    // to fit a full skill response — e.g. the openstack
                    // skill's `tool_call` JSON for a 50-project listing is
                    // ~25 KB, and truncating it mid-JSON makes the AI report
                    // "0 results" because it can't parse the payload.
                    const MAX: usize = 64_000;
                    if result.len() > MAX {
                        result.truncate(MAX);
                        result.push_str("\n... (truncated)");
                    }
                    result
                }
            }
            Err(e) => format!("Error executing command: {}", e),
        }
    }
}
