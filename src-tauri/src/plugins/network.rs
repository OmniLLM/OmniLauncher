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
        Some("net ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("net ")
            .unwrap_or("")
            .trim()
            .to_lowercase();

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
