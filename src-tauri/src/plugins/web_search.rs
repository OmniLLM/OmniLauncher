use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// All known prefixes — used both in query() and the fallback exclusion list.
/// Format: (prefix, name, icon, url_template, subtitle)
const SEARCH_ENGINES: &[(&str, &str, &str, &str, &str)] = &[
    // short aliases (kept for muscle-memory)
    (
        "g ",
        "Google",
        "🔍",
        "https://www.google.com/search?q={}",
        "google.com",
    ),
    (
        "yt ",
        "YouTube",
        "▶️",
        "https://www.youtube.com/results?search_query={}",
        "youtube.com",
    ),
    (
        "gh ",
        "GitHub",
        "🐙",
        "https://github.com/search?q={}",
        "github.com",
    ),
    // Flow.Launcher-compatible keywords
    (
        "youtube ",
        "YouTube",
        "▶️",
        "https://www.youtube.com/results?search_query={}",
        "youtube.com",
    ),
    (
        "netflix ",
        "Netflix",
        "🎬",
        "https://www.netflix.com/search?q={}",
        "netflix.com",
    ),
    (
        "translate ",
        "Google Translate",
        "🌐",
        "https://translate.google.com/#auto|en|{}",
        "translate.google.com",
    ),
    (
        "gmail ",
        "Gmail",
        "📧",
        "https://mail.google.com/mail/ca/u/0/#apps/{}",
        "mail.google.com",
    ),
    (
        "drive ",
        "Google Drive",
        "📁",
        "https://drive.google.com/?hl=en&tab=bo#search/{}",
        "drive.google.com",
    ),
    (
        "ytmusic ",
        "YouTube Music",
        "🎵",
        "https://music.youtube.com/search?q={}",
        "music.youtube.com",
    ),
    (
        "wiki ",
        "Wikipedia",
        "📖",
        "https://en.wikipedia.org/wiki/{}",
        "wikipedia.org",
    ),
    (
        "facebook ",
        "Facebook",
        "👥",
        "https://www.facebook.com/search/?q={}",
        "facebook.com",
    ),
    (
        "twitter ",
        "Twitter / X",
        "🐦",
        "https://twitter.com/search?q={}",
        "twitter.com",
    ),
    (
        "maps ",
        "Google Maps",
        "🗺️",
        "https://maps.google.com/maps?q={}",
        "maps.google.com",
    ),
    (
        "duckduckgo ",
        "DuckDuckGo",
        "🦆",
        "https://duckduckgo.com/?q={}",
        "duckduckgo.com",
    ),
    (
        "ddg ",
        "DuckDuckGo",
        "🦆",
        "https://duckduckgo.com/?q={}",
        "duckduckgo.com",
    ),
    (
        "github ",
        "GitHub",
        "🐙",
        "https://github.com/search?q={}",
        "github.com",
    ),
    (
        "gist ",
        "GitHub Gist",
        "📝",
        "https://gist.github.com/search?q={}",
        "gist.github.com",
    ),
    (
        "wolframalpha ",
        "WolframAlpha",
        "🧮",
        "https://www.wolframalpha.com/input/?i={}",
        "wolframalpha.com",
    ),
    (
        "so ",
        "Stack Overflow",
        "💬",
        "https://stackoverflow.com/search?q={}",
        "stackoverflow.com",
    ),
    (
        "stackoverflow ",
        "Stack Overflow",
        "💬",
        "https://stackoverflow.com/search?q={}",
        "stackoverflow.com",
    ),
    (
        "lucky ",
        "I'm Feeling Lucky",
        "🍀",
        "https://www.google.com/search?q={}&btnI=I",
        "google.com",
    ),
    (
        "image ",
        "Google Images",
        "🖼️",
        "https://www.google.com/search?q={}&tbm=isch",
        "images.google.com",
    ),
    (
        "bing ",
        "Bing",
        "🔵",
        "https://www.bing.com/search?q={}",
        "bing.com",
    ),
    (
        "yahoo ",
        "Yahoo",
        "💜",
        "https://search.yahoo.com/search?p={}",
        "search.yahoo.com",
    ),
];

pub struct WebSearchPlugin;

