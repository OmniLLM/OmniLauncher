use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::external::{discover_plugins_in_repo, ext_plugins_dir, load_manifest};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn ensure_ext_plugins_dir() -> Result<PathBuf, String> {
    let dir = plugin_storage_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create ext-plugins directory: {e}"))?;
    Ok(dir)
}

fn plugin_storage_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("OMNILAUNCHER_PLUGIN_BASE_DIR") {
        let path = PathBuf::from(dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    ext_plugins_dir()
}

fn run_git_command(args: &[&str], cwd: &PathBuf) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("Failed to spawn git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        return Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("git {:?} failed", args)
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_remote_url(dir: &PathBuf) -> Option<String> {
    run_git_command(&["remote", "get-url", "origin"], dir).ok()
}

fn git_branch(dir: &PathBuf) -> Option<String> {
    run_git_command(&["rev-parse", "--abbrev-ref", "HEAD"], dir).ok()
}

fn git_ahead_behind(dir: &PathBuf) -> Option<(u64, u64)> {
    let branch = git_branch(dir)?;
    if branch == "HEAD" {
        return Some((0, 0));
    }
    let output =
        run_git_command(&["rev-list", "--left-right", "--count", "HEAD...@{u}"], dir).ok()?;
    let mut parts = output.split_whitespace();
    let behind = parts.next()?.parse::<u64>().ok()?;
    let ahead = parts.next()?.parse::<u64>().ok()?;
    Some((ahead, behind))
}

fn git_status_clean(dir: &PathBuf) -> Option<bool> {
    let output = run_git_command(&["status", "--porcelain"], dir).ok()?;
    Some(output.is_empty())
}

fn inspect_git_repo(dir: &PathBuf) -> Option<serde_json::Value> {
    let is_repo = run_git_command(&["rev-parse", "--is-inside-work-tree"], dir).ok()?;
    if is_repo.trim() != "true" {
        return None;
    }

    let remote_url = git_remote_url(dir);
    let branch = git_branch(dir);
    let clean = git_status_clean(dir);
    let ahead_behind = git_ahead_behind(dir);

    Some(serde_json::json!({
        "is_git_repo": true,
        "git_remote": remote_url,
        "git_branch": branch,
        "git_clean": clean,
        "git_ahead": ahead_behind.map(|p| p.0),
        "git_behind": ahead_behind.map(|p| p.1),
    }))
}

/// Derive a plugin directory name from a git URL or local path.
/// git@github.com:user/my-plugin.git  →  "my-plugin"
/// https://github.com/user/my-plugin  →  "my-plugin"
/// /home/user/projects/my-plugin      →  "my-plugin"
fn dir_name_from_source(source: &str) -> String {
    let base = source.trim_end_matches('/').trim_end_matches(".git");

    base.rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or("plugin")
        .to_string()
}

fn legacy_split_collection_identity(repo_name: &str) -> Option<(String, String, String)> {
    const OMNILAUNCHER_PLUGINS: &[&str] = &[
        "color",
        "currency",
        "db-backup",
        "devdocs",
        "ip-info",
        "timestamp",
    ];

    if OMNILAUNCHER_PLUGINS.contains(&repo_name) {
        let collection = "OmniLLM/omnilauncher-plugins".to_string();
        return Some((
            collection.clone(),
            collection,
            "https://github.com/OmniLLM/omnilauncher-plugins.git".to_string(),
        ));
    }

    None
}

fn legacy_split_collection_source(collection_key: &str) -> Option<String> {
    if collection_key == "OmniLLM/omnilauncher-plugins" {
        return Some("https://github.com/OmniLLM/omnilauncher-plugins.git".to_string());
    }

    None
}

fn infer_collection_identity(
    repo_dir: &PathBuf,
    base_dir: &PathBuf,
    discovered_count: usize,
) -> (String, String) {
    let repo_name = repo_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    if let Some((name, key, _source)) = legacy_split_collection_identity(&repo_name) {
        return (name, key);
    }

    // A real collection repo (multiple plugin folders inside one repo)
    if discovered_count > 1 {
        return (repo_name.clone(), repo_name);
    }

    let canonical_base = std::fs::canonicalize(base_dir).unwrap_or_else(|_| base_dir.clone());
    let canonical_repo = match std::fs::canonicalize(repo_dir) {
        Ok(p) => p,
        Err(_) => return (repo_name.clone(), repo_name),
    };

    let Some(parent) = canonical_repo.parent() else {
        return (repo_name.clone(), repo_name);
    };

    // If plugin dir resolves outside the plugin base dir (e.g. Windows junction)
    // and its parent folder looks like a collection (contains multiple plugins),
    // use that parent folder name as collection identity.
    if parent != canonical_base {
        let sibling_plugin_dirs = std::fs::read_dir(parent)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.is_dir() && load_manifest(p).is_some())
                    .count()
            })
            .unwrap_or(0);

        if sibling_plugin_dirs >= 2 {
            let collection_name = parent
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|n| !n.is_empty())
                .unwrap_or(&repo_name)
                .to_string();
            let collection_key = parent.to_string_lossy().to_string();
            return (collection_name, collection_key);
        }
    }

    (repo_name.clone(), repo_name)
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
        let dest_str = dest.to_string_lossy().into_owned();
        let output = tokio::process::Command::new("git")
            .args(["clone", "--depth=1", &source, &dest_str])
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

    install_staged_plugin(&dest)
}

