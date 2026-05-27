use crate::guardrails::{GuardrailAction, Guardrails};
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Read file contents - inspired by codex/claude-code/opencode read tool
pub struct FileReadPlugin;

#[async_trait]
impl Plugin for FileReadPlugin {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read file contents"
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
                "name": "file_read",
                "description": "Read the contents of a file. Returns the file text. Supports text files of any type.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Absolute or relative path to the file" },
                        "start_line": { "type": "integer", "description": "Optional 1-based start line" },
                        "end_line": { "type": "integer", "description": "Optional 1-based end line" }
                    },
                    "required": ["path"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let path = args["path"].as_str().unwrap_or("");
        if path.is_empty() {
            return "Error: no path provided".to_string();
        }

        if let GuardrailAction::Deny(reason) = Guardrails::check_file_read(path) {
            return format!("Error: guardrail denied file_read: {}", reason);
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return format!("Error reading file: {}", e),
        };

        let start = args["start_line"].as_u64().unwrap_or(1).max(1) as usize;
        let end = args["end_line"].as_u64().unwrap_or(0) as usize;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();
        let start_idx = (start - 1).min(total_lines);
        let end_idx = if end > 0 {
            end.min(total_lines)
        } else {
            total_lines
        };
        let returned_lines = end_idx.saturating_sub(start_idx);

        let selected: Vec<String> = lines[start_idx..end_idx]
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:4} | {}", start_idx + i + 1, line))
            .collect();

        let result = selected.join("\n");
        const MAX_BYTES: usize = 8000;
        if result.len() > MAX_BYTES {
            // Truncate on a char boundary to avoid splitting multi-byte UTF-8.
            let mut cut = MAX_BYTES;
            while cut > 0 && !result.is_char_boundary(cut) {
                cut -= 1;
            }
            format!(
                "{}\n[TRUNCATED] showed {} bytes of {} (lines {}..{} of {}); call file_read again with start_line/end_line to fetch more.",
                &result[..cut],
                cut,
                result.len(),
                start_idx + 1,
                start_idx + returned_lines,
                total_lines
            )
        } else {
            result
        }
    }
}
