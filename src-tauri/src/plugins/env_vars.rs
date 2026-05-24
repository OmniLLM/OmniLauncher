use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Quick access to environment variables
pub struct EnvVarsPlugin;

#[async_trait]
impl Plugin for EnvVarsPlugin {
    fn name(&self) -> &str {
        "env_vars"
    }

    fn description(&self) -> &str {
        "Search and copy environment variables"
    }

    fn keyword(&self) -> Option<&str> {
        Some("env ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("env ")
            .unwrap_or("")
            .trim()
            .to_lowercase();
        if term.is_empty() {
            return vec![QueryResult {
                id: "env:help".to_string(),
                title: "Environment Variables".to_string(),
                subtitle: Some("Type to filter environment variables".to_string()),
                icon: Some("🔑".to_string()),
                score: 50,
                action_type: "copy".to_string(),
                action_data: String::new(),
            }];
        }

        std::env::vars()
            .filter(|(key, val)| {
                key.to_lowercase().contains(&term) || val.to_lowercase().contains(&term)
            })
            .take(10)
            .map(|(key, val)| {
                let preview = if val.len() > 80 {
                    format!("{}...", &val[..80])
                } else {
                    val.clone()
                };
                QueryResult {
                    id: format!("env:{}", key),
                    title: key.clone(),
                    subtitle: Some(preview),
                    icon: Some("🔑".to_string()),
                    score: 65,
                    action_type: "copy".to_string(),
                    action_data: val,
                }
            })
            .collect()
    }
}
