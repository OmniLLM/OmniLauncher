/// Screenshot Plugin
///
/// Inspired by Wox (GPL-3.0) — take screenshots and search screenshot history.
/// Uses system tools: `scrot` / `gnome-screenshot` / `import` on Linux,
/// `screencapture` on macOS.  OCR via `tesseract` if installed.
///
/// Commands:
///   `ss`          — list recent screenshots
///   `ss new`      — take a new screenshot (saves to ~/Pictures/Screenshots/)
///   `ss <query>`  — search screenshots by OCR text
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use chrono::Local;
use dirs::picture_dir;
use std::path::PathBuf;
use walkdir::WalkDir;

pub struct ScreenshotPlugin;

fn screenshots_dir() -> PathBuf {
    picture_dir()
        .unwrap_or_else(|| PathBuf::from("~/Pictures"))
        .join("Screenshots")
}

fn take_screenshot() -> Result<PathBuf, String> {
    let dir = screenshots_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let filename = format!("screenshot_{}.png", Local::now().format("%Y%m%d_%H%M%S"));
    let path = dir.join(&filename);

    // Try tools in order of preference
    let tools: &[(&str, Vec<&str>)] = &[
        ("scrot", vec!["-s", path.to_str().unwrap_or("")]),
        (
            "gnome-screenshot",
            vec!["-a", "-f", path.to_str().unwrap_or("")],
        ),
        (
            "import",
            vec!["-window", "root", path.to_str().unwrap_or("")],
        ),
        (
            "screencapture",
            vec!["-i", path.to_str().unwrap_or("")],
        ), // macOS
    ];

    for (tool, args) in tools {
        if which_tool(tool) {
            let status = std::process::Command::new(tool)
                .args(args)
                .status()
                .map_err(|e| e.to_string())?;
            if status.success() && path.exists() {
                return Ok(path);
            }
        }
    }

    Err("No screenshot tool found. Install scrot: sudo apt install scrot".to_string())
}

fn which_tool(tool: &str) -> bool {
    std::process::Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ocr_text(path: &PathBuf) -> Option<String> {
    if !which_tool("tesseract") {
        return None;
    }
    let out = std::process::Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .arg("-l")
        .arg("eng+chi_sim") // English + Simplified Chinese
        .output()
        .ok()?;
    if out.status.success() {
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        if text.trim().len() > 3 {
            return Some(text);
        }
    }
    None
}

fn list_screenshots(limit: usize) -> Vec<PathBuf> {
    let dir = screenshots_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut files: Vec<_> = WalkDir::new(&dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|ext| {
                        ext.eq_ignore_ascii_case("png")
                            || ext.eq_ignore_ascii_case("jpg")
                            || ext.eq_ignore_ascii_case("jpeg")
                    })
                    .unwrap_or(false)
        })
        .collect();
    files.sort_by(|a, b| {
        let mt = |e: &walkdir::DirEntry| {
            std::fs::metadata(e.path())
                .and_then(|m| m.modified())
                .ok()
        };
        mt(b).cmp(&mt(a))
    });
    files
        .into_iter()
        .take(limit)
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn file_age_label(path: &PathBuf) -> String {
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                let secs = elapsed.as_secs();
                if secs < 60 {
                    return format!("{}s ago", secs);
                } else if secs < 3600 {
                    return format!("{}m ago", secs / 60);
                } else if secs < 86400 {
                    return format!("{}h ago", secs / 3600);
                } else {
                    return format!("{}d ago", secs / 86400);
                }
            }
        }
    }
    String::new()
}

#[async_trait]
impl Plugin for ScreenshotPlugin {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Take screenshots and search screenshot history (type 'ss' or 'screenshot')"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim().to_lowercase();

        let (command, filter) = if let Some(rest) = raw
            .strip_prefix("screenshot ")
            .or_else(|| raw.strip_prefix("ss "))
        {
            (true, rest.trim().to_string())
        } else if raw == "ss" || raw == "screenshot" || raw == "截图" {
            (true, String::new())
        } else {
            return vec![];
        };

        if !command {
            return vec![];
        }

        // "ss new" → take a screenshot
        if filter == "new" || filter == "新建" {
            return vec![QueryResult {
                id: "screenshot:new".to_string(),
                title: "📸 Take New Screenshot".to_string(),
                subtitle: Some("Select area to capture".to_string()),
                icon: Some("📸".to_string()),
                score: 100,
                action_type: "shell".to_string(),
                action_data: format!(
                    "mkdir -p {} && scrot -s {}/screenshot_$(date +%Y%m%d_%H%M%S).png",
                    screenshots_dir().display(),
                    screenshots_dir().display()
                ),
            }];
        }

        let mut results = vec![];

        // Always offer "take new screenshot" as first item when no filter
        if filter.is_empty() {
            results.push(QueryResult {
                id: "screenshot:new".to_string(),
                title: "📸 Take New Screenshot".to_string(),
                subtitle: Some(format!("Saves to {}", screenshots_dir().display())),
                icon: Some("📸".to_string()),
                score: 90,
                action_type: "shell".to_string(),
                action_data: format!(
                    "mkdir -p {d} && scrot -s {d}/screenshot_$(date +%Y%m%d_%H%M%S).png",
                    d = screenshots_dir().display()
                ),
            });
        }

        // List recent screenshots
        for path in list_screenshots(20) {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let age = file_age_label(&path);

            // Filter by filename or OCR text
            if !filter.is_empty() {
                let matches_name = filename.to_lowercase().contains(&filter);
                // For OCR search, only run if filename doesn't already match
                if !matches_name {
                    let ocr = ocr_text(&path).unwrap_or_default();
                    if !ocr.to_lowercase().contains(&filter) {
                        continue;
                    }
                }
            }

            let subtitle = if filter.is_empty() {
                age.clone()
            } else {
                format!("{} · {}", age, path.display())
            };

            results.push(QueryResult {
                id: format!("screenshot:{}", path.display()),
                title: format!("🖼 {}", filename),
                subtitle: Some(subtitle),
                icon: Some("🖼".to_string()),
                score: if filter.is_empty() { 70 } else { 80 },
                action_type: "shell".to_string(),
                action_data: format!("xdg-open {}", path.display()),
            });
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "take_screenshot",
                "description": "Take a screenshot of the screen or a selected area",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["fullscreen", "selection"],
                            "description": "Capture mode: fullscreen or interactive selection"
                        }
                    },
                    "required": []
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let mode = args["mode"].as_str().unwrap_or("selection");
        let dir = screenshots_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!(
            "screenshot_{}.png",
            Local::now().format("%Y%m%d_%H%M%S")
        ));

        let result = if mode == "fullscreen" {
            std::process::Command::new("scrot")
                .arg(path.to_str().unwrap_or(""))
                .status()
        } else {
            std::process::Command::new("scrot")
                .arg("-s")
                .arg(path.to_str().unwrap_or(""))
                .status()
        };

        match result {
            Ok(s) if s.success() => format!("Screenshot saved to {}", path.display()),
            Ok(_) => "Screenshot failed or was cancelled".to_string(),
            Err(e) => format!("Error: {} — install scrot: sudo apt install scrot", e),
        }
    }
}
