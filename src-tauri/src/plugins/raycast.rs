//! Raycast extension adapter.
//!
//! OmniLauncher can install and run a useful subset of Raycast extensions
//! (https://www.raycast.com/store, https://github.com/raycast/extensions).
//!
//! Detection: a directory is considered a Raycast extension when its
//! `package.json` has `@raycast/api` in `dependencies` (or
//! `devDependencies`) AND a top-level `commands` array.
//!
//! When such an extension is installed, we synthesize the files needed
//! to fit OmniLauncher's stdin/stdout plugin protocol:
//!
//! - `plugin.json` — OmniLauncher manifest pointing at our shim
//! - `raycast-shim.cjs` — entry that lists / dispatches Raycast commands
//! - `raycast-api-shim.cjs` — mock of `@raycast/api`
//! - `raycast-source-loader.cjs` — helper to run TS source via `tsx`
//!
//! Execution: only "no-view" or simple commands fully work. View commands
//! degrade to printing captured side-effects (toast / HUD / clipboard).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

const SHIM_JS: &str = include_str!("../../assets/raycast-shim/shim.cjs");
const API_SHIM_JS: &str = include_str!("../../assets/raycast-shim/raycast-api-shim.cjs");
const SOURCE_LOADER_JS: &str =
    include_str!("../../assets/raycast-shim/raycast-source-loader.cjs");

const SHIM_FILENAME: &str = "raycast-shim.cjs";
const API_SHIM_FILENAME: &str = "raycast-api-shim.cjs";
const SOURCE_LOADER_FILENAME: &str = "raycast-source-loader.cjs";

const RAYCAST_EXTENSIONS_REPO: &str = "https://github.com/raycast/extensions.git";

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RaycastCommand {
    name: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    icon: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RaycastPackage {
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    commands: Option<Vec<RaycastCommand>>,
    #[serde(default)]
    dependencies: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, rename = "devDependencies")]
    dev_dependencies: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Serialize)]
struct SyntheticManifest<'a> {
    name: &'a str,
    description: &'a str,
    version: &'a str,
    keyword: &'a str,
    icon: &'a str,
    entry: &'a str,
    entry_windows: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_schema: Option<serde_json::Value>,
}

