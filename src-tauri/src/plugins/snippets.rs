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
