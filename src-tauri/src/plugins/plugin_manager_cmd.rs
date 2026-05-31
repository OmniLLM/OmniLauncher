use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::external::{discover_plugins_in_repo, ext_plugins_dir, load_manifest};
use crate::gh_helper;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn ensure_ext_plugins_dir() -> Result<PathBuf, String> {
    let dir = plugin_storage_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create ext-plugins directory: {e}"))?;
    Ok(dir)
}

/// Remove a directory tree, retrying on Windows after clearing read-only
/// attributes. Git pack files inside `.git/objects/pack` are written with the
/// read-only bit set, which causes `std::fs::remove_dir_all` to fail with
/// "Access is denied" on Windows. We walk the tree once to strip read-only
/// flags and then retry the removal.
fn force_remove_dir_all(path: &std::path::Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(first_err) => {
            #[cfg(windows)]
            {
                if clear_readonly_recursive(path).is_ok() {
                    if let Ok(()) = std::fs::remove_dir_all(path) {
                        return Ok(());
                    }
                }
            }
            // Exponential backoff (~2s total) to handle transient file-handle
            // holds (e.g. AV scanners, slow handle release on Windows).
            for delay_ms in [100u64, 200, 400, 800, 1500] {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                if std::fs::remove_dir_all(path).is_ok() {
                    return Ok(());
                }
            }
            Err(first_err)
        }
    }
}

#[cfg(windows)]
fn clear_readonly_recursive(path: &std::path::Path) -> std::io::Result<()> {
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
/// https://github.com/o/r/tree/main/extensions/foo  →  "foo"
fn dir_name_from_source(source: &str) -> String {
    if let Some(parsed) = parse_github_subdir_url(source) {
        return parsed.leaf_name;
    }
    let base = source.trim_end_matches('/').trim_end_matches(".git");

    base.rsplit(['/', '\\', ':'])
        .next()
        .unwrap_or("plugin")
        .to_string()
}

/// Parsed components of a GitHub-style subdirectory URL (the kind GitHub and
/// GitHub Enterprise show when you click into a folder):
/// `<host>/<owner>/<repo>/tree/<branch>/<subpath>` or `.../blob/<branch>/...`.
/// Such URLs are not directly clone-able and must be translated into a
/// sparse-checkout of `<subpath>`. Works for `github.com` as well as
/// self-hosted GitHub Enterprise instances (e.g. `ghe.example.com`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubSubdirUrl {
    host: String,
    owner: String,
    repo: String,
    clone_url: String,
    branch: String,
    subpath: String,
    leaf_name: String,
}

fn parse_github_subdir_url(source: &str) -> Option<GithubSubdirUrl> {
    let s = source.trim().trim_end_matches('/');
    let (scheme, rest) = if let Some(r) = s.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = s.strip_prefix("http://") {
        ("http", r)
    } else {
        return None;
    };
    // host / owner / repo / kind / branch / subpath...
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 6 {
        return None;
    }
    let host = parts[0];
    let owner = parts[1];
    let repo = parts[2].trim_end_matches(".git");
    let kind = parts[3];
    if host.is_empty() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    if kind != "tree" && kind != "blob" {
        return None;
    }
    // GitLab and Bitbucket expose folder URLs under a `/-/tree/…` path, so a
    // bare `/tree/…` on those forges is not a clone-able subdir URL.
    let host_lower = host.to_lowercase();
    if host_lower == "gitlab.com" || host_lower == "bitbucket.org" {
        return None;
    }
    let branch = parts[4];
    let subpath_parts = &parts[5..];
    if branch.is_empty() || subpath_parts.is_empty() {
        return None;
    }
    let subpath = subpath_parts.join("/");
    let leaf_name = subpath_parts
        .last()
        .copied()
        .unwrap_or(repo)
        .trim_end_matches(".git")
        .to_string();
    Some(GithubSubdirUrl {
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
        clone_url: format!("{scheme}://{host}/{owner}/{repo}.git"),
        branch: branch.to_string(),
        subpath,
        leaf_name,
    })
}

/// Extract `subdir.subpath` out of `subdir.clone_url@subdir.branch` into
/// `dest`, trying installers in priority order: `git` sparse-checkout first,
/// then `gh repo clone` (for private / GitHub Enterprise repos the user is
/// authenticated against), then a `curl` archive download as a last resort.
async fn sparse_checkout_subdir(subdir: &GithubSubdirUrl, dest: &PathBuf) -> Result<(), String> {
    match git_sparse_checkout_subdir(subdir, dest).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            log::warn!("sparse_checkout_subdir: git failed: {e}; trying gh");
            if dest.exists() {
                let _ = force_remove_dir_all(dest);
            }
        }
    }

    match gh_checkout_subdir(subdir, dest).await {
        Ok(()) => return Ok(()),
        Err(e) => {
            log::warn!("sparse_checkout_subdir: gh failed: {e}; trying curl");
            if dest.exists() {
                let _ = force_remove_dir_all(dest);
            }
        }
    }

    match curl_checkout_subdir(subdir, dest).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if dest.exists() {
                let _ = force_remove_dir_all(dest);
            }
            Err(format!(
                "Failed to fetch subdirectory '{}' from {}@{} (git, gh, and curl all failed): {e}",
                subdir.subpath, subdir.clone_url, subdir.branch
            ))
        }
    }
}

