use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct WindowsSettingsPlugin;

#[async_trait]
impl Plugin for WindowsSettingsPlugin {
    fn name(&self) -> &str {
        "windows_settings"
    }

    fn description(&self) -> &str {
        "Quick access to Windows Settings pages"
    }

    fn keyword(&self) -> Option<&str> {
        Some("settings ")
    }

    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let term = q
            .raw
            .strip_prefix("settings ")
            .unwrap_or("")
            .trim()
            .to_lowercase();

        let settings = vec![
            ("Display", "ms-settings:display", "Display settings"),
            ("Sound", "ms-settings:sound", "Sound settings"),
            (
                "Notifications",
                "ms-settings:notifications",
                "Notification settings",
            ),
            ("Power", "ms-settings:powersleep", "Power & sleep"),
            ("Battery", "ms-settings:batterysaver", "Battery settings"),
            ("Storage", "ms-settings:storagesense", "Storage settings"),
            ("Bluetooth", "ms-settings:bluetooth", "Bluetooth & devices"),
            ("WiFi", "ms-settings:network-wifi", "WiFi settings"),
            ("VPN", "ms-settings:network-vpn", "VPN settings"),
            ("Proxy", "ms-settings:network-proxy", "Proxy settings"),
            (
                "Ethernet",
                "ms-settings:network-ethernet",
                "Ethernet settings",
            ),
            (
                "Background",
                "ms-settings:personalization-background",
                "Background wallpaper",
            ),
            (
                "Colors",
                "ms-settings:personalization-colors",
                "Accent colors",
            ),
            (
                "Lock Screen",
                "ms-settings:lockscreen",
                "Lock screen settings",
            ),
            ("Taskbar", "ms-settings:taskbar", "Taskbar settings"),
            ("Startup Apps", "ms-settings:startupapps", "Startup apps"),
            ("Default Apps", "ms-settings:defaultapps", "Default apps"),
            (
                "Optional Features",
                "ms-settings:optionalfeatures",
                "Optional features",
            ),
            ("About", "ms-settings:about", "System info"),
            (
                "Windows Update",
                "ms-settings:windowsupdate",
                "Windows Update",
            ),
            ("Mouse", "ms-settings:mousetouchpad", "Mouse settings"),
            ("Keyboard", "ms-settings:keyboard", "Keyboard settings"),
            ("Apps", "ms-settings:appsfeatures", "Apps & features"),
            ("Privacy", "ms-settings:privacy", "Privacy settings"),
            ("Time & Language", "ms-settings:dateandtime", "Date & time"),
            ("Region", "ms-settings:regionformatting", "Region settings"),
            ("Accounts", "ms-settings:yourinfo", "Your account info"),
            ("Sign-in", "ms-settings:signinoptions", "Sign-in options"),
            ("Recovery", "ms-settings:recovery", "Recovery options"),
            ("Developers", "ms-settings:developers", "Developer settings"),
            ("Night Light", "ms-settings:nightlight", "Night light"),
            ("Focus", "ms-settings:quiethours", "Focus assist"),
            ("Multitasking", "ms-settings:multitasking", "Multitasking"),
            ("Clipboard", "ms-settings:clipboard", "Clipboard settings"),
            (
                "Remote Desktop",
                "ms-settings:remotedesktop",
                "Remote desktop",
            ),
            (
                "Firewall",
                "ms-settings:windowsdefender",
                "Windows Security",
            ),
        ];

        settings
            .into_iter()
            .filter(|(name, _, desc)| {
                term.is_empty()
                    || name.to_lowercase().contains(&term)
                    || desc.to_lowercase().contains(&term)
            })
            .take(10)
            .map(|(name, uri, desc)| QueryResult {
                id: format!("winsettings:{}", name),
                title: name.to_string(),
                subtitle: Some(desc.to_string()),
                icon: Some("⚙️".to_string()),
                score: 75,
                action_type: "open_url".to_string(),
                action_data: uri.to_string(),
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "windows_settings",
                "description": "Open a Windows Settings page by name",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "page": { "type": "string", "description": "Settings page name e.g. 'display', 'sound', 'notifications', 'power', 'bluetooth', 'wifi', 'privacy', 'update', 'apps'" }
                    },
                    "required": ["page"]
                }
            }
        }))
    }

    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let page = args["page"].as_str().unwrap_or("").trim().to_lowercase();
        if page.is_empty() {
            return "Error: 'page' parameter is required".to_string();
        }
        let settings = vec![
            ("display", "ms-settings:display"),
            ("sound", "ms-settings:sound"),
            ("notifications", "ms-settings:notifications"),
            ("power", "ms-settings:powersleep"),
            ("battery", "ms-settings:batterysaver"),
            ("storage", "ms-settings:storagesense"),
            ("bluetooth", "ms-settings:bluetooth"),
            ("wifi", "ms-settings:network-wifi"),
            ("vpn", "ms-settings:network-vpn"),
            ("proxy", "ms-settings:network-proxy"),
            ("ethernet", "ms-settings:network-ethernet"),
            ("background", "ms-settings:personalization-background"),
            ("colors", "ms-settings:personalization-colors"),
            ("lockscreen", "ms-settings:lockscreen"),
            ("taskbar", "ms-settings:taskbar"),
            ("startup", "ms-settings:startupapps"),
            ("defaultapps", "ms-settings:defaultapps"),
            ("about", "ms-settings:about"),
            ("update", "ms-settings:windowsupdate"),
            ("mouse", "ms-settings:mousetouchpad"),
            ("keyboard", "ms-settings:keyboard"),
            ("apps", "ms-settings:appsfeatures"),
            ("privacy", "ms-settings:privacy"),
            ("time", "ms-settings:dateandtime"),
            ("region", "ms-settings:regionformatting"),
            ("accounts", "ms-settings:yourinfo"),
            ("signin", "ms-settings:signinoptions"),
            ("recovery", "ms-settings:recovery"),
            ("developers", "ms-settings:developers"),
            ("nightlight", "ms-settings:nightlight"),
            ("focus", "ms-settings:quiethours"),
            ("multitasking", "ms-settings:multitasking"),
            ("clipboard", "ms-settings:clipboard"),
            ("remotedesktop", "ms-settings:remotedesktop"),
            ("firewall", "ms-settings:windowsdefender"),
        ];
        let matched = settings.iter().find(|(name, _)| page.contains(name) || name.contains(&page.as_str()));
        match matched {
            Some((name, uri)) => {
                let output = std::process::Command::new("cmd").args(["/C", &format!("start {}", uri)]).output();
                match output {
                    Ok(_) => format!("Opened Windows Settings: {} ({})", name, uri),
                    Err(e) => format!("Error opening settings: {}", e),
                }
            }
            None => format!("Unknown settings page: '{}'. Try: display, sound, notifications, power, bluetooth, wifi, privacy, update, apps", page),
        }
    }
}
