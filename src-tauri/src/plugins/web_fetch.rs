use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Fetch web page content - inspired by claude-code/opencode webfetch tool
pub struct WebFetchPlugin;

#[async_trait]
impl Plugin for WebFetchPlugin {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch content from a URL"
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
                "name": "web_fetch",
                "description": "Fetch content from a URL. Returns the page text content (HTML stripped to text). Use for reading web pages, APIs, documentation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The URL to fetch" },
                        "raw": { "type": "boolean", "description": "If true, return raw HTML instead of extracted text" }
                    },
                    "required": ["url"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let url = args["url"].as_str().unwrap_or("");
        let raw = args["raw"].as_bool().unwrap_or(false);

        if url.is_empty() {
            return "Error: no URL provided".to_string();
        }

        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(c) => c,
            Err(e) => return format!("Error creating client: {}", e),
        };

        match client.get(url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return format!("HTTP error: {}", resp.status());
                }
                match resp.text().await {
                    Ok(text) => {
                        let result = if raw { text } else { strip_html(&text) };
                        if result.len() > 8000 {
                            format!("{}\n... (truncated)", &result[..8000])
                        } else {
                            result
                        }
                    }
                    Err(e) => format!("Error reading response: {}", e),
                }
            }
            Err(e) => format!("Error fetching URL: {}", e),
        }
    }
}

fn strip_html(html: &str) -> String {
    // Simple HTML to text: remove tags, decode basic entities
    let mut text = html.to_string();
    // Remove script/style blocks
    let re_script = regex::Regex::new(r"(?is)<script[^>]*>.*?</script>")
        .unwrap_or_else(|_| regex::Regex::new(".^").unwrap());
    text = re_script.replace_all(&text, "").to_string();
    let re_style = regex::Regex::new(r"(?is)<style[^>]*>.*?</style>")
        .unwrap_or_else(|_| regex::Regex::new(".^").unwrap());
    text = re_style.replace_all(&text, "").to_string();
    // Remove tags
    let re_tags =
        regex::Regex::new(r"<[^>]+>").unwrap_or_else(|_| regex::Regex::new(".^").unwrap());
    text = re_tags.replace_all(&text, " ").to_string();
    // Decode entities
    text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    // Collapse whitespace
    let re_ws = regex::Regex::new(r"\s+").unwrap_or_else(|_| regex::Regex::new(".^").unwrap());
    text = re_ws.replace_all(&text, " ").to_string();
    text.trim().to_string()
}
