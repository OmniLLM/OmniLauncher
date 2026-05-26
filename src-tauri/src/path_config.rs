use std::path::PathBuf;

/// Configuration directory: `~/.config/omnilauncher/`
/// Holds settings.json and other user-editable config files.
///
/// Overridden by `OMNILAUNCHER_CONFIG_DIR` for test isolation.
pub fn config_dir() -> PathBuf {
    if let Ok(base) = std::env::var("OMNILAUNCHER_CONFIG_DIR") {
        return PathBuf::from(base);
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".config")
        .join("omnilauncher")
}

/// Data directory: `~/.omnilauncher/`
/// Holds database, notes, scripts, skills, logs, and other runtime data.
///
/// Overridden by `OMNILAUNCHER_CONFIG_DIR` for test isolation (shares the
/// same temp dir so plugins can find each other's data).
pub fn data_dir() -> PathBuf {
    if let Ok(base) = std::env::var("OMNILAUNCHER_CONFIG_DIR") {
        return PathBuf::from(base);
    }
    dirs::home_dir()
        .unwrap_or_default()
        .join(".omnilauncher")
}
