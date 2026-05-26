use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct ShowDesktopPlugin;

#[async_trait]
impl Plugin for ShowDesktopPlugin {
    fn name(&self) -> &str {
        "show_desktop"
    }

    fn description(&self) -> &str {
        "Show desktop — minimise all windows (Win+D)"
    }

    fn keyword(&self) -> Option<&str> {
        None // surfaced by search, not a keyword-gated plugin
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q.raw.trim().to_lowercase();
        let hits = [
            ("show desktop", "Show Desktop"),
            ("desktop", "Show Desktop"),
            ("win d", "Show Desktop"),
            ("minimize all", "Show Desktop"),
            ("minimise all", "Show Desktop"),
        ];

        let matched = hits
            .iter()
            .any(|(trigger, _)| trigger.contains(term.as_str()) || term.contains(trigger));

        if term.is_empty() || !matched {
            return vec![];
        }

        vec![QueryResult {
            id: "show_desktop:toggle".to_string(),
            title: "Show Desktop".to_string(),
            subtitle: Some("Minimise all windows (Win+D)".to_string()),
            icon: Some("🖥️".to_string()),
            score: 90,
            action_type: "shell".to_string(),
            action_data: show_desktop_cmd(),
        }]
    }

    async fn execute_tool(&self, _args: serde_json::Value) -> String {
        let cmd = show_desktop_cmd();
        run_show_desktop(&cmd)
    }
}

// ── Platform implementations ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn show_desktop_cmd() -> String {
    // PowerShell one-liner: send Win+D via the Shell COM object.
    // This is equivalent to pressing Win+D and is idempotent (toggles).
    r#"powershell -NoProfile -Command "(New-Object -ComObject Shell.Application).ToggleDesktop()""#
        .to_string()
}

#[cfg(target_os = "macos")]
fn show_desktop_cmd() -> String {
    // Mission Control "Show Desktop" gesture via AppleScript
    r#"osascript -e 'tell application "System Events" to key code 103 using {command down, mission control key}' 2>/dev/null || osascript -e 'tell application "Finder" to set collapsed of windows to true'"#.to_string()
}

#[cfg(target_os = "linux")]
fn show_desktop_cmd() -> String {
    // Try xdotool (X11) first; fall back to wmctrl, then qdbus (KDE).
    // We chain them with || so the first one that works wins.
    "xdotool key super+d 2>/dev/null || \
     wmctrl -k on 2>/dev/null || \
     qdbus org.kde.KWin /KWin showDesktop 2>/dev/null || \
     dbus-send --session --dest=org.gnome.Shell /org/gnome/Shell org.gnome.Shell.Eval string:'Main.overview.hide(); global.display.get_workspace_manager().get_active_workspace().list_windows().forEach(w => w.minimize());' 2>/dev/null"
        .to_string()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn show_desktop_cmd() -> String {
    "echo 'show desktop not supported on this platform'".to_string()
}

fn run_show_desktop(cmd: &str) -> String {
    #[cfg(target_os = "windows")]
    let output = std::process::Command::new("cmd")
        .args(["/C", cmd])
        .output();

    #[cfg(not(target_os = "windows"))]
    let output = std::process::Command::new("sh")
        .args(["-c", cmd])
        .output();

    match output {
        Ok(o) if o.status.success() => "Desktop shown".to_string(),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            if err.is_empty() {
                "Desktop shown".to_string()
            } else {
                format!("Error: {}", err.trim())
            }
        }
        Err(e) => format!("Failed to run command: {}", e),
    }
}