/// Build an OpenAI-style function tool schema so the AI agent can
/// invoke a Raycast extension by name. The tool exposes one parameter
/// `query` (required) and, when the extension has more than one
/// command, a `command` enum to pick which command to run.
fn build_tool_schema(pkg: &RaycastPackage) -> Option<serde_json::Value> {
    let cmds = pkg.commands.as_ref().filter(|c| !c.is_empty())?;

    let descs = cmds
        .iter()
        .map(|c| {
            let title = c.title.as_deref().unwrap_or(&c.name);
            match c.description.as_deref() {
                Some(d) if !d.is_empty() => format!("  - {}: {} ({})", c.name, title, d),
                _ => format!("  - {}: {}", c.name, title),
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let base_desc = pkg
        .description
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Raycast extension");

    let description = if cmds.len() > 1 {
        format!(
            "{} (Raycast extension). Commands:\n{}",
            base_desc, descs
        )
    } else {
        format!("{} (Raycast extension)", base_desc)
    };

    let mut properties = serde_json::Map::new();
    properties.insert(
        "query".into(),
        serde_json::json!({
            "type": "string",
            "description": "Search text or argument passed to the Raycast command (e.g. ticker symbol, search query)."
        }),
    );
    let mut required = vec!["query".to_string()];

    if cmds.len() > 1 {
        let names: Vec<String> = cmds.iter().map(|c| c.name.clone()).collect();
        properties.insert(
            "command".into(),
            serde_json::json!({
                "type": "string",
                "enum": names,
                "description": "Which command in the extension to invoke."
            }),
        );
        required.push("command".to_string());
    }

    Some(serde_json::json!({
        "type": "function",
        "function": {
            "name": pkg.name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        }
    }))
}

fn read_package(dir: &Path) -> Option<RaycastPackage> {
    let path = dir.join("package.json");
    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim_start_matches('\u{feff}');
    serde_json::from_str(trimmed)
        .map_err(|e| {
            log::debug!("Failed to parse {}: {e}", path.display());
            e
        })
        .ok()
}

fn depends_on_raycast_api(pkg: &RaycastPackage) -> bool {
    let has = |map: &Option<serde_json::Map<String, serde_json::Value>>| {
        map.as_ref()
            .map(|m| m.contains_key("@raycast/api"))
            .unwrap_or(false)
    };
    has(&pkg.dependencies) || has(&pkg.dev_dependencies)
}

/// Returns true when `dir` looks like a Raycast extension source repo.
pub fn is_raycast_extension(dir: &Path) -> bool {
    let Some(pkg) = read_package(dir) else {
        return false;
    };
    let has_commands = pkg.commands.as_ref().map(|c| !c.is_empty()).unwrap_or(false);
    has_commands && depends_on_raycast_api(&pkg)
}

/// Write the shim files and a synthesized `plugin.json` for the Raycast
/// extension at `dir`. Idempotent: re-running on an already-synthesized
/// extension overwrites the shims (so updates pick up shim improvements)
/// but leaves a pre-existing `plugin.json` alone if it was user-authored
/// (i.e. doesn't reference our shim).
pub fn synthesize_plugin_files(dir: &Path) -> Result<String, String> {
    let pkg = read_package(dir)
        .ok_or_else(|| format!("{} has no readable package.json", dir.display()))?;

    if !depends_on_raycast_api(&pkg) {
        return Err(format!(
            "{} is not a Raycast extension (missing @raycast/api dependency).",
            dir.display()
        ));
    }
    let commands = pkg
        .commands
        .as_ref()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| format!("{} has no Raycast commands.", dir.display()))?;

    // Write the shim files (always, so upgrades to the shim apply).
    std::fs::write(dir.join(SHIM_FILENAME), SHIM_JS)
        .map_err(|e| format!("Failed to write {SHIM_FILENAME}: {e}"))?;
    std::fs::write(dir.join(API_SHIM_FILENAME), API_SHIM_JS)
        .map_err(|e| format!("Failed to write {API_SHIM_FILENAME}: {e}"))?;
    std::fs::write(dir.join(SOURCE_LOADER_FILENAME), SOURCE_LOADER_JS)
        .map_err(|e| format!("Failed to write {SOURCE_LOADER_FILENAME}: {e}"))?;

    let plugin_json_path = dir.join("plugin.json");
    let should_write_manifest = if plugin_json_path.exists() {
        // Only overwrite if it already targets our shim (i.e. was previously
        // synthesized). Preserve user-authored manifests.
        std::fs::read_to_string(&plugin_json_path)
            .map(|s| s.contains(SHIM_FILENAME))
            .unwrap_or(true)
    } else {
        true
    };

    if should_write_manifest {
        let description = pkg
            .description
            .as_deref()
            .unwrap_or("Raycast extension (OmniLauncher adapter)");
        let version = pkg.version.as_deref().unwrap_or("0.0.0");
        let icon = pkg.icon.as_deref().unwrap_or("🟥");
        // Use the package name as keyword so users type e.g. `gif-search foo`.
        let keyword = pkg.name.as_str();

        let manifest = SyntheticManifest {
            name: &pkg.name,
            description,
            version,
            keyword,
            icon,
            entry: SHIM_FILENAME,
            entry_windows: SHIM_FILENAME,
            tool_schema: build_tool_schema(&pkg),
        };
        let json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize plugin.json: {e}"))?;
        std::fs::write(&plugin_json_path, json)
            .map_err(|e| format!("Failed to write plugin.json: {e}"))?;
    }

    log::info!(
        "Synthesized Raycast adapter for '{}' ({} command{}) at {}",
        pkg.name,
        commands.len(),
        if commands.len() == 1 { "" } else { "s" },
        dir.display()
    );
    Ok(pkg.name.clone())
}

/// Walk one level deep under `repo_dir` and synthesize plugin files for
/// every Raycast extension found. Returns the names of extensions
/// successfully synthesized. Errors per-extension are logged but do not
/// abort the walk.
pub fn synthesize_raycast_extensions_in(repo_dir: &Path) -> Vec<String> {
    log::debug!(
        "synthesize_raycast_extensions_in: scanning '{}'",
        repo_dir.display()
    );
    let mut synthesized = Vec::new();

    // Top-level Raycast extension (single-extension repo)
    if is_raycast_extension(repo_dir) {
        match synthesize_plugin_files(repo_dir) {
            Ok(name) => synthesized.push(name),
            Err(e) => log::warn!(
                "Failed to synthesize Raycast adapter for {}: {e}",
                repo_dir.display()
            ),
        }
        return synthesized;
    }

    // Otherwise look one level deep (monorepo / collection layout)
    let Ok(entries) = std::fs::read_dir(repo_dir) else {
        return synthesized;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_raycast_extension(&path) {
            match synthesize_plugin_files(&path) {
                Ok(name) => synthesized.push(name),
                Err(e) => {
                    log::warn!(
                        "Failed to synthesize Raycast adapter for {}: {e}",
                        path.display()
                    );
                }
            }
        }
    }
    synthesized
}

fn is_valid_extension_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Install a single Raycast extension from the official monorepo at
/// https://github.com/raycast/extensions by name (subdirectory under
/// `extensions/`). Uses sparse-checkout to avoid cloning the entire 6+ GB
/// repo.
///
/// `extension_name` must be a simple kebab-case name (the directory name
/// under `extensions/` in the monorepo); arbitrary URLs / paths are
/// rejected to keep this command safe.
pub async fn install_raycast_extension(
    extension_name: String,
    base_dir: PathBuf,
) -> Result<String, String> {
    if !is_valid_extension_name(&extension_name) {
        return Err(format!(
            "Invalid Raycast extension name '{extension_name}'. Use the kebab-case directory name from the raycast/extensions repo."
        ));
    }

    std::fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Failed to create plugin base dir: {e}"))?;

    let dest = base_dir.join(format!("raycast-{}", extension_name));
    if dest.exists() {
        return Err(format!(
            "Plugin directory '{}' already exists. Remove it first.",
            dest.display()
        ));
    }

    // Stage the clone in the OS temp dir, NOT under `base_dir`. Cloning
    // into `base_dir` and then removing the resulting `.git/` is fragile
    // on Windows because git writes pack files read-only and
    // `std::fs::remove_dir_all` refuses to delete them.
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!(
        "omnilauncher-raycast-{}-{}-{}",
        extension_name,
        std::process::id(),
        ts
    ));
    if stage.exists() {
        let _ = force_remove_dir_all(&stage);
    }
    let stage_str = stage.to_string_lossy().into_owned();

    // 1. Clone with no blobs + no checkout — fast, ~30 MB of metadata.
    let clone = tokio::process::Command::new("git")
        .args([
            "clone",
            "--depth=1",
            "--filter=blob:none",
            "--no-checkout",
            RAYCAST_EXTENSIONS_REPO,
            &stage_str,
        ])
        .output()
        .await
        .map_err(|e| format!("Failed to spawn git clone: {e}"))?;
    if !clone.status.success() {
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&clone.stderr).trim()
        ));
    }

    let run_git = |args: Vec<String>| -> Result<(), String> {
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(&stage)
            .output()
            .map_err(|e| format!("Failed to spawn git: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    };

    // 2. Sparse-checkout only the requested extension subdir.
    let sub_path = format!("extensions/{extension_name}");
    let checkout_result = run_git(vec![
        "sparse-checkout".into(),
        "init".into(),
        "--cone".into(),
    ])
    .and_then(|_| {
        run_git(vec![
            "sparse-checkout".into(),
            "set".into(),
            sub_path.clone(),
        ])
    })
    .and_then(|_| run_git(vec!["checkout".into()]));

    if let Err(e) = checkout_result {
        let _ = force_remove_dir_all(&stage);
        return Err(e);
    }

    let ext_src = stage.join("extensions").join(&extension_name);
    if !ext_src.is_dir() {
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "Extension '{extension_name}' not found in raycast/extensions repo."
        ));
    }

    // 3. Copy just the extension's files into the final `dest`. This
    //    leaves `.git/` behind in the temp stage — no Windows headaches.
    if let Err(e) = copy_dir_recursive(&ext_src, &dest) {
        let _ = force_remove_dir_all(&dest);
        let _ = force_remove_dir_all(&stage);
        return Err(e);
    }

    // 4. Best-effort cleanup of the temp stage. Don't fail install if it
    //    can't be removed — the OS will GC the temp dir eventually.
    let _ = force_remove_dir_all(&stage);

    let synthesized = synthesize_raycast_extensions_in(&dest);
    if synthesized.is_empty() {
        let _ = force_remove_dir_all(&dest);
        return Err(format!(
            "Downloaded '{extension_name}' but it does not look like a Raycast extension."
        ));
    }

    // Best-effort `npm install && npm run build` so dist/ is ready.
    try_build_extension(&dest);

    Ok(format!(
        "Installed Raycast extension '{extension_name}' ({} command{}).",
        synthesized.len(),
        if synthesized.len() == 1 { "" } else { "s" }
    ))
}

