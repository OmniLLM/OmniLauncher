use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Snippet/text expansion plugin - store and recall text snippets
pub struct SnippetsPlugin;

#[async_trait]
impl Plugin for SnippetsPlugin {
    fn name(&self) -> &str {
        "snippets"
    }

    fn description(&self) -> &str {
        "Store and recall text snippets"
    }

    fn keyword(&self) -> Option<&str> {
        Some("snip ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("snip ")
            .unwrap_or("")
            .trim()
            .to_lowercase();

        let snippets = load_snippets();
        if snippets.is_empty() {
            return vec![QueryResult {
                id: "snip:empty".to_string(),
                title: "No snippets found".to_string(),
                subtitle: Some("Add snippets to ~/.omnilauncher/snippets.json".to_string()),
                icon: Some("📋".to_string()),
                score: 50,
                action_type: "copy".to_string(),
                action_data: String::new(),
            }];
        }

        snippets
            .into_iter()
            .filter(|(name, content)| {
                term.is_empty()
                    || name.to_lowercase().contains(&term)
                    || content.to_lowercase().contains(&term)
            })
            .take(10)
            .map(|(name, content)| {
                let preview = if content.len() > 60 {
                    format!("{}...", &content[..60])
                } else {
                    content.clone()
                };
                QueryResult {
                    id: format!("snip:{}", name),
                    title: name,
                    subtitle: Some(preview),
                    icon: Some("📋".to_string()),
                    score: 70,
                    action_type: "copy".to_string(),
                    action_data: content,
                }
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "snippets",
                "description": "Manage text snippets: list, get, add, or delete",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "description": "'list', 'get', 'add', or 'delete'" },
                        "name": { "type": "string", "description": "Snippet name (required for get/add/delete)" },
                        "content": { "type": "string", "description": "Snippet content (required for add)" }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("").trim();
        match action {
            "list" => {
                let snips = load_snippets();
                if snips.is_empty() {
                    return "No snippets found. Add snippets to ~/.omnilauncher/snippets.json"
                        .to_string();
                }
                snips
                    .iter()
                    .map(|(name, content)| {
                        let preview = if content.len() > 60 {
                            format!("{}...", &content[..60])
                        } else {
                            content.clone()
                        };
                        format!("{}: {}", name, preview)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "get" => {
                let name = match args["name"].as_str() {
                    Some(n) if !n.is_empty() => n,
                    _ => return "Error: 'name' is required for get".to_string(),
                };
                let snips = load_snippets();
                snips
                    .into_iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, c)| c)
                    .unwrap_or_else(|| format!("Snippet '{}' not found", name))
            }
            "add" => {
                let name = match args["name"].as_str() {
                    Some(n) if !n.is_empty() => n,
                    _ => return "Error: 'name' is required for add".to_string(),
                };
                let content = match args["content"].as_str() {
                    Some(c) => c,
                    None => return "Error: 'content' is required for add".to_string(),
                };
                let path = snippets_path();
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let mut map: std::collections::HashMap<String, String> =
                    std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default();
                map.insert(name.to_string(), content.to_string());
                match serde_json::to_string_pretty(&map) {
                    Ok(json) => {
                        let _ = std::fs::write(&path, json);
                        format!("Snippet '{}' saved", name)
                    }
                    Err(e) => format!("Error saving snippet: {}", e),
                }
            }
            "delete" => {
                let name = match args["name"].as_str() {
                    Some(n) if !n.is_empty() => n,
                    _ => return "Error: 'name' is required for delete".to_string(),
                };
                let path = snippets_path();
                let mut map: std::collections::HashMap<String, String> =
                    std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default();
                if map.remove(name).is_some() {
                    match serde_json::to_string_pretty(&map) {
                        Ok(json) => {
                            let _ = std::fs::write(&path, json);
                            format!("Snippet '{}' deleted", name)
                        }
                        Err(e) => format!("Error saving: {}", e),
                    }
                } else {
                    format!("Snippet '{}' not found", name)
                }
            }
            _ => format!("Unknown action: '{}'. Use: list, get, add, delete", action),
        }
    }
}

fn snippets_path() -> std::path::PathBuf {
    let mut path = dirs::home_dir().unwrap_or_default();
    path.push(".omnilauncher");
    path.push("snippets.json");
    path
}

fn load_snippets() -> Vec<(String, String)> {
    let path = snippets_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let map: std::collections::HashMap<String, String> = match serde_json::from_str(&content) {
        Ok(m) => m,
        Err(_) => return vec![],
    };
    map.into_iter().collect()
}
