use crate::guardrails::{GuardrailAction, Guardrails};
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Write/create files - inspired by codex/claude-code/opencode write tool
pub struct FileWritePlugin;

#[async_trait]
impl Plugin for FileWritePlugin {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write or create files"
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
                "name": "file_write",
                "description": "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Creates parent directories as needed.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute or relative path to the file" },
                        "content": { "type": "string", "description": "The content to write to the file" },
                        "append": { "type": "boolean", "description": "If true, append to file instead of overwriting" }
                    },
                    "required": ["path", "content"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let path = args["path"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");
        let append = args["append"].as_bool().unwrap_or(false);

        if path.is_empty() {
            return "Error: no path provided".to_string();
        }

        if let GuardrailAction::Deny(reason) = Guardrails::check_file_write(path) {
            return format!("Error: guardrail denied file_write: {}", reason);
        }

        let file_path = std::path::Path::new(path);

        // Create parent directories
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return format!("Error creating directories: {}", e);
                }
            }
        }

        let result = if append {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .and_then(|mut f| f.write_all(content.as_bytes()))
        } else {
            std::fs::write(path, content)
        };

        match result {
            Ok(_) => format!("Successfully wrote {} bytes to {}", content.len(), path),
            Err(e) => format!("Error writing file: {}", e),
        }
    }
}