pub async fn update_plugin(name: String) -> Result<String, String> {
    let base = ensure_ext_plugins_dir()?;
    let target = base.join(&name);

    if !target.exists() {
        return Err(format!("Plugin '{}' is not installed.", name));
    }

    let is_git_repo = run_git_command(&["rev-parse", "--is-inside-work-tree"], &target)
        .map(|s| s.trim() == "true")
        .unwrap_or(false);

    if !is_git_repo {
        return Err(format!(
            "Plugin '{}' is not a git repository and cannot be updated.",
            name
        ));
    }

    let status = run_git_command(&["status", "--porcelain"], &target)?;
    if !status.is_empty() {
        return Err(format!(
            "Plugin '{}' has local changes. Commit or stash them before updating.",
            name
        ));
    }

    let _remote = git_remote_url(&target)
        .ok_or_else(|| format!("Plugin '{}' has no git remote configured.", name))?;

    let pull_result = run_git_command(&["pull", "--ff-only"], &target)?;

    Ok(format!("Updated {}\n{}", name, pull_result))
}

pub async fn update_plugin_collection(
    source: String,
    plugin_dirs: Vec<String>,
) -> Result<String, String> {
    if plugin_dirs.is_empty() {
        return Err("No plugins selected for collection update.".to_string());
    }

    let base = ensure_ext_plugins_dir()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("System clock error: {e}"))?
        .as_millis();
    let stage = std::env::temp_dir().join(format!("omnilauncher-plugin-update-{now}"));

    let is_remote = source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@");

    if is_remote {
        let stage_str = stage.to_string_lossy().into_owned();
        let output = tokio::process::Command::new("git")
            .args(["clone", "--depth=1", &source, &stage_str])
            .output()
            .await
            .map_err(|e| format!("Failed to spawn git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git clone failed: {stderr}"));
        }
    } else {
        let src_path = PathBuf::from(&source);
        if !src_path.is_dir() {
            return Err(format!(
                "Collection source '{}' is not a directory.",
                source
            ));
        }
        copy_dir_recursive(&src_path, &stage)?;
    }

    let discovered = discover_plugins_in_repo(&stage);
    let mut updated = Vec::new();
    let mut missing = Vec::new();

    for dir_name in plugin_dirs {
        let Some((plugin_dir, _manifest)) = discovered.iter().find(|(plugin_dir, _manifest)| {
            plugin_dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n == dir_name)
        }) else {
            missing.push(dir_name);
            continue;
        };

        let target = base.join(&dir_name);
        let backup = base.join(format!(".{dir_name}.update-backup-{now}"));

        if target.exists() {
            std::fs::rename(&target, &backup)
                .map_err(|e| format!("Failed to stage existing plugin '{}': {e}", dir_name))?;
        }

        if let Err(error) = copy_dir_recursive(plugin_dir, &target) {
            if backup.exists() {
                let _ = std::fs::rename(&backup, &target);
            }
            let _ = std::fs::remove_dir_all(&stage);
            return Err(error);
        }

        if backup.exists() {
            let _ = std::fs::remove_dir_all(&backup);
        }
        updated.push(dir_name);
    }

    let _ = std::fs::remove_dir_all(&stage);

    if !missing.is_empty() {
        return Err(format!(
            "Updated {}, missing {} in collection source: {}",
            updated.len(),
            missing.len(),
            missing.join(", ")
        ));
    }

    Ok(format!(
        "Updated collection plugins: {}",
        updated.join(", ")
    ))
}

