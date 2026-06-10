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
    dirs::home_dir().unwrap_or_default().join(".omnilauncher")
}

/// Process-global mutex any test mutating `OMNILAUNCHER_CONFIG_DIR` must
/// acquire before calling `set_var` / `remove_var`.
///
/// `cargo test` runs tests in parallel within one process; `set_var` is
/// process-global, so two tests racing on it observe each other's tempdirs.
/// Before this lock existed, tests in `scheduler` (which had its own private
/// `ENV_LOCK`) would intermittently fail when a `skill_runner` test (which
/// had no lock) mutated the env mid-tick, redirecting `open_db()` to the
/// wrong DB and producing flaky `row still present` panics.
///
/// Use `blocking_lock()` from `#[test]` and `lock().await` from
/// `#[tokio::test]` — both work because this is a `tokio::sync::Mutex`.
#[cfg(test)]
pub static CONFIG_DIR_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
