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
const SOURCE_LOADER_JS: &str = include_str!("../../assets/raycast-shim/raycast-source-loader.cjs");

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
        format!("{} (Raycast extension). Commands:\n{}", base_desc, descs)
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
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            log::trace!(
                "raycast::read_package: no package.json at {}: {e}",
                path.display()
            );
            return None;
        }
    };
    let trimmed = content.trim_start_matches('\u{feff}');
    serde_json::from_str(trimmed)
        .map_err(|e| {
            log::debug!(
                "raycast::read_package: failed to parse {}: {e}",
                path.display()
            );
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
    let has_commands = pkg
        .commands
        .as_ref()
        .map(|c| !c.is_empty())
        .unwrap_or(false);
    let has_api = depends_on_raycast_api(&pkg);
    log::trace!(
        "raycast::is_raycast_extension: dir={} name='{}' has_commands={} has_api={}",
        dir.display(),
        pkg.name,
        has_commands,
        has_api
    );
    has_commands && has_api
}

/// Returns Some(name) if every synthesized output file already exists
/// and is newer than package.json — i.e. nothing has changed since the
/// last synthesis. The returned name comes from the cached plugin.json
/// (whose `name` field equals package.json's `name` at synth time).
fn cached_synthesis_name(dir: &Path) -> Option<String> {
    let pkg_json = dir.join("package.json");
    let pkg_mtime = std::fs::metadata(&pkg_json).ok()?.modified().ok()?;
    let plugin_json_path = dir.join("plugin.json");
    let outputs = [
        plugin_json_path.clone(),
        dir.join(SHIM_FILENAME),
        dir.join(API_SHIM_FILENAME),
        dir.join(SOURCE_LOADER_FILENAME),
    ];
    for out in &outputs {
        let m = std::fs::metadata(out).ok()?.modified().ok()?;
        if m < pkg_mtime {
            return None;
        }
    }
    // Confirm plugin.json is one we synthesized (targets our shim).
    let body = std::fs::read_to_string(&plugin_json_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&body).ok()?;
    let entry = val.get("entry").and_then(|v| v.as_str()).unwrap_or("");
    if !entry.contains(SHIM_FILENAME) {
        return None;
    }
    val.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Write the shim files and a synthesized `plugin.json` for the Raycast
/// extension at `dir`. Idempotent: re-running on an already-synthesized
/// extension overwrites the shims (so updates pick up shim improvements)
/// but leaves a pre-existing `plugin.json` alone if it was user-authored
/// (i.e. doesn't reference our shim).
pub fn synthesize_plugin_files(dir: &Path) -> Result<String, String> {
    log::debug!("raycast::synthesize_plugin_files: dir={}", dir.display());
    // Fast-path: if all four output files exist AND every one of them is
    // newer than package.json, the cached synthesis is still valid and we
    // can skip the (expensive) JSON parse + 4 disk writes. Cuts repeat
    // cold-start cost for stable extensions to a single stat() per file.
    if let Some(name) = cached_synthesis_name(dir) {
        log::debug!(
            "raycast::synthesize_plugin_files: cache hit (mtime gate) for {}",
            dir.display()
        );
        return Ok(name);
    }

    let pkg = read_package(dir)
        .ok_or_else(|| format!("{} has no readable package.json", dir.display()))?;

    if !depends_on_raycast_api(&pkg) {
        log::debug!(
            "raycast::synthesize_plugin_files: {} missing @raycast/api dep",
            dir.display()
        );
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
    log::debug!(
        "raycast::synthesize_plugin_files: name='{}' commands={} version={:?}",
        pkg.name,
        commands.len(),
        pkg.version
    );

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
    log::debug!(
        "raycast::synthesize_plugin_files: should_write_manifest={} path={}",
        should_write_manifest,
        plugin_json_path.display()
    );

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
    log::info!(
        "raycast::install_raycast_extension: name='{}' base_dir={}",
        extension_name,
        base_dir.display()
    );
    if !is_valid_extension_name(&extension_name) {
        log::warn!(
            "raycast::install_raycast_extension: rejected invalid name '{}'",
            extension_name
        );
        return Err(format!(
            "Invalid Raycast extension name '{extension_name}'. Use the kebab-case directory name from the raycast/extensions repo."
        ));
    }

    std::fs::create_dir_all(&base_dir)
        .map_err(|e| format!("Failed to create plugin base dir: {e}"))?;

    let dest = base_dir.join(format!("raycast-{}", extension_name));
    if dest.exists() {
        log::warn!(
            "raycast::install_raycast_extension: dest already exists: {}",
            dest.display()
        );
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
    log::debug!(
        "raycast::install_raycast_extension: dest={} stage={}",
        dest.display(),
        stage.display()
    );
    if stage.exists() {
        let _ = force_remove_dir_all(&stage);
    }
    let stage_str = stage.to_string_lossy().into_owned();

    // 1. Clone with no blobs + no checkout — fast, ~30 MB of metadata.
    log::debug!(
        "raycast::install_raycast_extension: git clone --depth=1 --filter=blob:none --no-checkout {} {}",
        RAYCAST_EXTENSIONS_REPO,
        stage_str
    );
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
        let stderr = String::from_utf8_lossy(&clone.stderr);
        log::warn!(
            "raycast::install_raycast_extension: git clone failed: {}",
            stderr.trim()
        );
        let _ = force_remove_dir_all(&stage);
        return Err(format!("git clone failed: {}", stderr.trim()));
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
    log::debug!(
        "raycast::install_raycast_extension: sparse-checkout set {}",
        sub_path
    );
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
        log::warn!("raycast::install_raycast_extension: sparse-checkout failed: {e}");
        let _ = force_remove_dir_all(&stage);
        return Err(e);
    }

    let ext_src = stage.join("extensions").join(&extension_name);
    if !ext_src.is_dir() {
        log::warn!(
            "raycast::install_raycast_extension: extension '{}' not present at {}",
            extension_name,
            ext_src.display()
        );
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "Extension '{extension_name}' not found in raycast/extensions repo."
        ));
    }
    log::debug!(
        "raycast::install_raycast_extension: copying {} -> {}",
        ext_src.display(),
        dest.display()
    );

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
        log::warn!(
            "raycast::install_raycast_extension: no Raycast extensions synthesized under {}",
            dest.display()
        );
        let _ = force_remove_dir_all(&dest);
        return Err(format!(
            "Downloaded '{extension_name}' but it does not look like a Raycast extension."
        ));
    }
    log::debug!(
        "raycast::install_raycast_extension: synthesized={:?}",
        synthesized
    );

    // Best-effort `npm install && npm run build` so dist/ is ready.
    try_build_extension(&dest);

    log::info!(
        "raycast::install_raycast_extension: done name='{}' count={}",
        extension_name,
        synthesized.len()
    );
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
    for entry in
        std::fs::read_dir(src).map_err(|e| format!("Failed to read '{}': {e}", src.display()))?
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
            log::debug!(
                "raycast::force_remove_dir_all: first attempt failed for {}: {first_err}",
                path.display()
            );
            #[cfg(windows)]
            {
                if clear_readonly_recursive(path).is_ok() {
                    if std::fs::remove_dir_all(path).is_ok() {
                        log::debug!(
                            "raycast::force_remove_dir_all: succeeded after clearing readonly for {}",
                            path.display()
                        );
                        return Ok(());
                    }
                }
            }
            for attempt in 0..3 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if std::fs::remove_dir_all(path).is_ok() {
                    log::debug!(
                        "raycast::force_remove_dir_all: succeeded on retry {} for {}",
                        attempt + 1,
                        path.display()
                    );
                    return Ok(());
                }
            }
            log::warn!(
                "raycast::force_remove_dir_all: giving up on {}: {first_err}",
                path.display()
            );
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

