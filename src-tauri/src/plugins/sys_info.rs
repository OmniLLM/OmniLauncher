use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;
use std::process::Command;

/// System info tool - inspired by hermes-agent process and PowerToys system tools
pub struct SysInfoPlugin;

#[async_trait]
impl Plugin for SysInfoPlugin {
    fn name(&self) -> &str {
        "sys_info"
    }

    fn description(&self) -> &str {
        "Get system information: CPU, memory, disk, processes"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, _q: &Query) -> Vec<QueryResult> {
        vec![]
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "sys_info",
                "description": "Get system information. Info types: cpu, memory, disk, processes, uptime, os",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "info_type": { "type": "string", "enum": ["cpu", "memory", "disk", "processes", "uptime", "os", "all"], "description": "Type of system info to retrieve" }
                    },
                    "required": ["info_type"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let info_type = args["info_type"].as_str().unwrap_or("all");

        match info_type {
            "cpu" => run_cmd(cpu_cmd()),
            "memory" => run_cmd(memory_cmd()),
            "disk" => run_cmd(disk_cmd()),
            "processes" => run_cmd(processes_cmd()),
            "uptime" => run_cmd(uptime_cmd()),
            "os" => run_cmd(os_cmd()),
            "all" => {
                let mut result = String::new();
                result.push_str("=== OS ===\n");
                result.push_str(&run_cmd(os_cmd()));
                result.push_str("\n\n=== CPU ===\n");
                result.push_str(&run_cmd(cpu_cmd()));
                result.push_str("\n\n=== Memory ===\n");
                result.push_str(&run_cmd(memory_cmd()));
                result.push_str("\n\n=== Disk ===\n");
                result.push_str(&run_cmd(disk_cmd()));
                result
            }
            _ => format!("Unknown info type: {}", info_type),
        }
    }
}

fn run_cmd(cmd_str: (&str, Vec<&str>)) -> String {
    let (program, args) = cmd_str;
    match Command::new(program).args(&args).output() {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.len() > 3000 {
                format!("{}\n... (truncated)", &text[..3000])
            } else {
                text.to_string()
            }
        }
        Err(e) => format!("Error: {}", e),
    }
}

#[cfg(target_os = "windows")]
fn cpu_cmd() -> (&'static str, Vec<&'static str>) {
    ("powershell", vec!["-NoProfile", "-Command", "Get-CimInstance Win32_Processor | Select-Object Name, NumberOfCores, NumberOfLogicalProcessors, LoadPercentage | Format-List"])
}
#[cfg(not(target_os = "windows"))]
fn cpu_cmd() -> (&'static str, Vec<&'static str>) {
    ("sh", vec!["-c", "cat /proc/cpuinfo | head -20"])
}

#[cfg(target_os = "windows")]
fn memory_cmd() -> (&'static str, Vec<&'static str>) {
    ("powershell", vec!["-NoProfile", "-Command", "Get-CimInstance Win32_OperatingSystem | Select-Object @{N='TotalGB';E={[math]::Round($_.TotalVisibleMemorySize/1MB,2)}}, @{N='FreeGB';E={[math]::Round($_.FreePhysicalMemory/1MB,2)}}, @{N='UsedGB';E={[math]::Round(($_.TotalVisibleMemorySize - $_.FreePhysicalMemory)/1MB,2)}} | Format-List"])
}
#[cfg(not(target_os = "windows"))]
fn memory_cmd() -> (&'static str, Vec<&'static str>) {
    ("free", vec!["-h"])
}

#[cfg(target_os = "windows")]
fn disk_cmd() -> (&'static str, Vec<&'static str>) {
    ("powershell", vec!["-NoProfile", "-Command", "Get-PSDrive -PSProvider FileSystem | Select-Object Name, @{N='UsedGB';E={[math]::Round($_.Used/1GB,2)}}, @{N='FreeGB';E={[math]::Round($_.Free/1GB,2)}} | Format-Table"])
}
#[cfg(not(target_os = "windows"))]
fn disk_cmd() -> (&'static str, Vec<&'static str>) {
    ("df", vec!["-h"])
}

#[cfg(target_os = "windows")]
fn processes_cmd() -> (&'static str, Vec<&'static str>) {
    ("powershell", vec!["-NoProfile", "-Command", "Get-Process | Sort-Object CPU -Descending | Select-Object -First 15 Name, CPU, @{N='MemMB';E={[math]::Round($_.WorkingSet64/1MB,1)}} | Format-Table"])
}
#[cfg(not(target_os = "windows"))]
fn processes_cmd() -> (&'static str, Vec<&'static str>) {
    ("ps", vec!["aux", "--sort=-pcpu", "|", "head", "-15"])
}

#[cfg(target_os = "windows")]
fn uptime_cmd() -> (&'static str, Vec<&'static str>) {
    ("powershell", vec!["-NoProfile", "-Command", "(Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime | Select-Object Days, Hours, Minutes | Format-List"])
}
#[cfg(not(target_os = "windows"))]
fn uptime_cmd() -> (&'static str, Vec<&'static str>) {
    ("uptime", vec![])
}

#[cfg(target_os = "windows")]
fn os_cmd() -> (&'static str, Vec<&'static str>) {
    ("powershell", vec!["-NoProfile", "-Command", "Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture | Format-List"])
}
#[cfg(not(target_os = "windows"))]
fn os_cmd() -> (&'static str, Vec<&'static str>) {
    ("uname", vec!["-a"])
}
