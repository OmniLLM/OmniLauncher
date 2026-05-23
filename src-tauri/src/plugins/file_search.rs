use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use walkdir::WalkDir;

pub struct FileSearchPlugin;

#[async_trait]
impl Plugin for FileSearchPlugin {
    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Search files in your home directory"
    }

    fn keyword(&self) -> Option<&str> {
        None // We handle "f " and "open " prefixes manually
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = &q.raw;
        let term = if let Some(t) = raw.strip_prefix("f ") {
            t.trim()
        } else if let Some(t) = raw.strip_prefix("open ") {
            t.trim()
        } else {
            return vec![];
        };

        if term.is_empty() {
            return vec![];
        }

        let term_lower = term.to_lowercase();
        let home = match dirs::home_dir() {
            Some(h) => h,
            None => return vec![],
        };

        let mut results = vec![];
        let walker = WalkDir::new(&home)
            .max_depth(5)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok());

        for entry in walker {
            let file_name = entry.file_name().to_string_lossy().to_lowercase();
            if file_name.contains(&term_lower) {
                let path = entry.path().to_string_lossy().to_string();
                let score = if file_name == term_lower { 95 } else { 70 };
                let icon = if entry.file_type().is_dir() {
                    "📁"
                } else {
                    "📄"
                };
                results.push(QueryResult {
                    id: format!("file:{}", path),
                    title: entry.file_name().to_string_lossy().to_string(),
                    subtitle: Some(path.clone()),
                    icon: Some(icon.to_string()),
                    score,
                    action_type: "open".to_string(),
                    action_data: path,
                });
                if results.len() >= 10 {
                    break;
                }
            }
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "file_search",
                "description": "Search for files on the filesystem",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Filename or partial name to search for" }
                    },
                    "required": ["query"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let query = args["query"].as_str().unwrap_or("");
        let q = Query {
            raw: format!("f {}", query),
            terms: vec![query.to_string()],
        };
        let results = self.query(&q).await;
        if results.is_empty() {
            format!("No files found matching '{}'", query)
        } else {
            results
                .iter()
                .map(|r| r.action_data.clone())
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}