/// Recursively copy `src` into `dst`. Creates `dst` if missing.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create '{}': {e}", dst.display()))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read '{}': {e}", src.display()))?
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

/// Remove a directory tree, retrying after clearing read-only bits on
/// Windows. Git writes its pack files read-only, which makes plain
/// `remove_dir_all` fail; this helper handles that case.
fn force_remove_dir_all(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(first_err) => {
            #[cfg(windows)]
            {
                if clear_readonly_recursive(path).is_ok() {
                    if std::fs::remove_dir_all(path).is_ok() {
                        return Ok(());
                    }
                }
            }
            for _ in 0..3 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if std::fs::remove_dir_all(path).is_ok() {
                    return Ok(());
                }
            }
            Err(first_err)
        }
    }
}

#[cfg(windows)]
fn clear_readonly_recursive(path: &Path) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    let mut perms = meta.permissions();
    if perms.readonly() {
        perms.set_readonly(false);
        let _ = std::fs::set_permissions(path, perms);
    }
    if meta.file_type().is_dir() && !meta.file_type().is_symlink() {
        for entry in std::fs::read_dir(path)?.flatten() {
            let _ = clear_readonly_recursive(&entry.path());
        }
    }
    Ok(())
}

/// Best-effort `npm install && npm run build` so the extension's `dist/`
/// is populated. Silent on failure — the user will see a helpful message
/// in the result subtitle if dist is missing at execute time.
pub fn try_build_extension(dir: &Path) {
    if which("npm").is_none() {
        log::info!(
            "npm not found in PATH; skipping build of Raycast extension at {}",
            dir.display()
        );
        return;
    }
    log::info!(
        "Running 'npm install && npm run build' for Raycast extension at {}",
        dir.display()
    );

    let install = Command::new(npm_executable()).arg("install").current_dir(dir).output();
    match install {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            log::warn!(
                "npm install failed for {}: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return;
        }
        Err(e) => {
            log::warn!("npm install failed for {}: {e}", dir.display());
            return;
        }
    }

    let build = Command::new(npm_executable())
        .args(["run", "build"])
        .current_dir(dir)
        .output();
    match build {
        Ok(out) if out.status.success() => {
            log::info!("Built Raycast extension at {}", dir.display());
        }
        Ok(out) => {
            log::warn!(
                "npm run build failed for {}: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => log::warn!("npm run build failed for {}: {e}", dir.display()),
    }
}

fn npm_executable() -> &'static str {
    if cfg!(windows) {
        "npm.cmd"
    } else {
        "npm"
    }
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = dir.join(format!("{cmd}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ol-raycast-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn detects_raycast_extension() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("package.json"),
            r#"{
                "name": "demo",
                "title": "Demo",
                "description": "Test",
                "dependencies": {"@raycast/api": "^1.0.0"},
                "commands": [{"name": "hello", "title": "Hello"}]
            }"#,
        )
        .unwrap();
        assert!(is_raycast_extension(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_non_raycast_package() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("package.json"),
            r#"{"name": "x", "dependencies": {"react": "^18"}}"#,
        )
        .unwrap();
        assert!(!is_raycast_extension(&dir));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn synthesizes_plugin_json_and_shim() {
        let dir = tmpdir();
        std::fs::write(
            dir.join("package.json"),
            r#"{
                "name": "demo-ext",
                "title": "Demo Extension",
                "description": "Test extension",
                "icon": "🎉",
                "version": "1.2.3",
                "dependencies": {"@raycast/api": "^1.0.0"},
                "commands": [
                    {"name": "first", "title": "First", "mode": "no-view"},
                    {"name": "second", "title": "Second", "mode": "view"}
                ]
            }"#,
        )
        .unwrap();

        let name = synthesize_plugin_files(&dir).unwrap();
        assert_eq!(name, "demo-ext");
        assert!(dir.join("plugin.json").exists());
        assert!(dir.join(SHIM_FILENAME).exists());
        assert!(dir.join(API_SHIM_FILENAME).exists());

        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["name"], "demo-ext");
        assert_eq!(manifest["keyword"], "demo-ext");
        assert_eq!(manifest["entry"], SHIM_FILENAME);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_invalid_extension_name() {
        assert!(!is_valid_extension_name(""));
        assert!(!is_valid_extension_name("../etc/passwd"));
        assert!(!is_valid_extension_name("foo/bar"));
        assert!(is_valid_extension_name("hello-world"));
        assert!(is_valid_extension_name("gif_search"));
    }
}
