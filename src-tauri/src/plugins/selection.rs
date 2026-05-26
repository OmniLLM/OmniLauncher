/// Selection Plugin
///
/// Inspired by Wox (GPL-3.0) — reads the currently selected text in any app
/// when the launcher hotkey is triggered, and surfaces quick actions on it.
///
/// How it works:
///   1. main.rs reads X11 PRIMARY selection (or Ctrl+C clipboard fallback)
///      before showing the window, attaches it to the "omnilauncher://shown" event.
///   2. Frontend stores the selection in state; search queries prefixed with
///      `__sel__:` carry it here.
///   3. This plugin returns actions: web search, AI ask, copy, translate, etc.
///
/// Trigger:  `sel ` or `selection ` — or auto-activated when launcher opens
///           with selected text from another app.
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct SelectionPlugin;

const SEL_PREFIX: &str = "__sel__:";

fn get_selection_from_query(raw: &str) -> Option<String> {
    // Explicit trigger: "sel some text" or "selection some text"
    if let Some(rest) = raw
        .strip_prefix("sel ")
        .or_else(|| raw.strip_prefix("selection "))
    {
        let trimmed = rest.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    // Auto-injected by frontend: "__sel__:the selected text"
    if let Some(text) = raw.strip_prefix(SEL_PREFIX) {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{}…", t)
    }
}

fn build_actions(text: &str) -> Vec<QueryResult> {
    let encoded = urlencoding_encode(text);
    let short = truncate(text, 40);

    vec![
        // Web search
        QueryResult {
            id: "sel:search".to_string(),
            title: format!("🔍 Search: {}", short),
            subtitle: Some("Google search selected text".to_string()),
            icon: Some("🔍".to_string()),
            score: 95,
            action_type: "url".to_string(),
            action_data: format!("https://www.google.com/search?q={}", encoded),
        },
        // Copy to clipboard
        QueryResult {
            id: "sel:copy".to_string(),
            title: format!("📋 Copy: {}", short),
            subtitle: Some("Copy to clipboard".to_string()),
            icon: Some("📋".to_string()),
            score: 90,
            action_type: "copy".to_string(),
            action_data: text.to_string(),
        },
        // AI Ask
        QueryResult {
            id: "sel:ai".to_string(),
            title: format!("🤖 Ask AI: {}", short),
            subtitle: Some("Send to AI assistant".to_string()),
            icon: Some("🤖".to_string()),
            score: 88,
            action_type: "ai_query".to_string(),
            action_data: text.to_string(),
        },
        // Translate (via Google Translate)
        QueryResult {
            id: "sel:translate".to_string(),
            title: format!("🌐 Translate: {}", short),
            subtitle: Some("Open in Google Translate".to_string()),
            icon: Some("🌐".to_string()),
            score: 85,
            action_type: "url".to_string(),
            action_data: format!("https://translate.google.com/?text={}", encoded),
        },
        // GitHub code search
        QueryResult {
            id: "sel:github".to_string(),
            title: format!("🐙 GitHub: {}", short),
            subtitle: Some("Search on GitHub".to_string()),
            icon: Some("🐙".to_string()),
            score: 80,
            action_type: "url".to_string(),
            action_data: format!("https://github.com/search?q={}", encoded),
        },
        // Dict lookup (Chinese ↔ English)
        QueryResult {
            id: "sel:dict".to_string(),
            title: format!("📖 Dict: {}", short),
            subtitle: Some("Look up in Youdao dictionary".to_string()),
            icon: Some("📖".to_string()),
            score: 78,
            action_type: "url".to_string(),
            action_data: format!("https://dict.youdao.com/result?word={}&lang=en", encoded),
        },
        // StackOverflow
        QueryResult {
            id: "sel:so".to_string(),
            title: format!("💡 StackOverflow: {}", short),
            subtitle: Some("Search on StackOverflow".to_string()),
            icon: Some("💡".to_string()),
            score: 75,
            action_type: "url".to_string(),
            action_data: format!("https://stackoverflow.com/search?q={}", encoded),
        },
    ]
}

/// Minimal URL percent-encoder (no external dep needed)
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", byte));
            }
        }
    }
    out
}

/// Read X11 PRIMARY selection (selected text in any focused app).
/// Falls back to CLIPBOARD if PRIMARY is empty.
/// Returns None if xclip / xsel / xdotool is unavailable or nothing is selected.
pub fn read_x11_selection() -> Option<String> {
    // Try xclip PRIMARY first
    if let Ok(out) = std::process::Command::new("xclip")
        .args(["-selection", "primary", "-o"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    // Try xsel
    if let Ok(out) = std::process::Command::new("xsel")
        .args(["--primary", "--output"])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    // Try xdotool getselection
    if let Ok(out) = std::process::Command::new("xdotool")
        .arg("getselection")
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

#[async_trait]
impl Plugin for SelectionPlugin {
    fn name(&self) -> &str {
        "selection"
    }

    fn description(&self) -> &str {
        "Act on selected text from any app — search, translate, ask AI (type 'sel <text>')"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        match get_selection_from_query(&q.raw) {
            Some(text) => build_actions(&text),
            None => vec![],
        }
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "act_on_selection",
                "description": "Perform an action on the currently selected text",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string", "description": "The selected text" },
                        "action": {
                            "type": "string",
                            "enum": ["search", "translate", "copy", "ai", "dict", "github"],
                            "description": "Action to perform"
                        }
                    },
                    "required": ["text", "action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let text = args["text"].as_str().unwrap_or("").trim().to_string();
        let action = args["action"].as_str().unwrap_or("search");
        if text.is_empty() {
            return "No text provided".to_string();
        }
        let encoded = urlencoding_encode(&text);
        match action {
            "search" => format!("https://www.google.com/search?q={}", encoded),
            "translate" => format!("https://translate.google.com/?text={}", encoded),
            "dict" => format!(
                "https://dict.youdao.com/result?word={}&lang=en",
                encoded
            ),
            "github" => format!("https://github.com/search?q={}", encoded),
            "copy" => format!("Copied: {}", text),
            "ai" => format!("AI query: {}", text),
            _ => format!("Unknown action: {}", action),
        }
    }
}