/// Best-effort dependency install + build so the extension is runnable.
///
/// Cross-platform behavior:
///   - macOS / Linux: `npm install` (so `@raycast/api`'s postinstall sets up
///     the `ray` CLI) followed by `npm run build` → populates `dist/`.
///   - Windows: `@raycast/api`'s postinstall hook calls a relative-path bash
///     script (`bin/ray npm-post-install`) that fails on native Windows, and
///     `ray build` itself only ships Darwin/Linux binaries. We work around
///     both by installing with `--ignore-scripts` and then installing `tsx`
///     locally so the shim's source-loader fallback (`npx --no-install tsx`)
///     can run the extension's TypeScript directly at execute time.
///
/// Silent on failure — the shim emits a helpful subtitle at execute time if
/// neither `dist/` nor `tsx` ends up available.
pub fn try_build_extension(dir: &Path) {
    if which("npm").is_none() {
        log::warn!(
            "raycast::try_build_extension: npm not found in PATH; skipping build for {}",
            dir.display()
        );
        return;
    }

    let install_succeeded = run_npm_install(dir);
    if !install_succeeded {
        return;
    }

    if cfg!(windows) {
        // `ray build` is a bash script + Darwin/Linux-only binary; it cannot
        // run on native Windows. Try tsc first (typescript is always a
        // devDependency of Raycast extensions), then fall back to esbuild,
        // then tsx as a last resort.
        if !tsc_build_extension(dir) && !esbuild_extension(dir) {
            // Both compilers failed — install tsx so the source-loader
            // fallback path works at execute time.
            ensure_tsx_installed(dir);
        }
        return;
    }

    let build = Command::new(npm_executable())
        .args(["run", "build"])
        .current_dir(dir)
        .output();
    match build {
        Ok(out) if out.status.success() => {
            log::info!(
                "raycast::try_build_extension: built extension at {}",
                dir.display()
            );
        }
        Ok(out) => {
            log::warn!(
                "raycast::try_build_extension: npm run build failed for {}: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
            // Build failed — try the tsx fallback so the source loader works.
            ensure_tsx_installed(dir);
        }
        Err(e) => {
            log::warn!(
                "raycast::try_build_extension: failed to spawn npm run build for {}: {e}",
                dir.display()
            );
            ensure_tsx_installed(dir);
        }
    }
}

/// Use esbuild (via npx) to compile all Raycast command entry points listed in
/// `package.json` into `dist/<commandName>.js`. This is the Windows substitute
/// for `ray build` which only ships Darwin/Linux binaries.
///
/// Returns true if at least one command was compiled successfully.
fn esbuild_extension(dir: &Path) -> bool {
    let Some(pkg) = read_package(dir) else {
        log::warn!(
            "raycast::esbuild_extension: no package.json in {}",
            dir.display()
        );
        return false;
    };
    let Some(commands) = pkg.commands.as_ref().filter(|c| !c.is_empty()) else {
        log::warn!(
            "raycast::esbuild_extension: no commands in {}",
            dir.display()
        );
        return false;
    };

    // Collect source files that actually exist on disk.
    let src_dir = dir.join("src");
    let extensions = ["tsx", "ts", "jsx", "js"];
    let entry_files: Vec<PathBuf> = commands
        .iter()
        .flat_map(|cmd| {
            let src_dir = src_dir.clone();
            extensions
                .iter()
                .map(move |ext| src_dir.join(format!("{}.{}", cmd.name, ext)))
        })
        .filter(|p| p.is_file())
        .collect();

    if entry_files.is_empty() {
        log::warn!(
            "raycast::esbuild_extension: no source entry files found for commands in {}",
            dir.display()
        );
        return false;
    }

    // Ensure dist/ exists.
    let dist_dir = dir.join("dist");
    if let Err(e) = std::fs::create_dir_all(&dist_dir) {
        log::warn!(
            "raycast::esbuild_extension: failed to create dist/ in {}: {e}",
            dir.display()
        );
        return false;
    }

    // Build each entry file separately so each command gets its own dist/<name>.js.
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };
    let mut any_success = false;
    for entry in &entry_files {
        let file_stem = entry
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("command");
        let outfile = dist_dir.join(format!("{file_stem}.js"));
        log::debug!(
            "raycast::esbuild_extension: compiling {} -> {}",
            entry.display(),
            outfile.display()
        );
        let result = Command::new(npx)
            .args([
                "--no-install",
                "esbuild",
                &entry.to_string_lossy(),
                "--bundle",
                "--platform=node",
                "--format=cjs",
                "--external:@raycast/api",
                "--external:@raycast/utils",
                &format!("--outfile={}", outfile.to_string_lossy()),
            ])
            .current_dir(dir)
            .output();
        match result {
            Ok(out) if out.status.success() => {
                log::info!(
                    "raycast::esbuild_extension: compiled {} -> {}",
                    entry.display(),
                    outfile.display()
                );
                any_success = true;
                // Best-effort: also build a headless search bundle for this command.
                build_search_bundle(dir, npx, &dist_dir, entry, file_stem);
            }
            Ok(out) => {
                log::warn!(
                    "raycast::esbuild_extension: esbuild failed for {}: {}",
                    entry.display(),
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
            Err(e) => {
                log::warn!(
                    "raycast::esbuild_extension: failed to spawn esbuild for {}: {e}",
                    entry.display()
                );
            }
        }
    }
    any_success
}

/// Use `tsc` (via npx) to compile the extension's TypeScript source into
/// `dist/`. This is the preferred Windows build path because `typescript`
/// is always a devDependency of Raycast extensions (required by
/// `@raycast/api`) and is therefore available after `npm install`.
///
/// Passes `--project tsconfig.json` when that file exists in `dir`, and
/// always passes `--outDir dist --noEmit false` so output lands in `dist/`
/// regardless of what the tsconfig says.
///
/// Returns `true` if tsc exits successfully AND `dist/` contains at least
/// one `.js` file afterwards.
fn tsc_build_extension(dir: &Path) -> bool {
    let npx = if cfg!(windows) { "npx.cmd" } else { "npx" };

    let dist_dir = dir.join("dist");
    if let Err(e) = std::fs::create_dir_all(&dist_dir) {
        log::warn!(
            "raycast::tsc_build_extension: failed to create dist/ in {}: {e}",
            dir.display()
        );
        return false;
    }

    let mut args = vec!["--no-install".to_string(), "tsc".to_string()];

    if dir.join("tsconfig.json").is_file() {
        args.push("--project".to_string());
        args.push(dir.join("tsconfig.json").to_string_lossy().into_owned());
    }

    args.push("--outDir".to_string());
    args.push(dist_dir.to_string_lossy().into_owned());
    // NOTE: do NOT add --noEmit or false here; --noEmit is a boolean presence
    // flag and passing it at all suppresses output.

    log::debug!(
        "raycast::tsc_build_extension: running npx tsc --outDir dist for {}",
        dir.display()
    );

    let result = Command::new(npx).args(&args).current_dir(dir).output();

    match result {
        Ok(out) if out.status.success() => {
            // Verify dist/ was actually populated
            let has_js = std::fs::read_dir(&dist_dir)
                .ok()
                .map(|rd| {
                    rd.flatten()
                        .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("js"))
                })
                .unwrap_or(false);

            if has_js {
                log::info!(
                    "raycast::tsc_build_extension: compiled extension at {}",
                    dir.display()
                );
                true
            } else {
                log::warn!(
                    "raycast::tsc_build_extension: tsc succeeded but dist/ has no .js files in {}",
                    dir.display()
                );
                false
            }
        }
        Ok(out) => {
            log::warn!(
                "raycast::tsc_build_extension: tsc failed for {}: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
            false
        }
        Err(e) => {
            log::warn!(
                "raycast::tsc_build_extension: failed to spawn npx tsc for {}: {e}",
                dir.display()
            );
            false
        }
    }
}

/// Generate a headless `dist/<name>.search.js` bundle for a Raycast view command
/// so OmniLauncher's shim can perform searches without invoking React hooks.
///
/// Strategy: write a temporary entrypoint (into `dist/` with a unique name so it
/// never collides with extension source) that imports every `*.ts`/`*.tsx` file
/// from `src/utils/` and re-exports their named exports. Esbuild bundles it, then
/// we delete the temp file. The extension's own source files are never touched.
fn build_search_bundle(dir: &Path, npx: &str, dist_dir: &Path, _entry: &Path, file_stem: &str) {
    let src_utils = dir.join("src").join("utils");
    if !src_utils.is_dir() {
        return;
    }

    // Collect all TS/TSX utility files.
    let util_files: Vec<PathBuf> = match std::fs::read_dir(&src_utils) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| matches!(e, "ts" | "tsx"))
                        .unwrap_or(false)
            })
            .collect(),
        Err(_) => return,
    };

    if util_files.is_empty() {
        return;
    }

    // Build the temp entrypoint source: re-export everything from each util file
    // using relative paths from src/utils/ to the temp file location.
    let temp_entry_name = format!("__omni_search_entry_{file_stem}.mts");
    let temp_entry = src_utils.join(&temp_entry_name);

    let mut exports_src = String::from(
        "// OmniLauncher-generated headless search entrypoint — not part of the extension source\n",
    );
    for f in &util_files {
        if let Some(stem) = f.file_stem().and_then(|s| s.to_str()) {
            exports_src.push_str(&format!("export * from \"./{stem}\";\n"));
        }
    }

    if let Err(e) = std::fs::write(&temp_entry, &exports_src) {
        log::warn!("raycast::build_search_bundle: failed to write temp entry for {file_stem}: {e}");
        return;
    }

    let outfile = dist_dir.join(format!("{file_stem}.search.js"));
    let result = Command::new(npx)
        .args([
            "--no-install",
            "esbuild",
            &temp_entry.to_string_lossy(),
            "--bundle",
            "--platform=node",
            "--format=cjs",
            "--external:@raycast/api",
            "--external:@raycast/utils",
            &format!("--outfile={}", outfile.to_string_lossy()),
        ])
        .current_dir(dir)
        .output();

    // Always clean up the temp file regardless of build outcome.
    let _ = std::fs::remove_file(&temp_entry);

    match result {
        Ok(out) if out.status.success() => {
            log::info!(
                "raycast::build_search_bundle: built headless search bundle for '{file_stem}' at {}",
                outfile.display()
            );
        }
        Ok(out) => {
            log::debug!(
                "raycast::build_search_bundle: esbuild failed for {file_stem} (non-fatal): {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            log::debug!(
                "raycast::build_search_bundle: spawn failed for {file_stem} (non-fatal): {e}"
            );
        }
    }
}