fn install_staged_plugin(dest: &PathBuf) -> Result<String, String> {
    let discovered = discover_plugins_in_repo(dest);
    if discovered.is_empty() {
        let _ = std::fs::remove_dir_all(dest);
        return Err(
            "No valid plugin.json found in the plugin directory or its immediate subdirectories."
                .to_string(),
        );
    }

    if discovered.len() == 1 && discovered[0].0 == *dest {
        let manifest = &discovered[0].1;
        log::info!("Installed external plugin '{}'", manifest.name);
        return Ok(format!("Installed {}", manifest.name));
    }

    let installed_names = discovered
        .iter()
        .map(|(_, manifest)| manifest.name.clone())
        .collect::<Vec<_>>();

    log::info!(
        "Installed external plugin repo '{}' with {} plugins",
        dest.display(),
        installed_names.len()
    );

    Ok(format!(
        "Installed {} plugins: {}",
        installed_names.len(),
        installed_names.join(", ")
    ))
}

/// List all installed external plugins as JSON objects.
pub fn list_plugins() -> Vec<serde_json::Value> {
    let base = plugin_storage_dir();
    if !base.exists() {
        return vec![];
    }

    let mut plugins = vec![];
    if let Ok(entries) = std::fs::read_dir(&base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let discovered = discover_plugins_in_repo(&path);
                if discovered.is_empty() {
                    continue;
                }

                let (collection_name, collection_key) =
                    infer_collection_identity(&path, &base, discovered.len());
                let collection_source = legacy_split_collection_source(&collection_key);

                let mut repo = serde_json::json!({
                    "dir_name": path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    "collection_name": collection_name,
                    "collection_key": collection_key,
                    "collection_source": collection_source,
                    "plugins": []
                });

                if let serde_json::Value::Object(ref mut map) = repo {
                    if let Some(serde_json::Value::Object(meta)) = inspect_git_repo(&path) {
                        for (key, value) in meta {
                            map.insert(key, value);
                        }
                    }

                    let mut child_plugins = Vec::new();
                    for (plugin_dir, manifest) in discovered {
                        let mut plugin =
                            serde_json::to_value(&manifest).unwrap_or(serde_json::Value::Null);
                        if let serde_json::Value::Object(ref mut plugin_map) = plugin {
                            plugin_map.insert(
                                "dir_name".to_string(),
                                serde_json::Value::String(
                                    plugin_dir
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("")
                                        .to_string(),
                                ),
                            );
                        }
                        child_plugins.push(plugin);
                    }

                    map.insert(
                        "plugins".to_string(),
                        serde_json::Value::Array(child_plugins),
                    );
                }
                plugins.push(repo);
            }
        }
    }
    plugins
}

