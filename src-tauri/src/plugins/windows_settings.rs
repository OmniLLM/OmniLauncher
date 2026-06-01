use crate::plugins::{Plugin, Query, QueryResult};
use async_trait::async_trait;

pub struct WindowsSettingsPlugin;

#[async_trait]
impl Plugin for WindowsSettingsPlugin {
    fn name(&self) -> &str {
        "system_settings"
    }

    fn description(&self) -> &str {
        if cfg!(target_os = "windows") {
            "Quick access to Windows Settings pages (type 'settings ')"
        } else if cfg!(target_os = "macos") {
            "Quick access to macOS System Settings (type 'settings ')"
        } else {
            "Quick access to system settings via GNOME/KDE/etc. (type 'settings ')"
        }
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

        // Platform-specific settings entries: (display name, action_data, description)
        #[cfg(target_os = "windows")]
        let settings: Vec<(&str, &str, &str)> = vec![
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

        // macOS: use x-apple.systempreferences: URI scheme (works on macOS 13+)
        #[cfg(target_os = "macos")]
        let settings: Vec<(&str, &str, &str)> = vec![
            (
                "Display",
                "x-apple.systempreferences:com.apple.Displays-Settings.extension",
                "Display settings",
            ),
            (
                "Sound",
                "x-apple.systempreferences:com.apple.Sound-Settings.extension",
                "Sound settings",
            ),
            (
                "Notifications",
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
                "Notification settings",
            ),
            (
                "Battery",
                "x-apple.systempreferences:com.apple.Battery-Settings.extension",
                "Battery settings",
            ),
            (
                "Bluetooth",
                "x-apple.systempreferences:com.apple.BluetoothSettings",
                "Bluetooth & devices",
            ),
            (
                "WiFi",
                "x-apple.systempreferences:com.apple.wifi-settings-extension",
                "WiFi settings",
            ),
            (
                "VPN",
                "x-apple.systempreferences:com.apple.Network-Settings.extension",
                "VPN / Network settings",
            ),
            (
                "Wallpaper",
                "x-apple.systempreferences:com.apple.Wallpaper-Settings.extension",
                "Wallpaper settings",
            ),
            (
                "Appearance",
                "x-apple.systempreferences:com.apple.Appearance-Settings.extension",
                "Appearance & accent colors",
            ),
            (
                "Lock Screen",
                "x-apple.systempreferences:com.apple.Lock-Screen-Settings.extension",
                "Lock screen settings",
            ),
            (
                "Screen Saver",
                "x-apple.systempreferences:com.apple.ScreenSaver-Settings.extension",
                "Screen saver settings",
            ),
            (
                "Dock",
                "x-apple.systempreferences:com.apple.Dock-Settings.extension",
                "Dock & menu bar",
            ),
            (
                "Mission Control",
                "x-apple.systempreferences:com.apple.MissionControl-Settings.extension",
                "Mission Control",
            ),
            (
                "Mouse",
                "x-apple.systempreferences:com.apple.Mouse-Settings.extension",
                "Mouse settings",
            ),
            (
                "Trackpad",
                "x-apple.systempreferences:com.apple.Trackpad-Settings.extension",
                "Trackpad settings",
            ),
            (
                "Keyboard",
                "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
                "Keyboard settings",
            ),
            (
                "Software Update",
                "x-apple.systempreferences:com.apple.Software-Update-Settings.extension",
                "Software Update",
            ),
            (
                "Privacy",
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension",
                "Privacy & Security",
            ),
            (
                "Time & Language",
                "x-apple.systempreferences:com.apple.Localization-Settings.extension",
                "Language & Region",
            ),
            (
                "Accounts",
                "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension",
                "Internet Accounts",
            ),
            (
                "Users & Groups",
                "x-apple.systempreferences:com.apple.Users-Groups-Settings.extension",
                "Users & Groups",
            ),
            (
                "Accessibility",
                "x-apple.systempreferences:com.apple.Accessibility-Settings.extension",
                "Accessibility",
            ),
            (
                "Focus",
                "x-apple.systempreferences:com.apple.Focus-Settings.extension",
                "Focus / Do Not Disturb",
            ),
            (
                "Energy Saver",
                "x-apple.systempreferences:com.apple.Energy-Settings.extension",
                "Energy Saver",
            ),
            (
                "Sharing",
                "x-apple.systempreferences:com.apple.Sharing-Settings.extension",
                "Sharing",
            ),
            (
                "Startup Disk",
                "x-apple.systempreferences:com.apple.Startup-Disk-Settings.extension",
                "Startup Disk",
            ),
            (
                "About",
                "x-apple.systempreferences:com.apple.SystemInformation.pane",
                "About This Mac",
            ),
        ];

        // Linux: gnome-control-center panels (falls back to xdg-open for non-GNOME)
        // action_data is a shell command run via `sh -c`
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let settings: Vec<(&str, &str, &str)> = vec![
            (
                "Display",
                "gnome-control-center display",
                "Display settings",
            ),
            ("Sound", "gnome-control-center sound", "Sound settings"),
            (
                "Notifications",
                "gnome-control-center notifications",
                "Notification settings",
            ),
            ("Power", "gnome-control-center power", "Power settings"),
            ("Bluetooth", "gnome-control-center bluetooth", "Bluetooth"),
            ("WiFi", "gnome-control-center wifi", "WiFi settings"),
            (
                "Network",
                "gnome-control-center network",
                "Network settings",
            ),
            (
                "Background",
                "gnome-control-center background",
                "Background / Wallpaper",
            ),
            (
                "Appearance",
                "gnome-control-center appearance",
                "Appearance",
            ),
            (
                "Lock Screen",
                "gnome-control-center lock-screen",
                "Lock screen",
            ),
            ("Mouse", "gnome-control-center mouse", "Mouse & Touchpad"),
            (
                "Keyboard",
                "gnome-control-center keyboard",
                "Keyboard shortcuts",
            ),
            (
                "Apps",
                "gnome-control-center applications",
                "Default applications",
            ),
            ("Privacy", "gnome-control-center privacy", "Privacy"),
            (
                "Date & Time",
                "gnome-control-center datetime",
                "Date & Time",
            ),
            ("Region", "gnome-control-center region", "Region & Language"),
            (
                "Users",
                "gnome-control-center user-accounts",
                "User Accounts",
            ),
            (
                "Accessibility",
                "gnome-control-center universal-access",
                "Accessibility",
            ),
            (
                "Online Accounts",
                "gnome-control-center online-accounts",
                "Online Accounts",
            ),
            ("Printers", "gnome-control-center printers", "Printers"),
            (
                "Removable Media",
                "gnome-control-center removable-media",
                "Removable Media",
            ),
            ("Software", "gnome-software", "Software Center"),
            (
                "Updates",
                "gnome-software --mode=updates",
                "Software Updates",
            ),
            (
                "About",
                "gnome-control-center info-overview",
                "About This Computer",
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
                id: format!("settings:{}", name),
                title: name.to_string(),
                subtitle: Some(desc.to_string()),
                icon: Some("⚙️".to_string()),
                score: 75,
                action_type: if cfg!(target_os = "windows") || cfg!(target_os = "macos") {
                    "open_url"
                } else {
                    "shell"
                }
                .to_string(),
                action_data: uri.to_string(),
                source: None,
            })
            .collect()
    }

    fn tool_schema(&self) -> Option<serde_json::Value> {
        let desc = if cfg!(target_os = "windows") {
            "Open a Windows Settings page by name"
        } else if cfg!(target_os = "macos") {
            "Open a macOS System Settings panel by name"
        } else {
            "Open a system settings panel by name (GNOME Control Center)"
        };
        Some(serde_json::json!({
            "type": "function",
            "function": {
                "name": "system_settings",
                "description": desc,
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

        #[cfg(target_os = "windows")]
        let settings: Vec<(&str, &str)> = vec![
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
        #[cfg(target_os = "macos")]
        let settings: Vec<(&str, &str)> = vec![
            (
                "display",
                "x-apple.systempreferences:com.apple.Displays-Settings.extension",
            ),
            (
                "sound",
                "x-apple.systempreferences:com.apple.Sound-Settings.extension",
            ),
            (
                "notifications",
                "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
            ),
            (
                "battery",
                "x-apple.systempreferences:com.apple.Battery-Settings.extension",
            ),
            (
                "bluetooth",
                "x-apple.systempreferences:com.apple.BluetoothSettings",
            ),
            (
                "wifi",
                "x-apple.systempreferences:com.apple.wifi-settings-extension",
            ),
            (
                "network",
                "x-apple.systempreferences:com.apple.Network-Settings.extension",
            ),
            (
                "wallpaper",
                "x-apple.systempreferences:com.apple.Wallpaper-Settings.extension",
            ),
            (
                "appearance",
                "x-apple.systempreferences:com.apple.Appearance-Settings.extension",
            ),
            (
                "lockscreen",
                "x-apple.systempreferences:com.apple.Lock-Screen-Settings.extension",
            ),
            (
                "screensaver",
                "x-apple.systempreferences:com.apple.ScreenSaver-Settings.extension",
            ),
            (
                "dock",
                "x-apple.systempreferences:com.apple.Dock-Settings.extension",
            ),
            (
                "mouse",
                "x-apple.systempreferences:com.apple.Mouse-Settings.extension",
            ),
            (
                "trackpad",
                "x-apple.systempreferences:com.apple.Trackpad-Settings.extension",
            ),
            (
                "keyboard",
                "x-apple.systempreferences:com.apple.Keyboard-Settings.extension",
            ),
            (
                "update",
                "x-apple.systempreferences:com.apple.Software-Update-Settings.extension",
            ),
            (
                "privacy",
                "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension",
            ),
            (
                "time",
                "x-apple.systempreferences:com.apple.Localization-Settings.extension",
            ),
            (
                "accounts",
                "x-apple.systempreferences:com.apple.Internet-Accounts-Settings.extension",
            ),
            (
                "users",
                "x-apple.systempreferences:com.apple.Users-Groups-Settings.extension",
            ),
            (
                "accessibility",
                "x-apple.systempreferences:com.apple.Accessibility-Settings.extension",
            ),
            (
                "focus",
                "x-apple.systempreferences:com.apple.Focus-Settings.extension",
            ),
            (
                "energy",
                "x-apple.systempreferences:com.apple.Energy-Settings.extension",
            ),
            (
                "sharing",
                "x-apple.systempreferences:com.apple.Sharing-Settings.extension",
            ),
            (
                "about",
                "x-apple.systempreferences:com.apple.SystemInformation.pane",
            ),
        ];
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        let settings: Vec<(&str, &str)> = vec![
            ("display", "gnome-control-center display"),
            ("sound", "gnome-control-center sound"),
            ("notifications", "gnome-control-center notifications"),
            ("power", "gnome-control-center power"),
            ("bluetooth", "gnome-control-center bluetooth"),
            ("wifi", "gnome-control-center wifi"),
            ("network", "gnome-control-center network"),
            ("background", "gnome-control-center background"),
            ("appearance", "gnome-control-center appearance"),
            ("lockscreen", "gnome-control-center lock-screen"),
            ("mouse", "gnome-control-center mouse"),
            ("keyboard", "gnome-control-center keyboard"),
            ("apps", "gnome-control-center applications"),
            ("privacy", "gnome-control-center privacy"),
            ("time", "gnome-control-center datetime"),
            ("region", "gnome-control-center region"),
            ("users", "gnome-control-center user-accounts"),
            ("accessibility", "gnome-control-center universal-access"),
            ("accounts", "gnome-control-center online-accounts"),
            ("printers", "gnome-control-center printers"),
            ("software", "gnome-software"),
            ("update", "gnome-software --mode=updates"),
            ("about", "gnome-control-center info-overview"),
        ];

        let matched = settings
            .iter()
            .find(|(name, _)| page.contains(name) || name.contains(page.as_str()));
        match matched {
            Some((name, uri)) => {
                #[cfg(target_os = "windows")]
                let output = std::process::Command::new("cmd")
                    .args(["/C", &format!("start {}", uri)])
                    .output();
                #[cfg(target_os = "macos")]
                let output = std::process::Command::new("open")
                    .arg(uri)
                    .output();
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                let output = std::process::Command::new("sh")
                    .args(["-c", &format!("{} &", uri)])
                    .output();
                match output {
                    Ok(_) => format!("Opened system settings: {} ({})", name, uri),
                    Err(e) => format!("Error opening settings: {}", e),
                }
            }
            None => format!(
                "Unknown settings page: '{}'. Try: display, sound, notifications, power, bluetooth, wifi, privacy, update, apps",
                page
            ),
        }
    }
}