/// Run `npm install` for a Raycast extension. On Windows, uses
/// `--ignore-scripts` from the start since `@raycast/api`'s postinstall calls
/// a bash script that always fails on native Windows. Returns true if
/// `node_modules/` is populated afterward.
fn run_npm_install(dir: &Path) -> bool {
    log::info!(
        "raycast::run_npm_install: starting for {} (ignore_scripts={})",
        dir.display(),
        cfg!(windows)
    );

    let mut cmd = Command::new(npm_executable());
    cmd.arg("install");
    if cfg!(windows) {
        cmd.arg("--ignore-scripts");
    }
    let first = cmd.current_dir(dir).output();

    match first {
        Ok(out) if out.status.success() => {
            log::info!("raycast::run_npm_install: ok for {}", dir.display());
            return true;
        }
        Ok(out) => {
            log::warn!(
                "raycast::run_npm_install: npm install failed for {} (status={:?}): {}",
                dir.display(),
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Err(e) => {
            log::warn!(
                "raycast::run_npm_install: failed to spawn npm install for {}: {e}",
                dir.display()
            );
            return false;
        }
    }

    // Retry with --ignore-scripts on non-Windows (Windows already passed it).
    if !cfg!(windows) {
        log::info!(
            "raycast::run_npm_install: retrying with --ignore-scripts for {}",
            dir.display()
        );
        let retry = Command::new(npm_executable())
            .args(["install", "--ignore-scripts"])
            .current_dir(dir)
            .output();
        match retry {
            Ok(out) if out.status.success() => {
                log::info!(
                    "raycast::run_npm_install: retry with --ignore-scripts ok for {}",
                    dir.display()
                );
                return true;
            }
            Ok(out) => log::warn!(
                "raycast::run_npm_install: retry failed for {}: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(e) => log::warn!(
                "raycast::run_npm_install: retry spawn failed for {}: {e}",
                dir.display()
            ),
        }
    }

    // Even on failure, node_modules may have been populated (most failures
    // are in postinstall scripts after deps are already on disk).
    dir.join("node_modules").is_dir()
}

/// Install `tsx` locally (no save) so `npx --no-install tsx` resolves at
/// execute time. Used as a fallback when `npm run build` doesn't produce
/// `dist/` (notably on Windows, where Raycast's `ray build` doesn't run).
fn ensure_tsx_installed(dir: &Path) {
    let tsx_dir = dir.join("node_modules").join("tsx");
    if tsx_dir.is_dir() {
        log::debug!(
            "raycast::ensure_tsx_installed: tsx already present in {}",
            dir.display()
        );
        return;
    }
    log::info!(
        "raycast::ensure_tsx_installed: installing tsx as source-loader fallback in {}",
        dir.display()
    );
    let out = Command::new(npm_executable())
        .args(["install", "--no-save", "--ignore-scripts", "tsx"])
        .current_dir(dir)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            log::info!(
                "raycast::ensure_tsx_installed: tsx ready in {}",
                dir.display()
            );
        }
        Ok(o) => log::warn!(
            "raycast::ensure_tsx_installed: failed for {}: {}",
            dir.display(),
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => log::warn!(
            "raycast::ensure_tsx_installed: spawn failed for {}: {e}",
            dir.display()
        ),
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

    #[test]
    fn tsc_build_extension_populates_dist() {
        // This test requires node/npm/npx in PATH. Skip gracefully if not present.
        if which("npx").is_none() {
            return;
        }

        let dir = tmpdir();

        // Minimal package.json with typescript devDep and one command
        std::fs::write(
            dir.join("package.json"),
            r#"{
                "name": "tsc-test-ext",
                "description": "test",
                "dependencies": {"@raycast/api": "^1.39.0"},
                "devDependencies": {"typescript": "^4.4.3"},
                "commands": [{"name": "hello", "title": "Hello"}]
            }"#,
        )
        .unwrap();

        // Minimal tsconfig.json
        std::fs::write(
            dir.join("tsconfig.json"),
            r#"{"compilerOptions": {"module": "commonjs", "target": "es2021", "outDir": "dist", "skipLibCheck": true, "jsx": "react-jsx", "esModuleInterop": true},"include": ["src/**/*"]}"#,
        )
        .unwrap();

        // Minimal TypeScript source
        std::fs::create_dir(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("src").join("hello.tsx"),
            r#"export function run() { console.log("hello"); }"#,
        )
        .unwrap();

        // npm install to get typescript into node_modules
        let install = std::process::Command::new(npm_executable())
            .args(["install", "--ignore-scripts"])
            .current_dir(&dir)
            .output()
            .expect("npm install failed to spawn");
        if !install.status.success() {
            // Can't install — skip test
            std::fs::remove_dir_all(&dir).ok();
            return;
        }

        let result = tsc_build_extension(&dir);
        assert!(result, "tsc_build_extension should return true");

        let dist = dir.join("dist");
        assert!(dist.is_dir(), "dist/ should exist after tsc build");

        let has_js = std::fs::read_dir(&dist)
            .unwrap()
            .flatten()
            .any(|e| e.path().extension().and_then(|x| x.to_str()) == Some("js"));
        assert!(has_js, "dist/ should contain at least one .js file");

        std::fs::remove_dir_all(&dir).ok();
    }
}