/// `gh repo clone` the full repo into a temp stage, then copy the requested
/// subpath into `dest`. Only attempted when `gh` is on PATH.
async fn gh_checkout_subdir(subdir: &GithubSubdirUrl, dest: &PathBuf) -> Result<(), String> {
    if !gh_helper::is_gh_available() {
        return Err("gh CLI not available".to_string());
    }
    let repo = gh_helper::GithubRepoRef {
        host: subdir.host.clone(),
        owner: subdir.owner.clone(),
        repo: subdir.repo.clone(),
    };
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!(
        "omnilauncher-ghsubdir-{}-{}-{}",
        subdir.leaf_name,
        std::process::id(),
        ts
    ));
    if stage.exists() {
        let _ = force_remove_dir_all(&stage);
    }

    gh_helper::gh_clone(&repo, &stage, &["--depth=1", "--branch", &subdir.branch]).await?;

    let src = stage.join(&subdir.subpath);
    if !src.is_dir() {
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "Subdirectory '{}' not found in {}@{}.",
            subdir.subpath, subdir.clone_url, subdir.branch
        ));
    }
    let copy_result = copy_dir_recursive(&src, dest);
    let _ = force_remove_dir_all(&stage);
    copy_result
}

/// `curl` the repo archive tarball for the branch, extract it, then copy the
/// requested subpath into `dest`. Last-resort fallback when both git and gh
/// are unavailable or fail.
async fn curl_checkout_subdir(subdir: &GithubSubdirUrl, dest: &PathBuf) -> Result<(), String> {
    let (stage, inner) = curl_fetch_archive(
        &subdir.host,
        &subdir.owner,
        &subdir.repo,
        Some(&subdir.branch),
    )
    .await?;
    let src = inner.join(&subdir.subpath);
    if !src.is_dir() {
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "Subdirectory '{}' not found in {}@{}.",
            subdir.subpath, subdir.clone_url, subdir.branch
        ));
    }
    let copy_result = copy_dir_recursive(&src, dest);
    let _ = force_remove_dir_all(&stage);
    copy_result
}

/// Download a GitHub archive tarball via `curl` and extract it. Tries `branch`
/// when given, otherwise `main` then `master`. Requires `curl` and `tar` on
/// PATH (both ship with Windows 10+, macOS, and most modern Linux distros).
/// Returns `(stage_dir, extracted_repo_dir)`; the caller owns cleanup of
/// `stage_dir` (which contains `extracted_repo_dir`).
async fn curl_fetch_archive(
    host: &str,
    owner: &str,
    repo: &str,
    branch: Option<&str>,
) -> Result<(PathBuf, PathBuf), String> {
    let candidates: Vec<&str> = match branch {
        Some(b) => vec![b],
        None => vec!["main", "master"],
    };

    let mut last_err = String::new();
    for b in candidates {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let stage = std::env::temp_dir().join(format!(
            "omnilauncher-curl-{}-{}-{}-{}",
            repo,
            b.replace(['/', '\\'], "_"),
            std::process::id(),
            ts
        ));
        if stage.exists() {
            let _ = force_remove_dir_all(&stage);
        }
        std::fs::create_dir_all(&stage)
            .map_err(|e| format!("Failed to create curl stage dir: {e}"))?;

        let tarball = stage.join("archive.tar.gz");
        let tarball_str = tarball.to_string_lossy().into_owned();
        let url = format!("https://{host}/{owner}/{repo}/archive/refs/heads/{b}.tar.gz");

        log::info!("curl_fetch_archive: curl -fsSL {url} -> {tarball_str}");
        let dl = tokio::process::Command::new("curl")
            .args(["-fsSL", "-o", &tarball_str, &url])
            .output()
            .await
            .map_err(|e| format!("Failed to spawn curl: {e}"))?;
        if !dl.status.success() {
            last_err = format!(
                "curl download failed (branch '{b}'): {}",
                String::from_utf8_lossy(&dl.stderr).trim()
            );
            let _ = force_remove_dir_all(&stage);
            continue;
        }

        let stage_str = stage.to_string_lossy().into_owned();
        let extract = tokio::process::Command::new("tar")
            .args(["-xzf", &tarball_str, "-C", &stage_str])
            .output()
            .await
            .map_err(|e| format!("Failed to spawn tar: {e}"))?;
        if !extract.status.success() {
            last_err = format!(
                "tar extract failed: {}",
                String::from_utf8_lossy(&extract.stderr).trim()
            );
            let _ = force_remove_dir_all(&stage);
            continue;
        }

        // GitHub archives extract to a single `<repo>-<branch>` top-level dir.
        let inner = std::fs::read_dir(&stage)
            .map_err(|e| format!("Failed to read extracted archive: {e}"))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .find(|p| p.is_dir());
        match inner {
            Some(p) => return Ok((stage, p)),
            None => {
                last_err = "extracted archive contained no directory".to_string();
                let _ = force_remove_dir_all(&stage);
            }
        }
    }

    Err(if last_err.is_empty() {
        "curl archive download failed".to_string()
    } else {
        last_err
    })
}

