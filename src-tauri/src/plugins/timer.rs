use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use regex::Regex;

pub struct TimerPlugin;

#[async_trait]
impl Plugin for TimerPlugin {
    fn name(&self) -> &str {
        "timer"
    }

    fn description(&self) -> &str {
        "Set a timer with duration (e.g. timer 5m, timer 30s, timer 1h)"
    }

    fn keyword(&self) -> Option<&str> {
        Some("timer ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.strip_prefix("timer ").unwrap_or("").trim();
        if raw.is_empty() {
            return vec![];
        }

        if let Some(seconds) = parse_duration(raw) {
            let label = if seconds == 1 { "second" } else { "seconds" };
            let display = raw.to_string();
            // Build a more readable label
            let display_str = format_duration(seconds);
            let shell_cmd = format!(
                "powershell -Command \"Start-Sleep -Seconds {}; [System.Media.SystemSounds]::Beep.Play(); Write-Host 'Timer done!'\"",
                seconds
            );

            return vec![QueryResult {
                id: format!("timer:{}", seconds),
                title: format!("Timer: {} ({})", display_str, display),
                subtitle: Some(format!(
                    "Press Enter to start a {}-{} timer",
                    seconds, label
                )),
                icon: Some("⏰".to_string()),
                score: 100,
                action_type: "shell".to_string(),
                action_data: shell_cmd,
            }];
        }

        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "set_timer",
                "description": "Set a countdown timer for a specified duration",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "duration_seconds": {
                            "type": "integer",
                            "description": "Duration of the timer in seconds"
                        },
                        "label": {
                            "type": "string",
                            "description": "Optional label for the timer"
                        }
                    },
                    "required": ["duration_seconds"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let seconds = args["duration_seconds"].as_i64().unwrap_or(0);
        if seconds <= 0 {
            return "Invalid duration".to_string();
        }
        format!("Timer set for {} seconds", seconds)
    }
}

/// Parse a duration string like "5m", "30s", "1h", "90s"
fn parse_duration(input: &str) -> Option<u64> {
    let re = Regex::new(r"^(\d+)\s*(s|sec|second|seconds|m|min|minute|minutes|h|hr|hour|hours)$")
        .ok()?;
    if let Some(caps) = re.captures(input.trim()) {
        let value: u64 = caps.get(1)?.as_str().parse().ok()?;
        let unit = caps.get(2)?.as_str().to_lowercase();
        match unit.as_str() {
            "s" | "sec" | "second" | "seconds" => Some(value),
            "m" | "min" | "minute" | "minutes" => Some(value * 60),
            "h" | "hr" | "hour" | "hours" => Some(value * 3600),
            _ => None,
        }
    } else {
        // Try parsing as raw seconds
        input.trim().parse::<u64>().ok()
    }
}

/// Format seconds into a human-readable string like "5 minutes" or "1 hour 30 minutes"
fn format_duration(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    let mut parts = vec![];
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{}s", seconds));
    }
    parts.join(" ")
}
