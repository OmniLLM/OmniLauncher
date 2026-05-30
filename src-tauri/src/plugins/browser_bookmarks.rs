use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use serde::Deserialize;
use std::path::PathBuf;

pub struct BrowserBookmarksPlugin;

#[derive(Debug, Deserialize)]
struct BookmarkEntry {
    name: Option<String>,
    url: Option<String>,
    #[serde(rename = "type")]
    entry_type: Option<String>,
    children: Option<Vec<BookmarkEntry>>,
}

impl BrowserBookmarksPlugin {
    fn get_chrome_bookmarks_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            dirs::data_local_dir().map(|d| {
                d.join("Google")
                    .join("Chrome")
                    .join("User Data")
                    .join("Default")
                    .join("Bookmarks")
            })
        }
        #[cfg(target_os = "macos")]
        {
            dirs::home_dir().map(|d| {
                d.join("Library")
                    .join("Application Support")
                    .join("Google")
                    .join("Chrome")
                    .join("Default")
                    .join("Bookmarks")
            })
        }
        #[cfg(target_os = "linux")]
        {
            dirs::config_dir().map(|d| d.join("google-chrome").join("Default").join("Bookmarks"))
        }
    }

    fn get_edge_bookmarks_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            dirs::data_local_dir().map(|d| {
                d.join("Microsoft")
                    .join("Edge")
                    .join("User Data")
                    .join("Default")
                    .join("Bookmarks")
            })
        }
        #[cfg(target_os = "macos")]
        {
            dirs::home_dir().map(|d| {
                d.join("Library")
                    .join("Application Support")
                    .join("Microsoft Edge")
                    .join("Default")
                    .join("Bookmarks")
            })
        }
        #[cfg(target_os = "linux")]
        {
            dirs::config_dir().map(|d| d.join("microsoft-edge").join("Default").join("Bookmarks"))
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
        {
            None
        }
    }

    fn flatten_bookmarks(entry: &BookmarkEntry, results: &mut Vec<(String, String)>) {
        if let Some(ref t) = entry.entry_type {
            if t == "url" {
                if let (Some(name), Some(url)) = (&entry.name, &entry.url) {
                    results.push((name.clone(), url.clone()));
                }
            }
        }
        if let Some(ref children) = entry.children {
            for child in children {
                Self::flatten_bookmarks(child, results);
            }
        }
    }

    fn load_bookmarks_from_file(path: &PathBuf) -> Vec<(String, String)> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return vec![],
        };
        let mut results = vec![];
        if let Some(roots) = json.get("roots").and_then(|r| r.as_object()) {
            for (_key, val) in roots {
                if let Ok(entry) = serde_json::from_value::<BookmarkEntry>(val.clone()) {
                    Self::flatten_bookmarks(&entry, &mut results);
                }
            }
        }
        results
    }

    fn load_all_bookmarks() -> Vec<(String, String)> {
        let mut all = vec![];
        if let Some(path) = Self::get_chrome_bookmarks_path() {
            if path.exists() {
                all.extend(Self::load_bookmarks_from_file(&path));
            }
        }
        if let Some(path) = Self::get_edge_bookmarks_path() {
            if path.exists() {
                all.extend(Self::load_bookmarks_from_file(&path));
            }
        }
        all
    }
}

#[async_trait]
impl Plugin for BrowserBookmarksPlugin {
    fn name(&self) -> &str {
        "browser_bookmarks"
    }

    fn description(&self) -> &str {
        "Search browser bookmarks from Chrome and Edge"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();
        let term = if let Some(t) = raw.strip_prefix("bm ") {
            t.trim()
        } else if let Some(t) = raw.strip_prefix("b ") {
            t.trim()
        } else {
            return vec![];
        }
        .to_lowercase();
        if term.is_empty() {
            return vec![];
        }

        let bookmarks = Self::load_all_bookmarks();
        bookmarks
            .into_iter()
            .filter(|(name, url)| {
                name.to_lowercase().contains(&term) || url.to_lowercase().contains(&term)
            })
            .take(10)
            .map(|(name, url)| QueryResult {
                id: format!("bm:{}", url),
                title: name,
                subtitle: Some(url.clone()),
                icon: Some("🔖".to_string()),
                score: 70,
                action_type: "open_url".to_string(),
                action_data: url,
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "browser_bookmarks",
                "description": "Search browser bookmarks from Chrome and Edge",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search term to filter bookmarks by title or URL" }
                    },
                    "required": ["query"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let query = args["query"].as_str().unwrap_or("").trim().to_lowercase();
        if query.is_empty() {
            return "Error: 'query' parameter is required".to_string();
        }
        let bookmarks = Self::load_all_bookmarks();
        let matches: Vec<_> = bookmarks
            .into_iter()
            .filter(|(name, url)| {
                name.to_lowercase().contains(&query) || url.to_lowercase().contains(&query)
            })
            .take(20)
            .collect();
        if matches.is_empty() {
            return format!("No bookmarks found matching '{}'", query);
        }
        matches
            .into_iter()
            .map(|(name, url)| format!("- {} — {}", name, url))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