/// Sparse-checkout `subdir.subpath` out of `subdir.clone_url@subdir.branch`
/// into a temp stage, then copy that single folder's contents into `dest`.
async fn git_sparse_checkout_subdir(
    subdir: &GithubSubdirUrl,
    dest: &PathBuf,
) -> Result<(), String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!(
        "omnilauncher-subdir-{}-{}-{}",
        subdir.leaf_name,
        std::process::id(),
        ts
    ));
    log::debug!(
        "sparse_checkout_subdir: stage='{}' dest='{}'",
        stage.display(),
        dest.display()
    );
    if stage.exists() {
        log::debug!("sparse_checkout_subdir: clearing pre-existing stage");
        let _ = force_remove_dir_all(&stage);
    }
    let stage_str = stage.to_string_lossy().into_owned();

    log::debug!(
        "sparse_checkout_subdir: git clone --depth=1 --filter=blob:none --no-checkout --branch {} {} {}",
        subdir.branch, subdir.clone_url, stage_str
    );

    let clone = tokio::process::Command::new("git")
        .args([
            "clone",
            "--depth=1",
            "--filter=blob:none",
            "--no-checkout",
            "--branch",
            &subdir.branch,
            &subdir.clone_url,
            &stage_str,
        ])
        .envs(
            gh_helper::gh_token_for_host(&subdir.host)
                .map(|t| gh_helper::git_auth_env(&subdir.host, &t))
                .unwrap_or_default(),
        )
        .output()
        .await
        .map_err(|e| format!("Failed to spawn git: {e}"))?;
    if !clone.status.success() {
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&clone.stderr).trim()
        ));
    }

    let run_git = |args: Vec<String>| -> Result<(), String> {
        let out = Command::new("git")
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

    let checkout = run_git(vec![
        "sparse-checkout".into(),
        "init".into(),
        "--cone".into(),
    ])
    .and_then(|_| {
        run_git(vec![
            "sparse-checkout".into(),
            "set".into(),
            subdir.subpath.clone(),
        ])
    })
    .and_then(|_| run_git(vec!["checkout".into()]));

    if let Err(e) = checkout {
        let _ = force_remove_dir_all(&stage);
        return Err(e);
    }

    let src = stage.join(&subdir.subpath);
    if !src.is_dir() {
        let _ = force_remove_dir_all(&stage);
        return Err(format!(
            "Subdirectory '{}' not found in {}@{}.",
            subdir.subpath, subdir.clone_url, subdir.branch
        ));
    }

    log::debug!(
        "sparse_checkout_subdir: copying {} -> {}",
        src.display(),
        dest.display()
    );
    let copy_result = copy_dir_recursive(&src, dest);
    let _ = force_remove_dir_all(&stage);
    copy_result
}

// DEPRECATED: remove after v1.0 — migration shim for users upgrading from the pre-collection split layout
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
        log::info!(
            "legacy_split_collection used for repo '{repo_name}' — this shim will be removed in a future release"
        );
        let collection = "OmniLLM/omnilauncher-plugins".to_string();
        return Some((
            collection.clone(),
            collection,
            "https://github.com/OmniLLM/omnilauncher-plugins.git".to_string(),
        ));
    }

    None
}

