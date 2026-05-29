use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::cmp::Reverse;
use sysinfo::System;

pub struct ProcessManagerPlugin;

#[async_trait]
impl Plugin for ProcessManagerPlugin {
    fn name(&self) -> &str {
        "process_manager"
    }

    fn description(&self) -> &str {
        "List and kill processes by name"
    }

    fn keyword(&self) -> Option<&str> {
        Some("ps ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.strip_prefix("ps ").unwrap_or("").trim();
        if raw.is_empty() {
            // List top 10 processes by memory
            let mut system = System::new_all();
            system.refresh_all();
            let mut processes: Vec<_> = system
                .processes()
                .iter()
                .map(|(pid, proc)| {
                    (
                        pid.as_u32(),
                        proc.name().to_string(),
                        proc.memory(),
                        proc.cpu_usage(),
                    )
                })
                .collect();
            processes.sort_by_key(|b| Reverse(b.2));
            processes.truncate(10);

            return processes
                .into_iter()
                .enumerate()
                .map(|(i, (pid, name, mem, cpu))| {
                    let mem_mb = mem as f64 / 1024.0;
                    QueryResult {
                        id: format!("ps:{}", i),
                        title: format!("{} (PID: {})", name, pid),
                        subtitle: Some(format!("Memory: {:.1} MB | CPU: {:.1}%", mem_mb, cpu)),
                        icon: Some("⚙️".to_string()),
                        score: 100 - i as i32,
                        action_type: "copy".to_string(),
                        action_data: format!(
                            "{} (PID: {}) — {:.1} MB, {:.1}% CPU",
                            name, pid, mem_mb, cpu
                        ),
                    }
                })
                .collect();
        }

        if let Some(name) = raw.strip_prefix("kill ") {
            // On Windows, process names include the .exe suffix; append it if missing.
            let name_owned;
            let name = {
                let n = name.trim();
                if cfg!(target_os = "windows") && !n.ends_with(".exe") {
                    name_owned = format!("{}.exe", n);
                    name_owned.as_str()
                } else {
                    n
                }
            };
            let mut system = System::new_all();
            system.refresh_all();
            let mut killed: Vec<String> = Vec::new();
            let mut failed: Vec<String> = Vec::new();
            for proc in system.processes_by_name(name.as_ref()) {
                let pname = proc.name().to_string();
                let pid = proc.pid().as_u32();
                let entry = format!("{} (PID: {})", pname, pid);
                if proc.kill() {
                    killed.push(entry);
                } else {
                    failed.push(entry);
                }
            }

            if killed.is_empty() {
                let (title, subtitle, data) = if failed.is_empty() {
                    (
                        format!("No process found: {}", name),
                        "No matching processes to kill".to_string(),
                        format!("No process '{}' found", name),
                    )
                } else {
                    (
                        format!("Failed to kill {} process(es)", failed.len()),
                        format!("Could not kill: {}", failed.join(", ")),
                        format!("Failed to kill: {}", failed.join(", ")),
                    )
                };
                return vec![QueryResult {
                    id: "ps:kill:none".to_string(),
                    title,
                    subtitle: Some(subtitle),
                    icon: Some("❌".to_string()),
                    score: 50,
                    action_type: "copy".to_string(),
                    action_data: data,
                }];
            }

            let subtitle = if failed.is_empty() {
                killed.join(", ")
            } else {
                format!("{} (failed: {})", killed.join(", "), failed.join(", "))
            };
            return vec![QueryResult {
                id: "ps:kill:done".to_string(),
                title: format!("Killed {} process(es)", killed.len()),
                subtitle: Some(subtitle),
                icon: Some("✅".to_string()),
                score: 100,
                action_type: "copy".to_string(),
                action_data: format!("Killed: {}", killed.join(", ")),
            }];
        }

        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "process_manager",
                "description": "List or kill processes",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["list", "kill"],
                            "description": "Action to perform: list top processes or kill by name"
                        },
                        "name": {
                            "type": "string",
                            "description": "Process name to kill (required if action=kill)"
                        }
                    },
                    "required": ["action"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["action"].as_str().unwrap_or("");
        match action {
            "list" => {
                let mut system = System::new_all();
                system.refresh_all();
                let mut processes: Vec<_> = system
                    .processes()
                    .iter()
                    .map(|(pid, proc)| {
                        (
                            pid.as_u32(),
                            proc.name().to_string(),
                            proc.memory(),
                            proc.cpu_usage(),
                        )
                    })
                    .collect();
                processes.sort_by_key(|b| Reverse(b.2));
                processes.truncate(10);

                if processes.is_empty() {
                    return "No processes found".to_string();
                }

                processes
                    .into_iter()
                    .enumerate()
                    .map(|(i, (pid, name, mem, cpu))| {
                        let mem_mb = mem as f64 / 1024.0;
                        format!(
                            "#{}: {} (PID: {}) — {:.1} MB, {:.1}% CPU",
                            i + 1,
                            name,
                            pid,
                            mem_mb,
                            cpu
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            "kill" => {
                let raw_name = args["name"].as_str().unwrap_or("");
                if raw_name.is_empty() {
                    return "No process name provided".to_string();
                }
                // On Windows, process names include .exe suffix
                let name_owned;
                let name = if cfg!(target_os = "windows") && !raw_name.ends_with(".exe") {
                    name_owned = format!("{}.exe", raw_name);
                    name_owned.as_str()
                } else {
                    raw_name
                };
                let mut system = System::new_all();
                system.refresh_all();
                let mut killed: Vec<String> = Vec::new();
                let mut failed: Vec<String> = Vec::new();
                for proc in system.processes_by_name(name.as_ref()) {
                    let pname = proc.name().to_string();
                    let pid = proc.pid().as_u32();
                    let entry = format!("{} (PID: {})", pname, pid);
                    if proc.kill() {
                        killed.push(entry);
                    } else {
                        failed.push(entry);
                    }
                }

                if killed.is_empty() {
                    if failed.is_empty() {
                        format!("No process found with name '{}'", name)
                    } else {
                        format!("Failed to kill: {}", failed.join(", "))
                    }
                } else if failed.is_empty() {
                    format!("Killed: {}", killed.join(", "))
                } else {
                    format!(
                        "Killed: {}\nFailed to kill: {}",
                        killed.join(", "),
                        failed.join(", ")
                    )
                }
            }
            _ => "Unknown action: use 'list' or 'kill'".to_string(),
        }
    }
}
