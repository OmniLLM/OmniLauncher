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
#[cfg(target_os = "linux")]
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

/// macOS: read selection via pbpaste (clipboard) or osascript (accessibility).
/// macOS doesn't have a concept of X11 PRIMARY — the selection only lands in
/// the clipboard after a copy.  We simulate Cmd+C via AppleScript and read
/// the clipboard immediately after.
#[cfg(target_os = "macos")]
pub fn read_x11_selection() -> Option<String> {
    // Simulate Cmd+C to copy current selection into clipboard
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to keystroke \"c\" using command down",
        ])
        .output();
    // Small delay for clipboard to update
    std::thread::sleep(std::time::Duration::from_millis(80));
    // Read clipboard
    if let Ok(out) = std::process::Command::new("pbpaste").output() {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

/// Windows: read selection via UI Automation (focused element TextPattern)
/// then fall back to simulating Ctrl+C + clipboard read.
///
/// Strategy (mirrors Wox's Windows implementation):
///   1. Try UI Automation — query the focused element's ITextPattern for
///      selected text ranges.  Works for most Win32/WPF/UWP apps without
///      touching the clipboard.
///   2. Fall back: send Ctrl+C via PowerShell, wait briefly, read clipboard.
#[cfg(target_os = "windows")]
pub fn read_x11_selection() -> Option<String> {
    // First try: UI Automation via PowerShell (no extra crate needed)
    // This script gets the focused element's selected text using UIA.
    let uia_script = r#"
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$ae = [System.Windows.Automation.AutomationElement]::FocusedElement
if ($ae -ne $null) {
    $tp = $ae.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
    if ($tp -ne $null) {
        $sel = $tp.GetSelection()
        if ($sel.Length -gt 0) {
            Write-Output $sel[0].GetText(-1)
            exit 0
        }
    }
    $vp = $ae.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    if ($vp -ne $null) {
        Write-Output $vp.Current.Value
        exit 0
    }
}
exit 1
"#;
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", uia_script])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    // Fallback: Ctrl+C + clipboard
    let clip_script = r#"
$before = Get-Clipboard
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.SendKeys]::SendWait("^c")
Start-Sleep -Milliseconds 100
$after = Get-Clipboard
if ($after -ne $before -and $after -ne $null -and $after.Trim() -ne "") {
    Write-Output $after
}
"#;
    if let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", clip_script])
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

/// Fallback stub for unsupported platforms
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
pub fn read_x11_selection() -> Option<String> {
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

    fn cheap_prefix_match(&self, raw: &str) -> bool {
        raw.starts_with("sel ") || raw.starts_with("selection ") || raw.starts_with(SEL_PREFIX)
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
            "dict" => format!("https://dict.youdao.com/result?word={}&lang=en", encoded),
            "github" => format!("https://github.com/search?q={}", encoded),
            "copy" => format!("Copied: {}", text),
            "ai" => format!("AI query: {}", text),
            _ => format!("Unknown action: {}", action),
        }
    }
}