#[async_trait]
impl Plugin for WebSearchPlugin {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Search the web: g, yt, gh, youtube, wiki, maps, so, duckduckgo, github, gist, wolframalpha, bing, yahoo, image, lucky, netflix, gmail, drive, ytmusic, facebook, twitter, translate…"
    }

    fn keyword(&self) -> Option<&str> {
        None // handles multiple prefixes manually
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = &q.raw;
        let mut results = vec![];

        // Try each engine prefix
        for &(prefix, name, icon, url_tmpl, subtitle) in SEARCH_ENGINES {
            if let Some(term) = raw.strip_prefix(prefix) {
                let term = term.trim();
                if term.is_empty() {
                    continue;
                }
                let encoded = urlencoding(term);
                let url = url_tmpl.replace("{}", &encoded);
                results.push(QueryResult {
                    id: format!("{}:{}", prefix.trim(), term),
                    title: format!("Search {}: {}", name, term),
                    subtitle: Some(subtitle.to_string()),
                    icon: Some(icon.to_string()),
                    score: 90,
                    action_type: "url".to_string(),
                    action_data: url,
                });
                break; // first match wins
            }
        }

        // Fallback: bare query → Google (skip if any known prefix or other plugin prefix)
        if results.is_empty() && !raw.is_empty() && !is_other_plugin_prefix(raw) {
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
                "description": "Search the web using various engines",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "engine": {
                            "type": "string",
                            "enum": [
                                "google", "youtube", "github", "gist", "duckduckgo",
                                "bing", "yahoo", "wikipedia", "stackoverflow", "wolframalpha",
                                "google_images", "lucky", "maps", "netflix", "gmail",
                                "drive", "ytmusic", "facebook", "twitter", "translate"
                            ],
                            "description": "Search engine to use (default: google)"
                        }
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
            "gist" => format!("https://gist.github.com/search?q={}", encoded),
            "duckduckgo" => format!("https://duckduckgo.com/?q={}", encoded),
            "bing" => format!("https://www.bing.com/search?q={}", encoded),
            "yahoo" => format!("https://search.yahoo.com/search?p={}", encoded),
            "wikipedia" => format!("https://en.wikipedia.org/wiki/{}", encoded),
            "stackoverflow" => format!("https://stackoverflow.com/search?q={}", encoded),
            "wolframalpha" => format!("https://www.wolframalpha.com/input/?i={}", encoded),
            "google_images" => format!("https://www.google.com/search?q={}&tbm=isch", encoded),
            "lucky" => format!("https://www.google.com/search?q={}&btnI=I", encoded),
            "maps" => format!("https://maps.google.com/maps?q={}", encoded),
            "netflix" => format!("https://www.netflix.com/search?q={}", encoded),
            "gmail" => format!("https://mail.google.com/mail/ca/u/0/#apps/{}", encoded),
            "drive" => format!("https://drive.google.com/?hl=en&tab=bo#search/{}", encoded),
            "ytmusic" => format!("https://music.youtube.com/search?q={}", encoded),
            "facebook" => format!("https://www.facebook.com/search/?q={}", encoded),
            "twitter" => format!("https://twitter.com/search?q={}", encoded),
            "translate" => format!("https://translate.google.com/#auto|en|{}", encoded),
            _ => format!("https://www.google.com/search?q={}", encoded),
        }
    }
}

/// Returns true if the query starts with a prefix owned by another plugin,
/// so the bare-query Google fallback doesn't fire for those.
fn is_other_plugin_prefix(raw: &str) -> bool {
    raw.starts_with('>')        // bash_exec / shell_plugin
        || raw.starts_with('=')     // calculator
        || raw.starts_with("sys ")  // system_commands
        || raw.starts_with("f ")    // file_search
        || raw.starts_with("* ")    // file_search
        || raw.starts_with("bm ")   // browser_bookmarks
        || raw.starts_with("b ")    // browser_bookmarks
        || raw.starts_with("@")     // agent_delegate
        || raw.starts_with("cb ")   // clipboard
        || raw.starts_with("color ")// color_picker
        || raw.starts_with("env ")  // env_vars
        || raw.starts_with("git ")  // git
        || raw.starts_with("hosts ")// hosts
        || raw == "ip"
        || raw.starts_with("ip ")
        || raw.starts_with("net ")  // network
        || raw.starts_with("ps ")   // process_manager
        || raw.starts_with("snip ") // snippets
        || raw.starts_with("todo ") // todo
        || raw.starts_with("/todo ")// todo
        || raw.starts_with("/t ")   // todo
        || raw.starts_with("timer ")// timer
        || raw.starts_with("conv ") // unit_converter
        || raw.starts_with("settings ") // windows_settings
        || raw.starts_with("http://")   // url_opener
        || raw.starts_with("https://")  // url_opener
        || raw.starts_with("localhost:") // url_opener
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
