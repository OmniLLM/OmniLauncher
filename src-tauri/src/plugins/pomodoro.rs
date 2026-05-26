/// Pomodoro timer plugin — ported from Raycast `pomodoro` extension concept.
/// Commands:
///   pomo start       — start a 25-min work session
///   pomo short       — start a 5-min short break
///   pomo long        — start a 15-min long break
///   pomo status      — show current timer state
///   pomo stop        — cancel running timer
///
/// State is persisted to ~/.omnilauncher/pomodoro.json
use crate::path_config;
use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct PomodoroPlugin;

fn state_path() -> std::path::PathBuf {
    path_config::data_dir().join("pomodoro.json")
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PomodoroState {
    mode: String,    // "work" | "short_break" | "long_break" | "idle"
    started_at: i64, // unix seconds
    duration_secs: u64,
    session_count: u32,
}

fn load_state() -> Option<PomodoroState> {
    let path = state_path();
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

fn save_state(s: &PomodoroState) {
    if let Some(dir) = state_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(state_path(), json);
    }
}

fn clear_state() {
    let _ = std::fs::remove_file(state_path());
}

fn elapsed_secs(started_at: i64) -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    now - started_at
}

fn format_remaining(duration_secs: u64, started_at: i64) -> String {
    let elapsed = elapsed_secs(started_at).max(0) as u64;
    let remaining = duration_secs.saturating_sub(elapsed);
    let m = remaining / 60;
    let s = remaining % 60;
    format!("{:02}:{:02}", m, s)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn notify_script(title: &str, msg: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        format!(
            "powershell -Command \"Add-Type -AssemblyName System.Windows.Forms; \
             $n = [System.Windows.Forms.NotifyIcon]::new(); \
             $n.Icon = [System.Drawing.SystemIcons]::Information; \
             $n.Visible = $true; \
             $n.ShowBalloonTip(5000, '{}', '{}', 'Info'); \
             Start-Sleep 5; $n.Dispose()\"",
            title, msg
        )
    }
    #[cfg(target_os = "macos")]
    {
        format!(
            "osascript -e 'display notification \"{}\" with title \"{}\"'",
            msg, title
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        format!(
            "notify-send '{}' '{}' 2>/dev/null || echo '{}: {}'",
            title, msg, title, msg
        )
    }
}

/// Build a shell command that: sleeps for `secs`, fires notification, then writes idle state
fn timer_shell(secs: u64, done_title: &str, done_msg: &str, state_path: &str) -> String {
    let notify = notify_script(done_title, done_msg);
    #[cfg(target_os = "windows")]
    {
        format!(
            "powershell -Command \"Start-Sleep -Seconds {secs}; {notify}; \
             Remove-Item -Path '{state_path}' -ErrorAction SilentlyContinue\""
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        format!("sh -c 'sleep {secs}; {notify}; rm -f \"{state_path}\"'")
    }
}

#[async_trait]
impl Plugin for PomodoroPlugin {
    fn name(&self) -> &str {
        "pomodoro"
    }

    fn description(&self) -> &str {
        "Pomodoro timer — pomo start / short / long / status / stop"
    }

    fn keyword(&self) -> Option<&str> {
        Some("pomo")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();
        let state = load_state();

        // Status result always shown when prefix matches
        let status_result = if let Some(ref s) = state {
            let elapsed = elapsed_secs(s.started_at).max(0) as u64;
            let done = elapsed >= s.duration_secs;
            let mode_label = match s.mode.as_str() {
                "work" => "🍅 Work",
                "short_break" => "☕ Short Break",
                "long_break" => "🛋️ Long Break",
                _ => "⏸️ Idle",
            };
            let (icon, subtitle) = if done {
                (
                    "✅",
                    format!("{} — DONE! (session #{})", mode_label, s.session_count),
                )
            } else {
                (
                    "⏱️",
                    format!(
                        "{} — {} remaining (session #{})",
                        mode_label,
                        format_remaining(s.duration_secs, s.started_at),
                        s.session_count
                    ),
                )
            };
            Some(QueryResult {
                id: "pomo:status".to_string(),
                title: format!("{} Pomodoro Status", icon),
                subtitle: Some(subtitle),
                icon: Some(icon.to_string()),
                score: 95,
                action_type: "none".to_string(),
                action_data: String::new(),
            })
        } else {
            None
        };

        let sp = state_path().to_string_lossy().to_string();

        // Build action items based on subcommand
        let sub = raw.strip_prefix("pomo").unwrap_or("").trim();
        let mut results = vec![];

        if sub.is_empty() || sub.starts_with("st") {
            // show start / status / stop
            if let Some(sr) = status_result {
                results.push(sr);
            }
            if sub.is_empty() || "start".starts_with(sub) {
                let new_state = PomodoroState {
                    mode: "work".to_string(),
                    started_at: now_secs(),
                    duration_secs: 25 * 60,
                    session_count: state.as_ref().map(|s| s.session_count + 1).unwrap_or(1),
                };
                save_state(&new_state);
                let cmd = timer_shell(25 * 60, "🍅 Pomodoro done!", "Time for a break!", &sp);
                results.push(QueryResult {
                    id: "pomo:start".to_string(),
                    title: "🍅 Start Pomodoro (25 min)".to_string(),
                    subtitle: Some("Begin a focused work session".to_string()),
                    icon: Some("🍅".to_string()),
                    score: 90,
                    action_type: "shell_bg".to_string(),
                    action_data: cmd,
                });
            }
            if "stop".starts_with(sub) || sub == "stop" {
                results.push(QueryResult {
                    id: "pomo:stop".to_string(),
                    title: "⏹️ Stop Pomodoro".to_string(),
                    subtitle: Some("Cancel the current timer".to_string()),
                    icon: Some("⏹️".to_string()),
                    score: 80,
                    action_type: "callback".to_string(),
                    action_data: "pomo:stop".to_string(),
                });
            }
        }

        if sub.is_empty() || "short".starts_with(sub) {
            let cmd = timer_shell(5 * 60, "☕ Break over!", "Back to work!", &sp);
            results.push(QueryResult {
                id: "pomo:short".to_string(),
                title: "☕ Short Break (5 min)".to_string(),
                subtitle: Some("Quick breather".to_string()),
                icon: Some("☕".to_string()),
                score: 85,
                action_type: "shell_bg".to_string(),
                action_data: cmd,
            });
        }

        if sub.is_empty() || "long".starts_with(sub) {
            let cmd = timer_shell(15 * 60, "🛋️ Long break over!", "Ready to focus again?", &sp);
            results.push(QueryResult {
                id: "pomo:long".to_string(),
                title: "🛋️ Long Break (15 min)".to_string(),
                subtitle: Some("After 4 pomodoros".to_string()),
                icon: Some("🛋️".to_string()),
                score: 82,
                action_type: "shell_bg".to_string(),
                action_data: cmd,
            });
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "pomodoro",
                "description": "Control the Pomodoro timer: start a work session, short/long break, check status, or stop",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "'start' (25min work), 'short' (5min break), 'long' (15min break), 'status', or 'stop'" }
                    },
                    "required": ["command"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["command"].as_str().or_else(|| args["action"].as_str()).unwrap_or("");
        match action {
            "stop" => {
                clear_state();
                "Pomodoro timer stopped".to_string()
            }
            "status" => {
                if let Some(s) = load_state() {
                    let remaining = format_remaining(s.duration_secs, s.started_at);
                    format!(
                        "Mode: {}, Remaining: {}, Session: #{}",
                        s.mode, remaining, s.session_count
                    )
                } else {
                    "No active pomodoro".to_string()
                }
            }
            _ => "Unknown action".to_string(),
        }
    }
}