// DEPRECATED: remove after v1.0 — migration shim for users upgrading from the pre-collection split layout
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
    log::info!(
        "install_plugin: source='{}' target_dir={:?}",
        source,
        target_dir
    );
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
    log::debug!(
        "install_plugin: derived dir_name='{}' dest='{}'",
        dir_name,
        dest.display()
    );

    if dest.exists() {
        // If the existing directory has no discoverable plugins (an orphan from
        // a failed previous install), clean it up automatically so the user can
        // retry without manual intervention.
        let is_orphan = discover_plugins_in_repo(&dest).is_empty();
        if is_orphan {
            force_remove_dir_all(&dest).map_err(|e| {
                format!(
                    "Plugin directory '{}' already exists and could not be cleaned up: {e}",
                    dir_name
                )
            })?;
        } else {
            return Err(format!(
                "Plugin directory '{}' already exists. Remove it first with remove_plugin.",
                dir_name
            ));
        }
    }

    let is_gh_shorthand = !source.starts_with('/')
        && !source.starts_with('.')
        && !source.starts_with('~')
        && !source.contains("://")
        && !source.starts_with("git@")
        && !source.contains(' ')
        && source.matches('/').count() == 1
        && gh_helper::parse_github_repo(&source).is_some();
    let is_remote = source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
        || is_gh_shorthand;

    if let Some(subdir) = parse_github_subdir_url(&source) {
        // GitHub `tree/<branch>/<subpath>` URLs are not clone-able. Sparse-
        // checkout the requested subpath into a temp stage and copy it into
        // the final destination.
        log::info!(
            "install_plugin: detected GitHub subdir URL repo='{}' branch='{}' subpath='{}'",
            subdir.clone_url,
            subdir.branch,
            subdir.subpath
        );
        sparse_checkout_subdir(&subdir, &dest).await?;
    } else if is_remote {
        // Priority: git → gh → curl. Plain `git clone` works for all public
        // repos with no extra dependencies, so try it first. Fall back to `gh
        // repo clone` when git fails — typically because the repo is private or
        // on GitHub Enterprise and the user is authenticated via `gh auth
        // login`. As a last resort, download the repo archive tarball with
        // `curl` (+ `tar`) for hosts where neither git nor gh is configured.
        let dest_str = dest.to_string_lossy().into_owned();
        let mut cloned = false;
        let mut git_err: Option<String> = None;

        // For gh shorthand (`owner/repo`), git won't accept the bare form, so
        // resolve it to a full clone URL via gh's parser before invoking git.
        let git_source: String = match gh_helper::parse_github_repo(&source) {
            Some(repo) if is_gh_shorthand => repo.clone_url(),
            _ => source.clone(),
        };

        log::info!(
            "install_plugin: git clone --depth=1 {} -> {}",
            git_source,
            dest_str
        );
        let mut git_cmd = tokio::process::Command::new("git");
        git_cmd.args(["clone", "--depth=1", &git_source, &dest_str]);
        // When multiple gh accounts/hosts are configured, authenticate git as
        // the account gh considers active for this host.
        if let Some(repo) = gh_helper::parse_github_repo(&source) {
            if let Some(token) = gh_helper::gh_token_for_host(&repo.host) {
                for (k, v) in gh_helper::git_auth_env(&repo.host, &token) {
                    git_cmd.env(k, v);
                }
            }
        }
        let output = git_cmd
            .output()
            .await
            .map_err(|e| format!("Failed to spawn git: {e}"))?;

        if output.status.success() {
            cloned = true;
        } else {
            git_err = Some(String::from_utf8_lossy(&output.stderr).into_owned());
            // Clean up partial clone before gh retry.
            if dest.exists() {
                let _ = force_remove_dir_all(&dest);
            }
        }

        if !cloned {
            if let Some(repo) = gh_helper::parse_github_repo(&source) {
                if gh_helper::is_gh_available() {
                    log::info!(
                        "install_plugin: git failed, trying gh repo clone {}/{} -> {} (host={})",
                        repo.owner,
                        repo.repo,
                        dest_str,
                        repo.host
                    );
                    match gh_helper::gh_clone(&repo, &dest, &["--depth=1"]).await {
                        Ok(()) => cloned = true,
                        Err(e) => {
                            log::warn!("install_plugin: gh clone also failed: {e}");
                            if dest.exists() {
                                let _ = force_remove_dir_all(&dest);
                            }
                        }
                    }
                }
            }
        }

        if !cloned {
            if let Some(repo) = gh_helper::parse_github_repo(&source) {
                log::info!(
                    "install_plugin: git and gh failed, trying curl archive download {}/{} -> {}",
                    repo.owner,
                    repo.repo,
                    dest_str
                );
                match curl_fetch_archive(&repo.host, &repo.owner, &repo.repo, None).await {
                    Ok((stage, inner)) => {
                        let copy_result = copy_dir_recursive(&inner, &dest);
                        let _ = force_remove_dir_all(&stage);
                        match copy_result {
                            Ok(()) => cloned = true,
                            Err(e) => {
                                log::warn!("install_plugin: curl copy failed: {e}");
                                if dest.exists() {
                                    let _ = force_remove_dir_all(&dest);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!("install_plugin: curl fallback also failed: {e}");
                        if dest.exists() {
                            let _ = force_remove_dir_all(&dest);
                        }
                    }
                }
            }
        }

        if !cloned {
            return Err(format!(
                "git clone failed: {}",
                git_err.unwrap_or_else(|| "unknown error".to_string())
            ));
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

    if super::flow::clean_generated_adapter_files_in(&target)? {
        log::info!(
            "update_plugin: cleaned generated Flow adapter files for '{}' before pull",
            name
        );
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
    log::info!("update_plugin: pulled '{}'\n{}", name, pull_result.trim());

    // Re-run foreign-format synthesis + dependency setup so updates to
    // Raycast / Flow.Launcher source picks up new commands and rebuilds dist/.
    let raycast_synth = super::raycast::synthesize_raycast_extensions_in(&target);
    let flow_synth = super::flow::synthesize_flow_plugins_in(&target);
    log::debug!(
        "update_plugin: post-pull synth raycast={:?} flow={:?}",
        raycast_synth,
        flow_synth
    );
    if !raycast_synth.is_empty() {
        super::raycast::try_build_extension(&target);
    }
    if !flow_synth.is_empty() {
        super::flow::try_setup_dependencies(&target)?;
    }

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

    let is_gh_shorthand = !source.starts_with('/')
        && !source.starts_with('.')
        && !source.starts_with('~')
        && !source.contains("://")
        && !source.starts_with("git@")
        && !source.contains(' ')
        && source.matches('/').count() == 1
        && gh_helper::parse_github_repo(&source).is_some();
    let is_remote = source.starts_with("http://")
        || source.starts_with("https://")
        || source.starts_with("git@")
        || is_gh_shorthand;

    if is_remote {
        let stage_str = stage.to_string_lossy().into_owned();

        // Priority: git → gh. Same logic as install_plugin: try plain
        // `git clone` first, fall back to `gh repo clone` only if git fails
        // (private/GHE repo where the user has gh auth).
        let mut cloned = false;
        let mut git_err: Option<String> = None;

        let git_source: String = match gh_helper::parse_github_repo(&source) {
            Some(repo) if is_gh_shorthand => repo.clone_url(),
            _ => source.clone(),
        };

        log::info!(
            "update_plugin_collection: git clone --depth=1 {} -> {}",
            git_source,
            stage_str
        );
        let mut git_cmd = tokio::process::Command::new("git");
        git_cmd.args(["clone", "--depth=1", &git_source, &stage_str]);
        // Authenticate as the gh account active for this host when several
        // gh accounts/hosts are configured.
        if let Some(repo) = gh_helper::parse_github_repo(&source) {
            if let Some(token) = gh_helper::gh_token_for_host(&repo.host) {
                for (k, v) in gh_helper::git_auth_env(&repo.host, &token) {
                    git_cmd.env(k, v);
                }
            }
        }
        let output = git_cmd
            .output()
            .await
            .map_err(|e| format!("Failed to spawn git: {e}"))?;

        if output.status.success() {
            cloned = true;
        } else {
            git_err = Some(String::from_utf8_lossy(&output.stderr).into_owned());
            if stage.exists() {
                let _ = force_remove_dir_all(&stage);
            }
        }

        if !cloned {
            if let Some(repo) = gh_helper::parse_github_repo(&source) {
                if gh_helper::is_gh_available() {
                    log::info!(
                        "update_plugin_collection: git failed, trying gh repo clone {}/{} -> {}",
                        repo.owner,
                        repo.repo,
                        stage_str
                    );
                    match gh_helper::gh_clone(&repo, &stage, &["--depth=1"]).await {
                        Ok(()) => cloned = true,
                        Err(e) => {
                            log::warn!("update_plugin_collection: gh clone also failed: {e}");
                            if stage.exists() {
                                let _ = force_remove_dir_all(&stage);
                            }
                        }
                    }
                }
            }
        }

        if !cloned {
            return Err(format!(
                "git clone failed: {}",
                git_err.unwrap_or_else(|| "unknown error".to_string())
            ));
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
    log::debug!("install_staged_plugin: dest='{}'", dest.display());
    // Materialize plugin.json for foreign plugin formats (Raycast extensions,
    // Flow.Launcher plugins) before discovery so they show up like native
    // OmniLauncher plugins. Both calls are no-ops on directories that don't
    // look like the corresponding format.
    let raycast_synth = super::raycast::synthesize_raycast_extensions_in(dest);
    let (flow_synth, flow_errors) = super::flow::synthesize_flow_plugins_in_with_errors(dest);
    log::debug!(
        "install_staged_plugin: raycast_synthesized={:?} flow_synthesized={:?}",
        raycast_synth,
        flow_synth
    );
    if !raycast_synth.is_empty() {
        super::raycast::try_build_extension(dest);
    }
    if !flow_synth.is_empty() {
        if let Err(error) = super::flow::try_setup_dependencies(dest) {
            let _ = force_remove_dir_all(dest);
            return Err(error);
        }
    }

    let discovered = discover_plugins_in_repo(dest);
    log::debug!(
        "install_staged_plugin: discovered {} plugin(s): {:?}",
        discovered.len(),
        discovered.iter().map(|(_, m)| &m.name).collect::<Vec<_>>()
    );
    if discovered.is_empty() {
        let _ = force_remove_dir_all(dest);
        if !flow_errors.is_empty() {
            return Err(flow_errors.join("\n"));
        }
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

/// Return the names of all currently quarantined external plugins.
///
/// A plugin is quarantined after 5 consecutive failures (see
/// `ExternalPlugin`); quarantine is cleared by `reload_external_plugins`.
pub fn list_quarantined_plugins(pm: &super::PluginManager) -> Vec<String> {
    pm.quarantined_plugin_names()
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
                let is_orphan = discovered.is_empty();

                let (collection_name, collection_key) = if is_orphan {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    (name.clone(), name)
                } else {
                    infer_collection_identity(&path, &base, discovered.len())
                };
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
                    "is_orphan": is_orphan,
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
        .or_else(|_| force_remove_dir_all(&target))
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

// ─── Collection grouping (backend-owned) ───────────────────────────────────────
//
// These types and helpers were ported from the React frontend
// (`src/components/PluginManager.tsx`), which previously re-shaped the raw
// `list_plugins()` JSON into collections in the browser. The backend now owns
// this domain logic and hands the UI ready-to-render `PluginCollectionInfo`s.

use serde::Serialize;

/// One plugin within a collection, tagged with the repo it came from.
#[derive(Debug, Clone, Serialize)]
pub struct GroupedPluginInfo {
    #[serde(flatten)]
    pub manifest: serde_json::Value,
    pub repo_dir_name: String,
    pub repo_is_git_repo: bool,
    pub repo_git_remote: Option<String>,
}

/// A collection groups one or more plugin repos that share an origin (git
/// remote, or a multi-plugin repo / parent folder).
#[derive(Debug, Clone, Serialize)]
pub struct PluginCollectionInfo {
    pub key: String,
    pub name: String,
    pub has_git_repo: bool,
    pub collection_source: Option<String>,
    pub repos: Vec<serde_json::Value>,
    pub plugins: Vec<GroupedPluginInfo>,
}

/// Result of a collection-wide update/remove: which repos succeeded, which
/// failed, and a human-readable summary message for the UI to display.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionOpResult {
    pub updated: Vec<String>,
    pub failed: Vec<String>,
    pub message: String,
}

/// Reduce a git remote URL to its `owner/repo` form. Handles SSH
/// (`git@host:owner/repo.git`), HTTPS URLs, and bare paths. Mirrors the former
/// `normalizeGitRemote` in PluginManager.tsx.
pub fn normalize_git_remote(remote: Option<&str>) -> Option<String> {
    let remote = remote?.trim();
    // Strip a case-insensitive ".git" suffix.
    let trimmed = if remote.to_ascii_lowercase().ends_with(".git") {
        &remote[..remote.len() - 4]
    } else {
        remote
    };

    let last_two = |segments: &[&str]| -> Option<String> {
        let segs: Vec<&str> = segments.iter().filter(|s| !s.is_empty()).copied().collect();
        if segs.len() >= 2 {
            Some(format!("{}/{}", segs[segs.len() - 2], segs[segs.len() - 1]))
        } else {
            None
        }
    };

    // SSH form: git@host:owner/repo
    if let Some(colon_idx) = trimmed.find(':') {
        if trimmed[..colon_idx].contains('@') {
            let path = &trimmed[colon_idx + 1..];
            let segs: Vec<&str> = path.split('/').collect();
            if let Some(r) = last_two(&segs) {
                return Some(r);
            }
        }
    }

    // URL form
    if let Some(scheme_idx) = trimmed.find("://") {
        let after = &trimmed[scheme_idx + 3..];
        if let Some(slash) = after.find('/') {
            let path = &after[slash + 1..];
            let segs: Vec<&str> = path.split('/').collect();
            if let Some(r) = last_two(&segs) {
                return Some(r);
            }
        }
    }

    // Bare path fallback
    let segs: Vec<&str> = trimmed.split(['/', '\\']).collect();
    last_two(&segs)
}

fn json_str(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn json_bool(v: &serde_json::Value, key: &str) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(false)
}

/// Display name for a collection: normalized git remote, else collection name,
/// else dir name. Mirrors `collectionDisplayName` in the frontend.
fn collection_display_name(repo: &serde_json::Value) -> String {
    normalize_git_remote(json_str(repo, "git_remote").as_deref())
        .or_else(|| json_str(repo, "collection_name"))
        .or_else(|| json_str(repo, "dir_name"))
        .unwrap_or_default()
}

/// Grouping key for a collection: remote-first, else collection_key, else
/// collection_name, else dir_name. Mirrors `collectionGroupKey`.
fn collection_group_key(repo: &serde_json::Value) -> String {
    if let Some(remote) = normalize_git_remote(json_str(repo, "git_remote").as_deref()) {
        return format!("remote:{remote}");
    }
    json_str(repo, "collection_key")
        .or_else(|| json_str(repo, "collection_name"))
        .or_else(|| json_str(repo, "dir_name"))
        .unwrap_or_default()
}

/// List installed plugins grouped into collections, ready for the UI to render.
pub fn list_plugin_collections() -> Vec<PluginCollectionInfo> {
    let repos = list_plugins();
    let mut order: Vec<String> = Vec::new();
    let mut grouped: std::collections::HashMap<String, PluginCollectionInfo> =
        std::collections::HashMap::new();

    for repo in repos {
        let key = collection_group_key(&repo);
        let name = collection_display_name(&repo);
        let is_git = json_bool(&repo, "is_git_repo");
        let source = json_str(&repo, "collection_source");
        let dir_name = json_str(&repo, "dir_name").unwrap_or_default();
        let git_remote = json_str(&repo, "git_remote");

        let entry = grouped.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            PluginCollectionInfo {
                key: key.clone(),
                name,
                has_git_repo: false,
                collection_source: None,
                repos: Vec::new(),
                plugins: Vec::new(),
            }
        });

        if entry.collection_source.is_none() {
            entry.collection_source = source;
        }
        entry.has_git_repo = entry.has_git_repo || is_git;

        if let Some(plugins) = repo.get("plugins").and_then(|p| p.as_array()) {
            for plugin in plugins {
                entry.plugins.push(GroupedPluginInfo {
                    manifest: plugin.clone(),
                    repo_dir_name: dir_name.clone(),
                    repo_is_git_repo: is_git,
                    repo_git_remote: git_remote.clone(),
                });
            }
        }

        entry.repos.push(repo);
    }

    order
        .into_iter()
        .filter_map(|k| grouped.remove(&k))
        .collect()
}

/// Update every git-backed repo in a collection, or fall back to a
/// source-based collection update. Absorbs the per-repo orchestration and
/// partial-failure policy that previously lived in `handleUpdateCollection`.
pub async fn update_plugin_collection_all(
    collection_source: Option<String>,
    repo_dirs: Vec<String>,
    git_repo_dirs: Vec<String>,
) -> Result<CollectionOpResult, String> {
    // No git repos but a collection source → single source-based update.
    if git_repo_dirs.is_empty() {
        if let Some(source) = collection_source {
            let message = update_plugin_collection(source, repo_dirs).await?;
            return Ok(CollectionOpResult {
                updated: vec![],
                failed: vec![],
                message,
            });
        }
        return Err("This collection has no git repositories to update.".to_string());
    }

    let mut updated = Vec::new();
    let mut failed = Vec::new();
    for dir in git_repo_dirs {
        match update_plugin(dir.clone()).await {
            Ok(_) => updated.push(dir),
            Err(_) => failed.push(dir),
        }
    }

    let message = if failed.is_empty() {
        format!("Updated {} repo(s).", updated.len())
    } else {
        format!(
            "Updated {}, failed {} ({}).",
            updated.len(),
            failed.len(),
            failed.join(", ")
        )
    };
    Ok(CollectionOpResult {
        updated,
        failed,
        message,
    })
}

/// Remove every repo in a collection. Absorbs `handleRemoveCollection`.
pub async fn remove_plugin_collection(
    repo_dirs: Vec<String>,
) -> Result<CollectionOpResult, String> {
    let mut removed = Vec::new();
    let mut failed = Vec::new();
    for dir in repo_dirs {
        match remove_plugin(dir.clone()).await {
            Ok(_) => removed.push(dir),
            Err(_) => failed.push(dir),
        }
    }

    let message = if failed.is_empty() {
        format!("Removed {} repo(s).", removed.len())
    } else {
        format!(
            "Removed {}, failed {} ({}).",
            removed.len(),
            failed.len(),
            failed.join(", ")
        )
    };
    Ok(CollectionOpResult {
        updated: removed,
        failed,
        message,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::const_new(());

    #[test]
    fn normalize_git_remote_handles_ssh_https_and_path() {
        // SSH form
        assert_eq!(
            normalize_git_remote(Some("git@github.com:OmniLLM/omnilauncher-plugins.git")),
            Some("OmniLLM/omnilauncher-plugins".to_string())
        );
        // HTTPS form, with and without .git
        assert_eq!(
            normalize_git_remote(Some("https://github.com/raycast/extensions.git")),
            Some("raycast/extensions".to_string())
        );
        assert_eq!(
            normalize_git_remote(Some("https://github.com/raycast/extensions")),
            Some("raycast/extensions".to_string())
        );
        // Enterprise host, deeper path → last two segments win
        assert_eq!(
            normalize_git_remote(Some("https://ghosthub.corp.net/team/repo.git")),
            Some("team/repo".to_string())
        );
        // Bare path fallback
        assert_eq!(
            normalize_git_remote(Some("OmniLLM/omnilauncher-plugins")),
            Some("OmniLLM/omnilauncher-plugins".to_string())
        );
        // None / unparseable
        assert_eq!(normalize_git_remote(None), None);
        assert_eq!(normalize_git_remote(Some("singletoken")), None);
    }

    #[test]
    fn parse_github_subdir_url_handles_tree_with_trailing_slash() {
        let p = parse_github_subdir_url(
            "https://github.com/raycast/extensions/tree/main/extensions/apple-stocks-search/",
        )
        .expect("tree URL should parse");
        assert_eq!(p.clone_url, "https://github.com/raycast/extensions.git");
        assert_eq!(p.branch, "main");
        assert_eq!(p.subpath, "extensions/apple-stocks-search");
        assert_eq!(p.leaf_name, "apple-stocks-search");
    }

    #[test]
    fn parse_github_subdir_url_handles_blob() {
        let p = parse_github_subdir_url("https://github.com/o/r/blob/develop/a/b")
            .expect("blob URL should parse");
        assert_eq!(p.subpath, "a/b");
        assert_eq!(p.leaf_name, "b");
        assert_eq!(p.branch, "develop");
    }

    #[test]
    fn parse_github_subdir_url_handles_enterprise_host() {
        let p = parse_github_subdir_url(
            "https://ghosthub.example.com/cloud-foundations/cloudbot/tree/dev/backend/skills/jira",
        )
        .expect("enterprise tree URL should parse");
        assert_eq!(
            p.clone_url,
            "https://ghosthub.example.com/cloud-foundations/cloudbot.git"
        );
        assert_eq!(p.branch, "dev");
        assert_eq!(p.subpath, "backend/skills/jira");
        assert_eq!(p.leaf_name, "jira");
    }

    #[test]
    fn parse_github_subdir_url_rejects_plain_repo() {
        assert!(parse_github_subdir_url("https://github.com/o/r").is_none());
        assert!(parse_github_subdir_url("https://github.com/o/r.git").is_none());
        assert!(parse_github_subdir_url("https://github.com/o/r/tree/main").is_none());
        assert!(parse_github_subdir_url("https://gitlab.com/o/r/tree/main/x").is_none());
    }

    #[test]
    fn dir_name_uses_subpath_leaf_for_tree_urls() {
        assert_eq!(
            dir_name_from_source(
                "https://github.com/raycast/extensions/tree/main/extensions/apple-stocks-search/"
            ),
            "apple-stocks-search"
        );
        assert_eq!(
            dir_name_from_source("https://github.com/o/my-plugin.git"),
            "my-plugin"
        );
    }

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

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn install_csharp_flow_plugin_synthesizes_host() {
        let source = TempDir::new().unwrap();
        let target = TempDir::new().unwrap();
        std::fs::write(
            source.path().join("plugin.json"),
            r#"{
                "ID":"c3406b5c-22f0-4984-b018-3dae897cab3f",
                "ActionKeyword":"d",
                "Name":"Dictionary",
                "Description":"English dictionary.",
                "Version":"2.3.2",
                "Language":"csharp",
                "ExecuteFileName":"Dictionary.dll"
            }"#,
        )
        .unwrap();
        std::fs::write(
            source.path().join("Dictionary.csproj"),
            r#"<Project Sdk="Microsoft.NET.Sdk">
    <PropertyGroup>
        <TargetFramework>net8.0-windows</TargetFramework>
        <OutputType>Library</OutputType>
    </PropertyGroup>
    <ItemGroup>
        <PackageReference Include="Flow.Launcher.Plugin" Version="2.1.1" />
    </ItemGroup>
</Project>"#,
        )
        .unwrap();
        std::fs::write(
            source.path().join("Plugin.cs"),
            r#"using System.Collections.Generic;
using Flow.Launcher.Plugin;

public sealed class DictionaryPlugin : IPlugin
{
        public void Init(PluginInitContext context) { }
        public List<Result> Query(Query query) => new();
}"#,
        )
        .unwrap();

        let result = install_plugin(
            source.path().to_string_lossy().to_string(),
            Some(target.path().to_string_lossy().to_string()),
        )
        .await
        .unwrap();

        assert_eq!(result, "Installed Dictionary");
        let installed_dir = target.path().join(source.path().file_name().unwrap());
        assert!(installed_dir.join("flow-shim.cjs").exists());
        assert!(installed_dir
            .join(".omnilauncher-flow-host")
            .join("Program.cs")
            .exists());
    }

    #[test]
    fn list_plugins_groups_legacy_split_collection_dirs() {
        let _guard = ENV_LOCK.blocking_lock();
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
        let _guard = ENV_LOCK.lock().await;

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
