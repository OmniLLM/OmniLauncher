use std::path::PathBuf;

use super::external::{ext_plugins_dir, load_manifest};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn ensure_ext_plugins_dir() -> Result<PathBuf, String> {
    let dir = ext_plugins_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create ext-plugins directory: {e}"))?;
    Ok(dir)
}

/// Derive a plugin directory name from a git URL or local path.
/// git@github.com:user/my-plugin.git  →  "my-plugin"
/// https://github.com/user/my-plugin  →  "my-plugin"
/// /home/user/projects/my-plugin      →  "my-plugin"
fn dir_name_from_source(source: &str) -> String {
    let base = source.trim_end_matches('/').trim_end_matches(".git");

    base.rsplit(['/', ':'])
        .next()
        .unwrap_or("plugin")
        .to_string()
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// Install a plugin from a git URL or local path.
/// `target_dir`: optional install base directory. Defaults to `~/.omnilauncher/plugins/`.
/// Returns the plugin name on success.
pub async fn install_plugin(source: String, target_dir: Option<String>) -> Result<String, String> {
    let base_dir = match target_dir {
        Some(ref d) if !d.is_empty() => {
            let p = PathBuf::from(d);
            std::fs::create_dir_all(&p)
                .map_err(|e| format!("Failed to create target directory '{}': {e}", p.display()))?;
            p
        }
        _ => ensure_ext_plugins_dir()?,
    };
    let dir_name = dir_name_from_source(&source);
    let dest = base_dir.join(&dir_name);

    if dest.exists() {
        return Err(format!(
            "Plugin directory '{}' already exists. Remove it first with remove_plugin.",
            dir_name
        ));
    }

    let is_remote = source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@");

    if is_remote {
        // Clone the repo
        let output = tokio::process::Command::new("git")
            .args([
                "clone",
                "--depth=1",
                &source,
                dest.to_str().unwrap_or(&dir_name),
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to spawn git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git clone failed: {stderr}"));
        }
    } else {
        // Local path — resolve it
        let src_path = PathBuf::from(&source);
        if !src_path.exists() {
            return Err(format!("Local path '{}' does not exist.", source));
        }
        if !src_path.is_dir() {
            return Err(format!("'{}' is not a directory.", source));
        }

        // Try symlink first; fall back to copy
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&src_path, &dest)
                .map_err(|e| format!("Failed to create symlink: {e}"))?;
        }
        #[cfg(windows)]
        {
            // On Windows, try junction (no admin required), then copy
            let output = std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    dest.to_str().unwrap_or(""),
                    src_path.to_str().unwrap_or(""),
                ])
                .output()
                .map_err(|e| format!("Failed to create junction: {e}"))?;

            if !output.status.success() {
                // Fall back: copy directory tree
                copy_dir_recursive(&src_path, &dest)?;
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            copy_dir_recursive(&src_path, &dest)?;
        }
    }

    // Validate plugin.json
    match load_manifest(&dest) {
        Some(manifest) => {
            // Confirm entry exists
            let entry_path = dest.join(&manifest.entry);
            if !entry_path.exists() {
                // Clean up
                let _ = std::fs::remove_dir_all(&dest);
                return Err(format!(
                    "plugin.json found but entry '{}' does not exist.",
                    manifest.entry
                ));
            }
            log::info!("Installed external plugin '{}'", manifest.name);
            Ok(manifest.name)
        }
        None => {
            // Clean up invalid install
            let _ = std::fs::remove_dir_all(&dest);
            Err("No valid plugin.json found in the plugin directory.".to_string())
        }
    }
}

/// List all installed external plugins as JSON objects.
pub fn list_plugins() -> Vec<serde_json::Value> {
    let base = ext_plugins_dir();
    if !base.exists() {
        return vec![];
    }

    let mut plugins = vec![];
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(manifest) = load_manifest(&path) {
                    let mut val =
                        serde_json::to_value(&manifest).unwrap_or(serde_json::Value::Null);
                    // Attach the directory name so the UI can reference it
                    if let serde_json::Value::Object(ref mut map) = val {
                        map.insert(
                            "dir_name".to_string(),
                            serde_json::Value::String(
                                path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("")
                                    .to_string(),
                            ),
                        );
                    }
                    plugins.push(val);
                }
            }
        }
    }
    plugins
}

/// Remove an installed external plugin by name (directory name).
pub async fn remove_plugin(name: String) -> Result<(), String> {
    let base = ext_plugins_dir();
    let target = base.join(&name);

    if !target.exists() {
        return Err(format!("Plugin '{}' is not installed.", name));
    }

    // If the path is a symlink (Unix), just remove the symlink
    #[cfg(unix)]
    if target
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        std::fs::remove_file(&target)
            .map_err(|e| format!("Failed to remove plugin symlink: {e}"))?;
        log::info!("Removed external plugin symlink '{}'", name);
        return Ok(());
    }

    std::fs::remove_dir_all(&target)
        .map_err(|e| format!("Failed to remove plugin directory '{}': {e}", name))?;

    log::info!("Removed external plugin '{}'", name);
    Ok(())
}

// ─── Utility ─────────────────────────────────────────────────────────────────

#[allow(dead_code)]
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory '{}': {e}", dst.display()))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory '{}': {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("Directory entry error: {e}"))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());

        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("Failed to copy '{}': {e}", from.display()))?;
        }
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_name_from_https_url() {
        assert_eq!(
            dir_name_from_source("https://github.com/user/my-plugin"),
            "my-plugin"
        );
    }

    #[test]
    fn dir_name_from_git_url_with_dot_git() {
        assert_eq!(
            dir_name_from_source("git@github.com:user/my-plugin.git"),
            "my-plugin"
        );
    }

    #[test]
    fn dir_name_from_local_path() {
        assert_eq!(
            dir_name_from_source("/home/user/projects/my-plugin"),
            "my-plugin"
        );
    }

    #[test]
    fn dir_name_from_local_path_trailing_slash() {
        assert_eq!(
            dir_name_from_source("/home/user/projects/my-plugin/"),
            "my-plugin"
        );
    }
}
