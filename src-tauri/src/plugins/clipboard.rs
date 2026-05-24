use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Clipboard history plugin. Keyword: `cb `
/// Stores up to 50 entries in memory.
pub struct ClipboardPlugin {
    pub history: Vec<String>,
}

impl ClipboardPlugin {
    pub fn new() -> Self {
        Self { history: Vec::new() }
    }

    /// Add an entry to history (ring buffer of 50).
    pub fn add_entry(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        // Avoid duplicates — move to front instead
        self.history.retain(|e| e != &text);
        self.history.insert(0, text);
        if self.history.len() > 50 {
            self.history.truncate(50);
        }
    }

    /// Query entries by search term.
    pub fn search(&self, term: &str) -> Vec<&String> {
        let lower = term.to_lowercase();
        self.history
            .iter()
            .filter(|e| e.to_lowercase().contains(&lower))
            .collect()
    }
}

#[async_trait]
impl Plugin for ClipboardPlugin {
    fn name(&self) -> &str {
        "clipboard"
    }

    fn description(&self) -> &str {
        "Clipboard history (last 50 entries). Use prefix 'cb '"
    }

    fn keyword(&self) -> Option<&str> {
        Some("cb")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();
        // Expect "cb <search_term>"
        let search_term = if raw.to_lowercase().starts_with("cb ") {
            raw[3..].trim()
        } else {
            return vec![];
        };

        self.search(search_term)
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let preview = if entry.len() > 60 {
                    format!("{}…", &entry[..60])
                } else {
                    entry.to_string()
                };
                QueryResult {
                    id: format!("cb:{}", i),
                    title: preview,
                    subtitle: Some(format!("Clipboard entry #{}", i + 1)),
                    icon: Some("📋".to_string()),
                    score: (90 - i as i32).max(0),
                    action_type: "copy".to_string(),
                    action_data: entry.to_string(),
                }
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "clipboard_search",
                "description": "Search clipboard history for a term",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "term": { "type": "string", "description": "Search term" }
                    },
                    "required": ["term"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let term = args["term"].as_str().unwrap_or("");
        let results = self.search(term);
        if results.is_empty() {
            format!("No clipboard entries matching '{}'", term)
        } else {
            results.iter().enumerate()
                .map(|(i, e)| format!("{}. {}", i + 1, e))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}
