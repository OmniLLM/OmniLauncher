use crate::guardrails::{GuardrailAction, Guardrails};
use crate::plugins::{truncate_on_char_boundary, Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

/// Apply patches/edits to files - inspired by codex apply_patch and opencode edit tool
pub struct PatchPlugin;

#[async_trait]
impl Plugin for PatchPlugin {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Edit files by replacing text patterns"
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
                "name": "file_edit",
                "description": "Edit a file by replacing an exact string occurrence with new content. The old_string must match exactly (including whitespace/indentation).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Path to the file to edit" },
                        "old_string": { "type": "string", "description": "The exact text to find and replace" },
                        "new_string": { "type": "string", "description": "The replacement text" }
                    },
                    "required": ["path", "old_string", "new_string"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let path = args["path"].as_str().unwrap_or("");
        let old_string = args["old_string"].as_str().unwrap_or("");
        let new_string = args["new_string"].as_str().unwrap_or("");

        if path.is_empty() || old_string.is_empty() {
            return "Error: path and old_string are required".to_string();
        }

        // file_edit both reads and writes; enforce both guardrails.
        if let GuardrailAction::Deny(reason) = Guardrails::check_file_read(path) {
            return format!("Error: guardrail denied file_edit: {}", reason);
        }
        if let GuardrailAction::Deny(reason) = Guardrails::check_file_write(path) {
            return format!("Error: guardrail denied file_edit: {}", reason);
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return format!("Error reading file: {}", e),
        };

        let count = content.matches(old_string).count();
        if count == 0 {
            return "Error: old_string not found in file".to_string();
        }
        if count > 1 {
            return format!("Error: old_string found {} times, must be unique", count);
        }

        let new_content = content.replacen(old_string, new_string, 1);
        match std::fs::write(path, &new_content) {
            Ok(_) => format!("Successfully edited {}", path),
            Err(e) => format!("Error writing file: {}", e),
        }
    }
}

/// Code execution in a sandbox - inspired by hermes execute_code tool
pub struct CodeExecPlugin;

#[async_trait]
impl Plugin for CodeExecPlugin {
    fn name(&self) -> &str {
        "code_execute"
    }

    fn description(&self) -> &str {
        "Execute code snippets in various languages"
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
                "name": "code_execute",
                "description": "Execute a code snippet. Supports python, javascript/node, powershell, bash. Code is written to a temp file and executed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "language": { "type": "string", "enum": ["python", "javascript", "powershell", "bash", "rust"], "description": "Programming language" },
                        "code": { "type": "string", "description": "The code to execute" }
                    },
                    "required": ["language", "code"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let language = args["language"].as_str().unwrap_or("python");
        let code = args["code"].as_str().unwrap_or("");

        if code.is_empty() {
            return "Error: no code provided".to_string();
        }

        let (ext, cmd_name, cmd_args): (&str, &str, Vec<&str>) = match language {
            "python" => ("py", "python", vec![]),
            "javascript" => ("js", "node", vec![]),
            "powershell" => ("ps1", "powershell", vec!["-NoProfile", "-File"]),
            "bash" => ("sh", "bash", vec![]),
            "rust" => {
                // For Rust, we need to compile and run
                return execute_rust(code);
            }
            _ => return format!("Unsupported language: {}", language),
        };

        let temp_dir = std::env::temp_dir();
        let run_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos();
        let temp_file = temp_dir.join(format!(
            "omnilauncher_exec_{}_{}.{}",
            std::process::id(),
            run_id,
            ext
        ));
        if let Err(e) = std::fs::write(&temp_file, code) {
            return format!("Error writing temp file: {}", e);
        }

        let mut cmd = Command::new(cmd_name);
        for arg in &cmd_args {
            cmd.arg(arg);
        }
        cmd.arg(&temp_file);

        let result = match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let mut r = String::new();
                if !stdout.is_empty() {
                    r.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !r.is_empty() {
                        r.push_str("\n--- stderr ---\n");
                    }
                    r.push_str(&stderr);
                }
                if r.is_empty() {
                    format!(
                        "Executed successfully (exit code: {})",
                        output.status.code().unwrap_or(-1)
                    )
                } else {
                    r
                }
            }
            Err(e) => format!("Error executing code: {}", e),
        };

        let _ = std::fs::remove_file(&temp_file);
        if result.len() > 4000 {
            format!(
                "{}\n... (truncated)",
                truncate_on_char_boundary(&result, 4000)
            )
        } else {
            result
        }
    }
}

fn execute_rust(code: &str) -> String {
    let temp_dir = std::env::temp_dir();
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let src = temp_dir.join(format!(
        "omnilauncher_exec_{}_{}.rs",
        std::process::id(),
        run_id
    ));
    let exe = temp_dir.join(format!(
        "omnilauncher_exec_bin_{}_{}",
        std::process::id(),
        run_id
    ));

    if let Err(e) = std::fs::write(&src, code) {
        return format!("Error writing temp file: {}", e);
    }

    // Compile
    let compile = Command::new("rustc")
        .args([src.to_str().unwrap_or(""), "-o", exe.to_str().unwrap_or("")])
        .output();

    match compile {
        Ok(output) if output.status.success() => {
            // Run
            match Command::new(&exe).output() {
                Ok(run_output) => {
                    let _ = std::fs::remove_file(&src);
                    let _ = std::fs::remove_file(&exe);
                    let stdout = String::from_utf8_lossy(&run_output.stdout);
                    let stderr = String::from_utf8_lossy(&run_output.stderr);
                    format!("{}{}", stdout, stderr)
                }
                Err(e) => format!("Error running compiled binary: {}", e),
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!("Compilation error:\n{}", stderr)
        }
        Err(e) => format!("Error invoking rustc: {}", e),
    }
}
