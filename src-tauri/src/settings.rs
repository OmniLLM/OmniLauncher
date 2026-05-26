use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub ai_base_url: String,
    pub ai_model: String,
    pub ai_api_key: String,
    pub theme: String,
    pub hotkey: String,
    pub max_results: usize,
    /// Extra plugin directories to scan in addition to the default
    /// `~/.omnilauncher/plugins/`.  Each entry is an absolute path string.
    #[serde(default)]
    pub plugin_dirs: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            ai_base_url: "http://localhost:5000".to_string(),
            ai_model: "auto".to_string(),
            ai_api_key: String::new(),
            theme: "dark".to_string(),
            hotkey: "Alt+Space".to_string(),
            max_results: 10,
            plugin_dirs: vec![],
        }
    }
}

pub fn settings_path() -> std::path::PathBuf {
    let config_dir = dirs::home_dir().unwrap_or_default().join(".config");
    config_dir.join("omnilauncher").join("settings.json")
}

pub fn load_settings() -> AppSettings {
    let path = settings_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str(&content) {
                return s;
            }
        }
    }
    AppSettings::default()
}

pub fn save_settings(settings: &AppSettings) -> bool {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    match serde_json::to_string_pretty(settings) {
        Ok(json) => std::fs::write(&path, json).is_ok(),
        Err(_) => false,
    }
}
