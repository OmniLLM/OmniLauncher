use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct WebSearchPlugin;

#[async_trait]
impl Plugin for WebSearchPlugin {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web using Google, YouTube, or GitHub"
    }

    fn keyword(&self) -> Option<&str> {
        None // handles multiple prefixes manually
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = &q.raw;
        let mut results = vec![];

        if let Some(term) = raw.strip_prefix("g ") {
            let encoded = urlencoding(term);
            results.push(QueryResult {
                id: format!("google:{}", term),
                title: format!("Search Google: {}", term),
                subtitle: Some("google.com".to_string()),
                icon: Some("🔍".to_string()),
                score: 90,
                action_type: "url".to_string(),
                action_data: format!("https://www.google.com/search?q={}", encoded),
            });
        } else if let Some(term) = raw.strip_prefix("yt ") {
            let encoded = urlencoding(term);
            results.push(QueryResult {
                id: format!("youtube:{}", term),
                title: format!("Search YouTube: {}", term),
                subtitle: Some("youtube.com".to_string()),
                icon: Some("▶️".to_string()),
                score: 90,
                action_type: "url".to_string(),
                action_data: format!("https://www.youtube.com/results?search_query={}", encoded),
            });
        } else if let Some(term) = raw.strip_prefix("gh ") {
            let encoded = urlencoding(term);
            results.push(QueryResult {
                id: format!("github:{}", term),
                title: format!("Search GitHub: {}", term),
                subtitle: Some("github.com".to_string()),
                icon: Some("🐙".to_string()),
                score: 90,
                action_type: "url".to_string(),
                action_data: format!("https://github.com/search?q={}", encoded),
            });
        } else if !raw.is_empty() && !raw.starts_with('>') && !raw.starts_with('=') && !raw.starts_with("sys ") && !raw.starts_with("f ") {
            // fallback: bare query → Google
            let encoded = urlencoding(raw);
            results.push(QueryResult {
                id: format!("google_fallback:{}", raw),
                title: format!("Search Google: {}", raw),
                subtitle: Some("google.com".to_string()),
                icon: Some("🔍".to_string()),
                score: 30,
                action_type: "url".to_string(),
                action_data: format!("https://www.google.com/search?q={}", encoded),
            });
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the web",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "engine": { "type": "string", "enum": ["google", "youtube", "github"], "description": "Search engine" }
                    },
                    "required": ["query"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let query = args["query"].as_str().unwrap_or("");
        let engine = args["engine"].as_str().unwrap_or("google");
        let encoded = urlencoding(query);
        match engine {
            "youtube" => format!("https://www.youtube.com/results?search_query={}", encoded),
            "github" => format!("https://github.com/search?q={}", encoded),
            _ => format!("https://www.google.com/search?q={}", encoded),
        }
    }
}

fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}
