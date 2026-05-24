use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Todo/memory tool - inspired by hermes-agent todo and memory tools
pub struct TodoPlugin;

#[async_trait]
impl Plugin for TodoPlugin {
    fn name(&self) -> &str {
        "todo_memory"
    }

    fn description(&self) -> &str {
        "Manage a persistent todo list and notes"
    }

    fn keyword(&self) -> Option<&str> {
        Some("todo ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q.raw.strip_prefix("todo ").unwrap_or("").trim();
        if term.is_empty() || term == "list" {
            let items = load_todos();
            if items.is_empty() {
                return vec![QueryResult {
                    id: "todo:empty".to_string(),
                    title: "No todos".to_string(),
                    subtitle: Some("Use 'todo add <text>' to create one".to_string()),
                    icon: Some("📝".to_string()),
                    score: 50,
                    action_type: "copy".to_string(),
                    action_data: String::new(),
                }];
            }
            return items
                .iter()
                .enumerate()
                .map(|(i, item)| QueryResult {
                    id: format!("todo:{}", i),
                    title: item.clone(),
                    subtitle: Some(format!("#{}", i + 1)),
                    icon: Some("☐".to_string()),
                    score: 60,
                    action_type: "copy".to_string(),
                    action_data: item.clone(),
                })
                .collect();
        }
        vec![QueryResult {
            id: "todo:action".to_string(),
            title: format!("Todo: {}", term),
            subtitle: Some("Press Enter to add".to_string()),
            icon: Some("📝".to_string()),
            score: 70,
            action_type: "copy".to_string(),
            action_data: term.to_string(),
        }]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "todo_memory",
                "description": "Manage a persistent todo list and notes. Actions: list, add, remove, clear, note_save, note_read.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["list", "add", "remove", "clear", "note_save", "note_read"], "description": "Action to perform" },
                        "text": { "type": "string", "description": "Todo text (for add), index (for remove), note key (for note_save/note_read)" },
                        "content": { "type": "string", "description": "Note content (for note_save)" }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("list");
        let text = args["text"].as_str().unwrap_or("");
        let content = args["content"].as_str().unwrap_or("");

        match action {
            "list" => {
                let items = load_todos();
                if items.is_empty() {
                    "Todo list is empty.".to_string()
                } else {
                    items
                        .iter()
                        .enumerate()
                        .map(|(i, item)| format!("{}. {}", i + 1, item))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            "add" => {
                if text.is_empty() {
                    return "Error: no text provided".to_string();
                }
                let mut items = load_todos();
                items.push(text.to_string());
                save_todos(&items);
                format!("Added: {}", text)
            }
            "remove" => {
                let idx: usize = text.parse().unwrap_or(0);
                let mut items = load_todos();
                if idx == 0 || idx > items.len() {
                    return format!("Invalid index: {}. List has {} items.", text, items.len());
                }
                let removed = items.remove(idx - 1);
                save_todos(&items);
                format!("Removed: {}", removed)
            }
            "clear" => {
                save_todos(&vec![]);
                "Todo list cleared.".to_string()
            }
            "note_save" => {
                if text.is_empty() || content.is_empty() {
                    return "Error: need text (key) and content".to_string();
                }
                let notes_dir = notes_dir();
                let _ = std::fs::create_dir_all(&notes_dir);
                let path = notes_dir.join(format!("{}.md", text));
                match std::fs::write(&path, content) {
                    Ok(_) => format!("Note '{}' saved.", text),
                    Err(e) => format!("Error saving note: {}", e),
                }
            }
            "note_read" => {
                if text.is_empty() {
                    return "Error: need note key".to_string();
                }
                let path = notes_dir().join(format!("{}.md", text));
                match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => format!("Note '{}' not found.", text),
                }
            }
            _ => format!("Unknown action: {}", action),
        }
    }
}

fn data_dir() -> std::path::PathBuf {
    let mut p = dirs::home_dir().unwrap_or_default();
    p.push(".omnilauncher");
    p
}

fn notes_dir() -> std::path::PathBuf {
    data_dir().join("notes")
}

fn todos_path() -> std::path::PathBuf {
    data_dir().join("todos.json")
}

fn load_todos() -> Vec<String> {
    let path = todos_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    serde_json::from_str(&content).unwrap_or_default()
}

fn save_todos(items: &[String]) {
    let path = todos_path();
    let _ = std::fs::create_dir_all(path.parent().unwrap_or(&std::path::PathBuf::from(".")));
    let _ = std::fs::write(&path, serde_json::to_string_pretty(items).unwrap_or_default());
}
