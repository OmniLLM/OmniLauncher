use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct UrlOpenerPlugin;

#[async_trait]
impl Plugin for UrlOpenerPlugin {
    fn name(&self) -> &str {
        "url_opener"
    }

    fn description(&self) -> &str {
        "Open URLs directly from the launcher"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();
        if raw.starts_with("http://") || raw.starts_with("https://") || raw.starts_with("localhost:") {
            vec![QueryResult {
                id: format!("url:{}", raw),
                title: format!("Open: {}", raw),
                subtitle: Some("Press Enter to open URL in browser".to_string()),
                icon: Some("🌐".to_string()),
                score: 100,
                action_type: "url".to_string(),
                action_data: raw.to_string(),
            }]
        } else {
            vec![]
        }
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "open_url",
                "description": "Open a URL in the default browser",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": {
                            "type": "string",
                            "description": "The URL to open"
                        }
                    },
                    "required": ["url"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let url = args["url"].as_str().unwrap_or("");
        if url.is_empty() {
            return "Error: no URL provided".to_string();
        }

        let result = {
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("cmd")
                    .args(["/C", "start", "", url])
                    .spawn()
            }
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open").arg(url).spawn()
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("xdg-open").arg(url).spawn()
            }
        };

        match result {
            Ok(_) => format!("Opened {}", url),
            Err(e) => format!("Failed to open {}: {}", url, e),
        }
    }
}