/// Remove an installed external plugin by name (directory name).
pub async fn remove_plugin(name: String) -> Result<(), String> {
    let base = plugin_storage_dir();
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
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn git(args: &[&str], cwd: &std::path::Path) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .unwrap_or_else(|e| panic!("failed to run git {:?}: {e}", args));
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn write_test_plugin(dir: &std::path::Path, name: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            format!(
                r#"{{"name":"{name}","description":"Test plugin","version":"1.0.0","entry":"run.cmd"}}"#
            ),
        )
        .unwrap();
        std::fs::write(dir.join("run.cmd"), "@echo off\r\necho {}\r\n").unwrap();
    }

    #[tokio::test]
    async fn install_single_plugin_from_local_path() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_test_plugin(source.path(), "single-test");

        let result = install_plugin(
            source.path().to_string_lossy().to_string(),
            Some(target.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        let installed_dir = source.path().file_name().unwrap();
        assert_eq!(result, "Installed single-test");
        assert!(target
            .path()
            .join(installed_dir)
            .join("plugin.json")
            .exists());
    }

    #[tokio::test]
    async fn install_plugin_collection_from_local_path() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        write_test_plugin(&source.path().join("color"), "color");
        write_test_plugin(&source.path().join("currency"), "currency");
        std::fs::create_dir(source.path().join("notes")).unwrap();

        let result = install_plugin(
            source.path().to_string_lossy().to_string(),
            Some(target.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        assert!(result.contains("Installed 2 plugins"), "{result}");
        assert!(result.contains("color"), "{result}");
        assert!(result.contains("currency"), "{result}");
        let installed_dir = source.path().file_name().unwrap();
        assert!(target
            .path()
            .join(installed_dir)
            .join("color")
            .join("plugin.json")
            .exists());
        assert!(target
            .path()
            .join(installed_dir)
            .join("currency")
            .join("plugin.json")
            .exists());
    }

    #[tokio::test]
    async fn install_plugin_collection_requires_at_least_one_valid_plugin() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        std::fs::create_dir(source.path().join("not-a-plugin")).unwrap();

        let error = install_plugin(
            source.path().to_string_lossy().to_string(),
            Some(target.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap_err();

        assert_eq!(
            error,
            "No valid plugin.json found in the plugin directory or its immediate subdirectories."
        );
    }

    #[test]
    fn list_plugins_groups_legacy_split_collection_dirs() {
        let _guard = ENV_LOCK.lock().unwrap();
        let target = TempDir::new().unwrap();
        write_test_plugin(&target.path().join("color"), "color");
        write_test_plugin(&target.path().join("currency"), "currency");

        std::env::set_var(
            "OMNILAUNCHER_PLUGIN_BASE_DIR",
            target.path().to_string_lossy().to_string(),
        );

        let plugins = list_plugins();
        assert_eq!(plugins.len(), 2);
        for plugin in plugins {
            assert_eq!(
                plugin["collection_name"].as_str(),
                Some("OmniLLM/omnilauncher-plugins")
            );
            assert_eq!(
                plugin["collection_key"].as_str(),
                Some("OmniLLM/omnilauncher-plugins")
            );
        }

        std::env::remove_var("OMNILAUNCHER_PLUGIN_BASE_DIR");
    }

    #[tokio::test]
    async fn update_plugin_pulls_latest_git_commit() {
        let _guard = ENV_LOCK.lock().unwrap();

        let remote = TempDir::new().unwrap();
        git(&["init", "--bare"], remote.path());

        let work = TempDir::new().unwrap();
        git(&["init"], work.path());
        git(&["config", "user.email", "test@example.com"], work.path());
        git(&["config", "user.name", "Test User"], work.path());

        write_test_plugin(work.path(), "git-test");
        std::fs::write(work.path().join("run.cmd"), "@echo off\r\necho v1\r\n").unwrap();
        git(&["add", "."], work.path());
        git(&["commit", "-m", "initial"], work.path());
        let remote_path = remote.path().to_string_lossy().replace('\\', "/");
        git(&["remote", "add", "origin", &remote_path], work.path());
        git(&["push", "-u", "origin", "HEAD"], work.path());

        let target = TempDir::new().unwrap();
        std::env::set_var(
            "OMNILAUNCHER_PLUGIN_BASE_DIR",
            target.path().to_string_lossy().to_string(),
        );
        let install_result = install_plugin(work.path().to_string_lossy().to_string(), None)
            .await
            .unwrap();
        assert_eq!(install_result, "Installed git-test");

        std::fs::write(work.path().join("run.cmd"), "@echo off\r\necho v2\r\n").unwrap();
        git(&["add", "."], work.path());
        git(&["commit", "-m", "update"], work.path());
        git(&["push"], work.path());

        let installed_dir_name = work
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let update_result = update_plugin(installed_dir_name.clone()).await.unwrap();
        assert!(update_result.contains("Updated"), "{update_result}");

        let installed_path = target.path().join(installed_dir_name);
        let updated = std::fs::read_to_string(installed_path.join("run.cmd")).unwrap();
        assert!(
            updated.contains("v2"),
            "Expected updated plugin content, got: {updated}"
        );

        std::env::remove_var("OMNILAUNCHER_PLUGIN_BASE_DIR");
    }

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
