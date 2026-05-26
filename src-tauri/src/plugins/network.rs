use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Network utilities - IP, ping, DNS lookup
pub struct NetworkPlugin;

#[async_trait]
impl Plugin for NetworkPlugin {
    fn name(&self) -> &str {
        "network"
    }

    fn description(&self) -> &str {
        "Network utilities: IP address, ping, DNS"
    }

    fn keyword(&self) -> Option<&str> {
        None
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim_start();
        let Some(term) = network_term(raw) else {
            return vec![];
        };

        let commands = vec![
            ("ip", "🌍", "Show public IP address", get_ip_cmd()),
            ("localip", "🖥️", "Show local IP address", get_local_ip_cmd()),
            ("flush", "🔄", "Flush DNS cache", flush_dns_cmd()),
            (
                "connections",
                "🔗",
                "Show active connections",
                connections_cmd(),
            ),
            ("ports", "📡", "Show listening ports", ports_cmd()),
            ("wifi", "📶", "Show WiFi profiles", wifi_cmd()),
        ];

        let mut results: Vec<QueryResult> = commands
            .into_iter()
            .filter(|(name, _, desc, _)| {
                term.is_empty()
                    || name.contains(term.as_str())
                    || desc.to_lowercase().contains(&term)
            })
            .map(|(name, icon, desc, cmd)| QueryResult {
                id: format!("net:{}", name),
                title: desc.to_string(),
                subtitle: Some(format!("net {}", name)),
                icon: Some(icon.to_string()),
                score: 70,
                action_type: "shell".to_string(),
                action_data: cmd,
            })
            .collect();

        // If term looks like a hostname, offer ping
        if !term.is_empty()
            && !["ip", "localip", "flush", "connections", "ports", "wifi"].contains(&term.as_str())
        {
            results.insert(
                0,
                QueryResult {
                    id: format!("net:ping:{}", term),
                    title: format!("Ping {}", term),
                    subtitle: Some(format!("ping {}", term)),
                    icon: Some("📡".to_string()),
                    score: 80,
                    action_type: "shell".to_string(),
                    action_data: format!("ping {}", term),
                },
            );
        }

        results
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "network",
                "description": "Network utilities: get public/local IP, flush DNS, show connections, ping a host, DNS lookup",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Command: 'ip', 'localip', 'flush', 'connections', 'ports', 'wifi', 'ping <host>', or 'dns <host>'" }
                    },
                    "required": ["command"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let command = args["command"].as_str().unwrap_or("").trim();
        if command.is_empty() {
            return "Error: 'command' parameter is required. Options: ip, localip, flush, connections, ports, wifi, ping <host>, dns <host>".to_string();
        }
        let shell_cmd = if command == "ip" {
            get_ip_cmd()
        } else if command == "localip" {
            get_local_ip_cmd()
        } else if command == "flush" {
            flush_dns_cmd()
        } else if command == "connections" {
            connections_cmd()
        } else if command == "ports" {
            ports_cmd()
        } else if command == "wifi" {
            wifi_cmd()
        } else if let Some(host) = command.strip_prefix("ping ") {
            format!("ping {}", host.trim())
        } else if let Some(host) = command.strip_prefix("dns ") {
            let h = host.trim();
            if cfg!(target_os = "windows") {
                format!("nslookup {}", h)
            } else {
                format!("dig +short {}", h)
            }
        } else {
            return format!("Unknown command: '{}'. Options: ip, localip, flush, connections, ports, wifi, ping <host>, dns <host>", command);
        };

        let output = if cfg!(target_os = "windows") {
            std::process::Command::new("cmd").args(["/C", &shell_cmd]).output()
        } else {
            std::process::Command::new("sh").args(["-c", &shell_cmd]).output()
        };
        match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                if !stdout.is_empty() { stdout } else if !stderr.is_empty() { stderr } else { "Command completed with no output".to_string() }
            }
            Err(e) => format!("Error running command: {}", e),
        }
    }
}

fn network_term(raw: &str) -> Option<String> {
    if raw.trim_end().eq_ignore_ascii_case("net") {
        return Some(String::new());
    }

    if let Some(term) = raw.strip_prefix("net ") {
        return Some(term.trim().to_lowercase());
    }

    if raw.eq_ignore_ascii_case("ip") {
        return Some("ip".to_string());
    }

    raw.strip_prefix("ip ")
        .map(|term| term.trim().to_lowercase())
}

#[cfg(target_os = "windows")]
fn get_ip_cmd() -> String {
    "powershell -Command \"(Invoke-WebRequest -Uri 'https://api.ipify.org' -UseBasicParsing).Content\"".to_string()
}
#[cfg(not(target_os = "windows"))]
fn get_ip_cmd() -> String {
    "curl -s https://api.ipify.org".to_string()
}

#[cfg(target_os = "windows")]
fn get_local_ip_cmd() -> String {
    "powershell -Command \"(Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.InterfaceAlias -notlike '*Loopback*'}).IPAddress\"".to_string()
}
#[cfg(not(target_os = "windows"))]
fn get_local_ip_cmd() -> String {
    "hostname -I".to_string()
}

#[cfg(target_os = "windows")]
fn flush_dns_cmd() -> String {
    "ipconfig /flushdns".to_string()
}
#[cfg(not(target_os = "windows"))]
fn flush_dns_cmd() -> String {
    "sudo systemd-resolve --flush-caches".to_string()
}

#[cfg(target_os = "windows")]
fn connections_cmd() -> String {
    "netstat -an | findstr ESTABLISHED".to_string()
}
#[cfg(not(target_os = "windows"))]
fn connections_cmd() -> String {
    "ss -tunapl".to_string()
}

#[cfg(target_os = "windows")]
fn ports_cmd() -> String {
    "netstat -an | findstr LISTENING".to_string()
}
#[cfg(not(target_os = "windows"))]
fn ports_cmd() -> String {
    "ss -tlnp".to_string()
}

#[cfg(target_os = "windows")]
fn wifi_cmd() -> String {
    "netsh wlan show profiles".to_string()
}
#[cfg(not(target_os = "windows"))]
fn wifi_cmd() -> String {
    "nmcli connection show".to_string()
}
