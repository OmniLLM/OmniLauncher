use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

/// Quick access to the system hosts file
pub struct HostsPlugin;

#[async_trait]
impl Plugin for HostsPlugin {
    fn name(&self) -> &str {
        "hosts"
    }

    fn description(&self) -> &str {
        "View and search system hosts file entries"
    }

    fn keyword(&self) -> Option<&str> {
        Some("hosts ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("hosts ")
            .unwrap_or("")
            .trim()
            .to_lowercase();

        let hosts_path = get_hosts_path();
        let content = match std::fs::read_to_string(&hosts_path) {
            Ok(c) => c,
            Err(_) => {
                return vec![QueryResult {
                    id: "hosts:error".to_string(),
                    title: "Cannot read hosts file".to_string(),
                    subtitle: Some(hosts_path.clone()),
                    icon: Some("⚠️".to_string()),
                    score: 50,
                    action_type: "copy".to_string(),
                    action_data: hosts_path,
                }];
            }
        };

        let mut results: Vec<QueryResult> = content
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#')
            })
            .filter(|line| term.is_empty() || line.to_lowercase().contains(&term))
            .take(10)
            .map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                let ip = parts.first().unwrap_or(&"");
                let host = parts.get(1).unwrap_or(&"");
                QueryResult {
                    id: format!("hosts:{}", host),
                    title: host.to_string(),
                    subtitle: Some(format!("→ {}", ip)),
                    icon: Some("🌐".to_string()),
                    score: 60,
                    action_type: "copy".to_string(),
                    action_data: line.trim().to_string(),
                }
            })
            .collect();

        // Always add an "Edit hosts file" option
        results.push(QueryResult {
            id: "hosts:edit".to_string(),
            title: "Edit hosts file".to_string(),
            subtitle: Some(hosts_path.clone()),
            icon: Some("📝".to_string()),
            score: 40,
            action_type: "shell".to_string(),
            action_data: edit_hosts_cmd(),
        });

        results
    }
}

fn get_hosts_path() -> String {
    #[cfg(target_os = "windows")]
    {
        r"C:\Windows\System32\drivers\etc\hosts".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        "/etc/hosts".to_string()
    }
}

fn edit_hosts_cmd() -> String {
    #[cfg(target_os = "windows")]
    {
        r#"powershell -Command "Start-Process notepad 'C:\Windows\System32\drivers\etc\hosts' -Verb RunAs""#.to_string()
    }
    #[cfg(target_os = "macos")]
    {
        "open -a TextEdit /etc/hosts".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "xdg-open /etc/hosts".to_string()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "echo /etc/hosts".to_string()
    }
}
