/// Screenshot Plugin
///
/// Cross-platform screenshot support:
///
/// **Windows** (PowerToys-style):
///   - Area selection: triggers `ms-screenclip:` (Snip & Sketch overlay, same as Win+Shift+S)
///   - Full screen: PowerShell + System.Drawing BitBlt capture
///   - Lists recent screenshots from %USERPROFILE%\Pictures\Screenshots
///
/// **Linux**: scrot / gnome-screenshot / import (ImageMagick)
/// **macOS**: screencapture
///
/// Commands:
///   `ss`          — list recent screenshots
///   `ss new`      — take a new screenshot (area selection)
///   `ss full`     — take a full-screen screenshot
///   `ss <query>`  — search screenshots by filename (or OCR text if tesseract installed)
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use chrono::Local;
use dirs::picture_dir;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct ScreenshotPlugin;

fn screenshots_dir() -> PathBuf {
    picture_dir()
        .unwrap_or_else(|| PathBuf::from("~/Pictures"))
        .join("Screenshots")
}

// ─── Platform-specific capture ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn take_screenshot_fullscreen(path: &Path) -> Result<(), String> {
    // PowerShell: capture primary screen via System.Drawing BitBlt
    let ps = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms,System.Drawing;
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds;
$bmp = New-Object System.Drawing.Bitmap($b.Width, $b.Height);
$g = [System.Drawing.Graphics]::FromImage($bmp);
$g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size);
$bmp.Save('{path}');
$g.Dispose(); $bmp.Dispose()"#,
        path = path.to_str().unwrap_or("").replace('\'', "''")
    );
    let status = std::process::Command::new("powershell")
        .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() && path.exists() {
        Ok(())
    } else {
        Err("PowerShell screenshot failed".to_string())
    }
}

#[cfg(target_os = "windows")]
fn area_select_action(dir: &PathBuf) -> String {
    // Open Windows Snip & Sketch overlay (same as Win+Shift+S).
    // The user draws the snip; Windows saves it to clipboard + the Screenshots folder.
    // We also pass the dir so the label is informative.
    let _ = dir; // used for display only
    "powershell -WindowStyle Hidden -NoProfile -Command \"Start-Process 'ms-screenclip:'\""
        .to_string()
}

#[cfg(target_os = "windows")]
fn fullscreen_action(path: &Path) -> String {
    let ps = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $bmp=New-Object System.Drawing.Bitmap($b.Width,$b.Height); $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size); $bmp.Save('{}'); $g.Dispose(); $bmp.Dispose()"#,
        path.to_str().unwrap_or("").replace('\'', "''")
    );
    format!(
        "powershell -WindowStyle Hidden -NoProfile -Command \"{}\"",
        ps
    )
}

#[cfg(target_os = "windows")]
fn open_file_action(path: &Path) -> String {
    format!("explorer \"{}\"", path.display())
}

// ─── Linux ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn which_tool(tool: &str) -> bool {
    std::process::Command::new("which")
        .arg(tool)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn area_select_action(dir: &Path) -> String {
    let d = dir.display();
    if which_tool("scrot") {
        format!("mkdir -p {d} && scrot -s {d}/screenshot_$(date +%Y%m%d_%H%M%S).png")
    } else if which_tool("gnome-screenshot") {
        format!("mkdir -p {d} && gnome-screenshot -a -f {d}/screenshot_$(date +%Y%m%d_%H%M%S).png")
    } else {
        format!("mkdir -p {d} && import {d}/screenshot_$(date +%Y%m%d_%H%M%S).png")
    }
}

#[cfg(target_os = "linux")]
fn fullscreen_action(path: &Path) -> String {
    let p = path.display();
    if which_tool("scrot") {
        format!("scrot {p}")
    } else if which_tool("gnome-screenshot") {
        format!("gnome-screenshot -f {p}")
    } else {
        format!("import -window root {p}")
    }
}

#[cfg(target_os = "linux")]
fn open_file_action(path: &Path) -> String {
    format!("xdg-open \"{}\"", path.display())
}

// ─── macOS ────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn area_select_action(dir: &Path) -> String {
    format!(
        "mkdir -p {d} && screencapture -i {d}/screenshot_$(date +%Y%m%d_%H%M%S).png",
        d = dir.display()
    )
}

#[cfg(target_os = "macos")]
fn fullscreen_action(path: &Path) -> String {
    format!("screencapture {}", path.display())
}

#[cfg(target_os = "macos")]
fn open_file_action(path: &Path) -> String {
    format!("open \"{}\"", path.display())
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

fn ocr_text(path: &Path) -> Option<String> {
    // Only attempt if tesseract is available
    let ok = std::process::Command::new("tesseract")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let out = std::process::Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .arg("-l")
        .arg("eng+chi_sim")
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
        let mt =
            |e: &walkdir::DirEntry| std::fs::metadata(e.path()).and_then(|m| m.modified()).ok();
        mt(b).cmp(&mt(a))
    });
    files
        .into_iter()
        .take(limit)
        .map(|e| e.path().to_path_buf())
        .collect()
}

