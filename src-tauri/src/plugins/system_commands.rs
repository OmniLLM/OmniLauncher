use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct SystemCommandsPlugin;

#[async_trait]
impl Plugin for SystemCommandsPlugin {
    fn name(&self) -> &str {
        "system_commands"
    }

    fn description(&self) -> &str {
        "System commands: lock, sleep, shutdown, restart"
    }

    fn keyword(&self) -> Option<&str> {
        Some("sys ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("sys ")
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let commands = vec![
            ("lock", "🔒", "Lock screen", sys_lock_cmd()),
            ("sleep", "💤", "Sleep / suspend", sys_sleep_cmd()),
            ("shutdown", "⏻", "Shut down system", sys_shutdown_cmd()),
            ("restart", "🔄", "Restart system", sys_restart_cmd()),
        ];

        commands
            .into_iter()
            .filter(|(name, _, _, _)| term.is_empty() || name.contains(term.as_str()))
            .map(|(name, icon, desc, cmd)| QueryResult {
                id: format!("sys:{}", name),
                title: desc.to_string(),
                subtitle: Some(format!("sys {}", name)),
                icon: Some(icon.to_string()),
                score: 80,
                action_type: "shell".to_string(),
                action_data: cmd,
            })
            .collect()
    }
}

#[cfg(target_os = "linux")]
fn sys_lock_cmd() -> String {
    "loginctl lock-session".to_string()
}
#[cfg(target_os = "linux")]
fn sys_sleep_cmd() -> String {
    "systemctl suspend".to_string()
}
#[cfg(target_os = "linux")]
fn sys_shutdown_cmd() -> String {
    "systemctl poweroff".to_string()
}
#[cfg(target_os = "linux")]
fn sys_restart_cmd() -> String {
    "systemctl reboot".to_string()
}

#[cfg(target_os = "macos")]
fn sys_lock_cmd() -> String {
    "pmset displaysleepnow".to_string()
}
#[cfg(target_os = "macos")]
fn sys_sleep_cmd() -> String {
    "pmset sleepnow".to_string()
}
#[cfg(target_os = "macos")]
fn sys_shutdown_cmd() -> String {
    "osascript -e 'tell app \"System Events\" to shut down'".to_string()
}
#[cfg(target_os = "macos")]
fn sys_restart_cmd() -> String {
    "osascript -e 'tell app \"System Events\" to restart'".to_string()
}

#[cfg(target_os = "windows")]
fn sys_lock_cmd() -> String {
    "rundll32.exe user32.dll,LockWorkStation".to_string()
}
#[cfg(target_os = "windows")]
fn sys_sleep_cmd() -> String {
    "rundll32.exe powrprof.dll,SetSuspendState 0,1,0".to_string()
}
#[cfg(target_os = "windows")]
fn sys_shutdown_cmd() -> String {
    "shutdown /s /t 0".to_string()
}
#[cfg(target_os = "windows")]
fn sys_restart_cmd() -> String {
    "shutdown /r /t 0".to_string()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn sys_lock_cmd() -> String {
    "echo lock".to_string()
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn sys_sleep_cmd() -> String {
    "echo sleep".to_string()
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn sys_shutdown_cmd() -> String {
    "echo shutdown".to_string()
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn sys_restart_cmd() -> String {
    "echo restart".to_string()
}
