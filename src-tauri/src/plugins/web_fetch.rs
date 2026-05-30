use crate::guardrails::{GuardrailAction, Guardrails};
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use regex::Regex;
use std::sync::LazyLock;

/// Compiled once at first use instead of recompiling on every `strip_html` call.
static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default()
});

static RE_SCRIPT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap());
static RE_STYLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap());
static RE_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static RE_WS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

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

        if let GuardrailAction::Deny(reason) = Guardrails::check_url(url) {
            return format!("Error: guardrail denied web_fetch: {}", reason);
        }

        let client = &*CLIENT;

        match client.get(url).send().await {
            Ok(resp) => {
                if !resp.status().is_success() {
                    return format!("HTTP error: {}", resp.status());
                }
                match resp.text().await {
                    Ok(text) => {
                        let result = if raw { text } else { strip_html(&text) };
                        if result.len() > 8000 {
                            format!(
                                "{}\n... (truncated)",
                                truncate_at_char_boundary(&result, 8000)
                            )
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
    // Remove script/style blocks (regexes compiled once, see statics above)
    text = RE_SCRIPT.replace_all(&text, "").to_string();
    text = RE_STYLE.replace_all(&text, "").to_string();
    // Remove tags
    text = RE_TAGS.replace_all(&text, " ").to_string();
    // Decode entities
    text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    // Collapse whitespace
    text = RE_WS.replace_all(&text, " ").to_string();
    text.trim().to_string()
}

/// Truncate a UTF-8 string to at most `max_bytes` bytes without splitting a
/// multi-byte character. Returns the largest valid prefix `<= max_bytes`.
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

#[cfg(test)]
mod truncate_tests {
    use super::truncate_at_char_boundary;

    #[test]
    fn ascii_short_string_unchanged() {
        assert_eq!(truncate_at_char_boundary("hello", 100), "hello");
    }

    #[test]
    fn ascii_truncated_exactly() {
        assert_eq!(truncate_at_char_boundary("abcdef", 3), "abc");
    }

    #[test]
    fn cjk_boundary_does_not_panic() {
        // Each Chinese character is 3 bytes in UTF-8 -> 9000 bytes.
        let s: String = "中".repeat(3000);
        let out = truncate_at_char_boundary(&s, 8000);
        assert!(out.len() <= 8000);
        assert!(out.len() >= 7998); // last whole char boundary <= 8000
                                    // Round-trips as valid UTF-8 with only whole CJK chars.
        assert_eq!(out.chars().count(), out.len() / 3);
    }

    #[test]
    fn emoji_boundary_does_not_panic() {
        // 4-byte emoji * 2500 = 10000 bytes
        let s: String = "🚀".repeat(2500);
        let out = truncate_at_char_boundary(&s, 8000);
        assert!(out.len() <= 8000);
        assert_eq!(out.len() % 4, 0);
    }
}
