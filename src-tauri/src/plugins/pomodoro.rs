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

/// Per-state data for a Pomodoro timer phase — the "state" in the State pattern.
pub struct PomodoroPhase {
    pub duration_secs: u64,
    pub label: &'static str,
    pub icon: &'static str,
    pub done_title: &'static str,
    pub done_msg: &'static str,
}

/// All timer modes. Serializes as snake_case to stay compatible with existing
/// persisted JSON files (`"work"`, `"short_break"`, `"long_break"`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroMode {
    Work,
    ShortBreak,
    LongBreak,
}

impl PomodoroMode {
    /// Return the phase data for this mode — duration, labels, notification text.
    pub fn phase(&self) -> PomodoroPhase {
        match self {
            PomodoroMode::Work => PomodoroPhase {
                duration_secs: 25 * 60,
                label: "🍅 Work",
                icon: "🍅",
                done_title: "🍅 Pomodoro done!",
                done_msg: "Time for a break!",
            },
            PomodoroMode::ShortBreak => PomodoroPhase {
                duration_secs: 5 * 60,
                label: "☕ Short Break",
                icon: "☕",
                done_title: "☕ Break over!",
                done_msg: "Back to work!",
            },
            PomodoroMode::LongBreak => PomodoroPhase {
                duration_secs: 15 * 60,
                label: "🛋️ Long Break",
                icon: "🛋️",
                done_title: "🛋️ Long break over!",
                done_msg: "Ready to focus again?",
            },
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PomodoroState {
    mode: PomodoroMode,
    started_at: i64,
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
        let sp = state_path().to_string_lossy().to_string();

        // Build status result from current state (if any)
        let status_result = if let Some(ref s) = state {
            let phase = s.mode.phase();
            let elapsed = elapsed_secs(s.started_at).max(0) as u64;
            let done = elapsed >= s.duration_secs;
            let (icon, subtitle) = if done {
                (
                    "✅",
                    format!("{} — DONE! (session #{})", phase.label, s.session_count),
                )
            } else {
                (
                    "⏱️",
                    format!(
                        "{} — {} remaining (session #{})",
                        phase.label,
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
                source: None,
            })
        } else {
            None
        };

        let sub = raw.strip_prefix("pomo").unwrap_or("").trim();
        let mut results = vec![];

        if sub.is_empty() || sub.starts_with("st") {
            if let Some(sr) = status_result {
                results.push(sr);
            }
            if sub.is_empty() || "start".starts_with(sub) {
                let phase = PomodoroMode::Work.phase();
                let new_state = PomodoroState {
                    mode: PomodoroMode::Work,
                    started_at: now_secs(),
                    duration_secs: phase.duration_secs,
                    session_count: state.as_ref().map(|s| s.session_count + 1).unwrap_or(1),
                };
                save_state(&new_state);
                let cmd = timer_shell(phase.duration_secs, phase.done_title, phase.done_msg, &sp);
                results.push(QueryResult {
                    id: "pomo:start".to_string(),
                    title: format!("{} Start Pomodoro (25 min)", phase.icon),
                    subtitle: Some("Begin a focused work session".to_string()),
                    icon: Some(phase.icon.to_string()),
                    score: 90,
                    action_type: "shell_bg".to_string(),
                    action_data: cmd,
                    source: None,
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
                    source: None,
                });
            }
        }

        if sub.is_empty() || "short".starts_with(sub) {
            let phase = PomodoroMode::ShortBreak.phase();
            let cmd = timer_shell(phase.duration_secs, phase.done_title, phase.done_msg, &sp);
            results.push(QueryResult {
                id: "pomo:short".to_string(),
                title: format!("{} Short Break (5 min)", phase.icon),
                subtitle: Some("Quick breather".to_string()),
                icon: Some(phase.icon.to_string()),
                score: 85,
                action_type: "shell_bg".to_string(),
                action_data: cmd,
                source: None,
            });
        }

        if sub.is_empty() || "long".starts_with(sub) {
            let phase = PomodoroMode::LongBreak.phase();
            let cmd = timer_shell(phase.duration_secs, phase.done_title, phase.done_msg, &sp);
            results.push(QueryResult {
                id: "pomo:long".to_string(),
                title: format!("{} Long Break (15 min)", phase.icon),
                subtitle: Some("After 4 pomodoros".to_string()),
                icon: Some(phase.icon.to_string()),
                score: 82,
                action_type: "shell_bg".to_string(),
                action_data: cmd,
                source: None,
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
        let action = args["command"]
            .as_str()
            .or_else(|| args["action"].as_str())
            .unwrap_or("");
        match action {
            "stop" => {
                clear_state();
                "Pomodoro timer stopped".to_string()
            }
            "status" => {
                if let Some(s) = load_state() {
                    let phase = s.mode.phase();
                    let remaining = format_remaining(s.duration_secs, s.started_at);
                    format!(
                        "Mode: {}, Remaining: {}, Session: #{}",
                        phase.label, remaining, s.session_count
                    )
                } else {
                    "No active pomodoro".to_string()
                }
            }
            _ => "Unknown action".to_string(),
        }
    }
}

#[cfg(test)]
mod pomodoro_state_tests {
    use super::*;

    #[test]
    fn test_work_phase_duration() {
        assert_eq!(PomodoroMode::Work.phase().duration_secs, 25 * 60);
    }

    #[test]
    fn test_short_break_phase_duration() {
        assert_eq!(PomodoroMode::ShortBreak.phase().duration_secs, 5 * 60);
    }

    #[test]
    fn test_long_break_phase_duration() {
        assert_eq!(PomodoroMode::LongBreak.phase().duration_secs, 15 * 60);
    }

    #[test]
    fn test_mode_serde_round_trip() {
        let serialized = serde_json::to_string(&PomodoroMode::Work).unwrap();
        assert_eq!(serialized, "\"work\"");
        let deserialized: PomodoroMode = serde_json::from_str("\"work\"").unwrap();
        assert_eq!(deserialized, PomodoroMode::Work);

        let serialized = serde_json::to_string(&PomodoroMode::ShortBreak).unwrap();
        assert_eq!(serialized, "\"short_break\"");

        let serialized = serde_json::to_string(&PomodoroMode::LongBreak).unwrap();
        assert_eq!(serialized, "\"long_break\"");
    }

    #[test]
    fn test_work_phase_labels() {
        let p = PomodoroMode::Work.phase();
        assert_eq!(p.label, "🍅 Work");
        assert_eq!(p.icon, "🍅");
        assert!(p.done_title.contains("Pomodoro"));
    }

    #[test]
    fn test_short_break_phase_labels() {
        let p = PomodoroMode::ShortBreak.phase();
        assert_eq!(p.label, "☕ Short Break");
        assert_eq!(p.icon, "☕");
    }

    #[test]
    fn test_long_break_phase_labels() {
        let p = PomodoroMode::LongBreak.phase();
        assert_eq!(p.label, "🛋️ Long Break");
        assert_eq!(p.icon, "🛋️");
    }
}
