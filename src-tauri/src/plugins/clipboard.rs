use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Clipboard history plugin. Keyword: `cb `
/// Stores up to 50 entries in memory.
pub struct ClipboardPlugin {
    pub history: Vec<String>,
}

impl Default for ClipboardPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardPlugin {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
        }
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
                // BUGFIX: `&entry[..60]` panics with `byte index N is not a
                // char boundary` whenever the clipboard contains multi-byte
                // UTF-8 (CJK text, emoji, accented chars, …). Slice on a
                // real char boundary instead so the launcher never crashes
                // mid-query just because the user copied "café" or "🚀".
                let preview = if entry.len() > 60 {
                    format!("{}…", truncate_on_char_boundary(entry, 60))
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
                    source: None,
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
            results
                .iter()
                .enumerate()
                .map(|(i, e)| format!("{}. {}", i + 1, e))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }
}

/// Return the longest prefix of `s` that is `<= max_bytes` bytes AND ends on
/// a UTF-8 character boundary. Naïve byte slicing (`&s[..n]`) panics when
/// `n` falls inside a multi-byte sequence — see the BUGFIX note above.
fn truncate_on_char_boundary(s: &str, max_bytes: usize) -> &str {
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
mod clipboard_truncate_tests {
    use super::truncate_on_char_boundary;

    #[test]
    fn ascii_under_limit_unchanged() {
        assert_eq!(truncate_on_char_boundary("hello", 60), "hello");
    }

    #[test]
    fn ascii_truncates_exactly() {
        let s = "x".repeat(100);
        assert_eq!(truncate_on_char_boundary(&s, 60).len(), 60);
    }

    #[test]
    fn cjk_does_not_panic_and_stays_valid_utf8() {
        // 3-byte chars; 25 of them = 75 bytes, > 60.
        let s: String = "中".repeat(25);
        let out = truncate_on_char_boundary(&s, 60);
        assert!(out.len() <= 60);
        // Must be a multiple of 3 (whole CJK chars only).
        assert_eq!(out.len() % 3, 0);
    }

    #[test]
    fn emoji_does_not_panic() {
        // 4-byte emoji; force the boundary mid-codepoint.
        let s: String = "🚀".repeat(20);
        let out = truncate_on_char_boundary(&s, 61); // 61 is mid-emoji
        assert!(out.len() <= 61);
        assert_eq!(out.len() % 4, 0);
    }
}