fn file_age_label(path: &Path) -> String {
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

fn new_screenshot_path() -> PathBuf {
    screenshots_dir().join(format!(
        "screenshot_{}.png",
        Local::now().format("%Y%m%d_%H%M%S")
    ))
}

// ─── Plugin impl ──────────────────────────────────────────────────────────────

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

    fn cheap_prefix_match(&self, raw: &str) -> bool {
        let r = raw.trim().to_lowercase();
        r == "ss"
            || r == "screenshot"
            || r == "截图"
            || r.starts_with("ss ")
            || r.starts_with("screenshot ")
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

        let dir = screenshots_dir();
        let _ = std::fs::create_dir_all(&dir);

        // "ss new" → area selection
        if filter == "new" || filter == "新建" || filter == "area" {
            return vec![QueryResult {
                id: "screenshot:new".to_string(),
                title: "📸 Take Screenshot (Area)".to_string(),
                subtitle: Some("Select area to capture".to_string()),
                icon: Some("📸".to_string()),
                score: 100,
                action_type: "shell".to_string(),
                action_data: area_select_action(&dir),
                source: None,
            }];
        }

        // "ss full" → full-screen capture
        if filter == "full" || filter == "全屏" {
            let path = new_screenshot_path();
            return vec![QueryResult {
                id: "screenshot:full".to_string(),
                title: "🖥 Take Full-Screen Screenshot".to_string(),
                subtitle: Some(format!("Saves to {}", path.display())),
                icon: Some("🖥".to_string()),
                score: 100,
                action_type: "shell".to_string(),
                action_data: fullscreen_action(&path),
                source: None,
            }];
        }

        let mut results = vec![];

        // Top item: area selection
        if filter.is_empty() {
            results.push(QueryResult {
                id: "screenshot:new".to_string(),
                title: "📸 Take Screenshot (Area)".to_string(),
                subtitle: Some("Select area to capture → clipboard + file".to_string()),
                icon: Some("📸".to_string()),
                score: 95,
                action_type: "shell".to_string(),
                action_data: area_select_action(&dir),
                source: None,
            });
            // Second item: full-screen
            let path = new_screenshot_path();
            results.push(QueryResult {
                id: "screenshot:full".to_string(),
                title: "🖥 Take Full-Screen Screenshot".to_string(),
                subtitle: Some(format!("Saves to {}", dir.display())),
                icon: Some("🖥".to_string()),
                score: 90,
                action_type: "shell".to_string(),
                action_data: fullscreen_action(&path),
                source: None,
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

            if !filter.is_empty() {
                let matches_name = filename.to_lowercase().contains(&filter);
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
                action_data: open_file_action(&path),
                source: None,
            });
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "take_screenshot",
                "description": "Take a screenshot. On Windows uses Snip & Sketch (area) or PowerShell (fullscreen). On Linux uses scrot/gnome-screenshot.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "mode": {
                            "type": "string",
                            "enum": ["fullscreen", "selection"],
                            "description": "fullscreen = capture entire screen; selection = interactive area picker"
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

        if mode == "selection" {
            // For selection mode, fire off the OS snip tool and return immediately
            let action = area_select_action(&dir);
            let result = if cfg!(target_os = "windows") {
                std::process::Command::new("powershell")
                    .args([
                        "-WindowStyle",
                        "Hidden",
                        "-NoProfile",
                        "-Command",
                        "Start-Process 'ms-screenclip:'",
                    ])
                    .status()
            } else {
                std::process::Command::new("sh")
                    .arg("-c")
                    .arg(&action)
                    .status()
            };
            return match result {
                Ok(_) => "Snip overlay opened — draw your selection".to_string(),
                Err(e) => format!("Error: {}", e),
            };
        }

        // Fullscreen capture
        let path = new_screenshot_path();

        #[cfg(target_os = "windows")]
        {
            let ps = format!(
                r#"Add-Type -AssemblyName System.Windows.Forms,System.Drawing; $b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $bmp=New-Object System.Drawing.Bitmap($b.Width,$b.Height); $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size); $bmp.Save('{}'); $g.Dispose(); $bmp.Dispose()"#,
                path.to_str().unwrap_or("").replace('\'', "''")
            );
            let result = std::process::Command::new("powershell")
                .args(["-WindowStyle", "Hidden", "-NoProfile", "-Command", &ps])
                .status();
            return match result {
                Ok(s) if s.success() => format!("Screenshot saved to {}", path.display()),
                Ok(_) => "Screenshot failed".to_string(),
                Err(e) => format!("Error: {}", e),
            };
        }

        #[cfg(not(target_os = "windows"))]
        {
            let action = fullscreen_action(&path);
            let result = std::process::Command::new("sh")
                .arg("-c")
                .arg(&action)
                .status();
            match result {
                Ok(s) if s.success() && path.exists() => {
                    format!("Screenshot saved to {}", path.display())
                }
                Ok(_) => "Screenshot failed or was cancelled".to_string(),
                Err(e) => format!("Error: {}", e),
            }
        }
    }
}
