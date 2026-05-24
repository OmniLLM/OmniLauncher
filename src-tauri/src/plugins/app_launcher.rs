use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

pub struct AppLauncherPlugin {
    pub apps: Vec<AppEntry>,
}

#[derive(Clone)]
pub struct AppEntry {
    pub name: String,
    pub exec: String,
    pub icon: Option<String>,
}

impl Default for AppLauncherPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl AppLauncherPlugin {
    pub fn new() -> Self {
        let apps = Self::load_apps();
        Self { apps }
    }

    #[cfg(target_os = "linux")]
    fn load_apps() -> Vec<AppEntry> {
        let mut apps = vec![];
        let dirs = vec![
            PathBuf::from("/usr/share/applications"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".local/share/applications"),
        ];
        for dir in dirs {
            if !dir.exists() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                        if let Some(app) = parse_desktop_file(&path) {
                            apps.push(app);
                        }
                    }
                }
            }
        }
        apps
    }

    #[cfg(target_os = "macos")]
    fn load_apps() -> Vec<AppEntry> {
        let mut apps = vec![];
        let app_dirs = vec![PathBuf::from("/Applications")];
        for dir in app_dirs {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.extension().map(|e| e == "app").unwrap_or(false) {
                        let name = path
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        apps.push(AppEntry {
                            name,
                            exec: path.to_string_lossy().to_string(),
                            icon: Some("🖥️".to_string()),
                        });
                    }
                }
            }
        }
        apps
    }

    #[cfg(target_os = "windows")]
    fn load_apps() -> Vec<AppEntry> {
        // Scan Start Menu for .lnk files
        let mut apps = vec![];
        let paths = vec![
            PathBuf::from(std::env::var("PROGRAMDATA").unwrap_or_default())
                .join("Microsoft\\Windows\\Start Menu\\Programs"),
            PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
                .join("Microsoft\\Windows\\Start Menu\\Programs"),
        ];
        for dir in paths {
            let walker =
                walkdir::WalkDir::new(&dir)
                    .into_iter()
                    .try_fold(vec![], |mut acc, e| {
                        if let Ok(e) = e {
                            if e.path().extension().map(|x| x == "lnk").unwrap_or(false) {
                                let name = e
                                    .path()
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
                                acc.push(AppEntry {
                                    name,
                                    exec: e.path().to_string_lossy().to_string(),
                                    icon: Some("🪟".to_string()),
                                });
                            }
                        }
                        Ok::<_, std::convert::Infallible>(acc)
                    })
                    .unwrap_or_default();
            apps.extend(walker);
        }
        apps
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    fn load_apps() -> Vec<AppEntry> {
        vec![]
    }
}

#[cfg(target_os = "linux")]
fn parse_desktop_file(path: &PathBuf) -> Option<AppEntry> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut name = None;
    let mut exec = None;
    let mut no_display = false;
    let mut in_desktop_entry = false;

    for line in content.lines() {
        if line.trim() == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }
        if line.starts_with('[') && line != "[Desktop Entry]" {
            in_desktop_entry = false;
        }
        if !in_desktop_entry {
            continue;
        }
        if let Some(v) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(v.trim().to_string());
            }
        } else if let Some(v) = line.strip_prefix("Exec=") {
            if exec.is_none() {
                // Remove field codes like %u %f %F etc
                let e = v
                    .split_whitespace()
                    .filter(|t| !t.starts_with('%'))
                    .collect::<Vec<_>>()
                    .join(" ");
                exec = Some(e);
            }
        } else if line
            .strip_prefix("NoDisplay=")
            .map(|v| v.trim().to_lowercase() == "true")
            .unwrap_or(false)
        {
            no_display = true;
        }
    }

    if no_display {
        return None;
    }

    Some(AppEntry {
        name: name?,
        exec: exec?,
        icon: Some("🚀".to_string()),
    })
}

#[async_trait]
impl Plugin for AppLauncherPlugin {
    fn name(&self) -> &str {
        "app_launcher"
    }

    fn description(&self) -> &str {
        "Launch installed applications"
    }

    fn keyword(&self) -> Option<&str> {
        None // No prefix — searches all app names
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        if q.raw.is_empty()
            || q.raw.starts_with("= ")
            || q.raw.starts_with('>')
            || q.raw.starts_with("sys ")
        {
            return vec![];
        }

        let term = q.raw.to_lowercase();
        let mut results = vec![];

        for app in &self.apps {
            let name_lower = app.name.to_lowercase();
            if name_lower.starts_with(&term) {
                results.push(QueryResult {
                    id: format!("app:{}", app.name),
                    title: app.name.clone(),
                    subtitle: Some(app.exec.clone()),
                    icon: app.icon.clone(),
                    score: 85,
                    action_type: "open_app".to_string(),
                    action_data: app.exec.clone(),
                });
            } else if name_lower.contains(&term) {
                results.push(QueryResult {
                    id: format!("app:{}", app.name),
                    title: app.name.clone(),
                    subtitle: Some(app.exec.clone()),
                    icon: app.icon.clone(),
                    score: 60,
                    action_type: "open_app".to_string(),
                    action_data: app.exec.clone(),
                });
            }
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "app_launcher",
                "description": "Launch an installed application by name. Searches Start Menu shortcuts (Windows), .app bundles (macOS), or .desktop files (Linux) and executes the best match. Use this to open apps like VS Code, Chrome, Notepad, etc.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Application name to search for and launch (e.g. 'code', 'chrome', 'notepad', 'vscode insider')" }
                    },
                    "required": ["name"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let name = args["name"].as_str().unwrap_or("");
        if name.is_empty() {
            return "Error: no application name provided".to_string();
        }

        let term = name.to_lowercase();
        let mut best: Option<&AppEntry> = None;
        let mut best_score = 0;

        for app in &self.apps {
            let app_lower = app.name.to_lowercase();
            if app_lower == term {
                best = Some(app);
                break;
            } else if app_lower.starts_with(&term) && best_score < 2 {
                best = Some(app);
                best_score = 2;
            } else if app_lower.contains(&term) && best_score < 1 {
                best = Some(app);
                best_score = 1;
            }
        }

        if let Some(app) = best {
            let app_name = app.name.clone();
            launch_app(&app.exec);
            format!("Launched: {}", app_name)
        } else {
            format!("No application found matching: '{}'", name)
        }
    }
}

/// Launch an application — mimics Flow.Launcher's approach:
/// ProcessStartInfo { FileName = path, UseShellExecute = true }
/// In Rust, the equivalent is `cmd /c start "" "path"` which calls ShellExecuteEx
/// to resolve .lnk shortcuts and launch the target with proper working directory.
fn launch_app(exec: &str) {
    #[cfg(target_os = "windows")]
    {
        // Flow.Launcher uses ProcessStartInfo with UseShellExecute=true on the .lnk path.
        // The Rust equivalent: spawn cmd with `start` which calls ShellExecuteEx.
        // The empty "" is required as window title when path has spaces.
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", exec])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW - don't flash a cmd window
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(exec)
            .spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("sh")
            .args(["-c", exec])
            .spawn();
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = std::process::Command::new("sh")
            .args(["-c", exec])
            .spawn();
    }
}
