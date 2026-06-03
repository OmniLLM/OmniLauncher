//! GitHub CLI (`gh`) integration helpers.
//!
//! Used by both the PluginManager (`install_plugin`, `update_plugin_collection`)
//! and the SkillManager (`install_from_url`) so that GitHub repos — including
//! private repos and GitHub Enterprise instances — install transparently when
//! the user already has `gh auth login` configured.
//!
//! Strategy:
//!
//! 1. Detect whether the source string is a GitHub-style URL (github.com or
//!    a known GHE hostname configured via `gh auth status`).
//! 2. If `gh` is on PATH, try the gh-backed path first (`gh repo clone` for
//!    full repos, `gh api` for individual SKILL.md files).
//! 3. On any failure (gh not found, not authenticated for that host, network
//!    error, repo not accessible), bubble back so the caller can fall back to
//!    plain `git clone` / `curl`.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Can `program` be spawned and does `gh --version` succeed? Used both to probe
/// PATH and to validate a candidate absolute path before we commit to it.
fn gh_runs(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve the `gh` executable to use for every gh invocation in the app.
///
/// GUI processes (launched from the macOS Dock/Finder, the Windows shell, or a
/// VS Code session started before `gh` was installed / added to PATH) routinely
/// inherit a minimal `PATH` that omits the directory holding `gh`. Probing the
/// bare name therefore reports "not found" even though `gh` works fine in the
/// user's terminal. To be robust across platforms we:
///
/// 1. honour an explicit override (`OMNILAUNCHER_GH` / `GH_PATH`),
/// 2. try the bare `gh` resolved via the inherited `PATH`, then
/// 3. fall back to the well-known per-platform install locations
///    (Program Files / winget / scoop / choco on Windows; Homebrew, system
///    bins, `~/.local/bin`, snap on macOS & Linux).
///
/// The resolved value (an absolute path, or the bare name `"gh"` when nothing
/// validated) is cached for the process lifetime.
pub fn gh_program() -> String {
    static GH_PATH: OnceLock<String> = OnceLock::new();
    GH_PATH.get_or_init(resolve_gh_program).clone()
}

fn resolve_gh_program() -> String {
    // 1) Explicit override wins, so users can point at a non-standard install.
    for var in ["OMNILAUNCHER_GH", "GH_PATH"] {
        if let Ok(p) = std::env::var(var) {
            let p = p.trim().to_string();
            if !p.is_empty() && gh_runs(&p) {
                log::info!("gh_helper: using gh from {var}={p}");
                return p;
            }
        }
    }

    // 2) Bare name resolved through the inherited PATH.
    if gh_runs("gh") {
        return "gh".to_string();
    }

    // 3) Well-known install locations the inherited PATH may have dropped.
    for candidate in gh_well_known_paths() {
        if candidate.is_file() {
            let s = candidate.to_string_lossy().into_owned();
            if gh_runs(&s) {
                log::info!("gh_helper: resolved gh at {s}");
                return s;
            }
        }
    }

    // Give up: return the bare name so callers still attempt a spawn and fail
    // with a clear OS error rather than silently doing nothing.
    "gh".to_string()
}

/// Per-platform list of likely `gh` install paths, ordered most-specific first.
fn gh_well_known_paths() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let exe = "gh.exe";
        for var in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Ok(dir) = std::env::var(var) {
                out.push(PathBuf::from(dir).join("GitHub CLI").join(exe));
            }
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            // winget shim directory.
            out.push(
                PathBuf::from(&local)
                    .join("Microsoft")
                    .join("WinGet")
                    .join("Links")
                    .join(exe),
            );
            out.push(PathBuf::from(&local).join("GitHub CLI").join(exe));
        }
        if let Ok(userprofile) = std::env::var("USERPROFILE") {
            // scoop shim directory.
            out.push(
                PathBuf::from(&userprofile)
                    .join("scoop")
                    .join("shims")
                    .join(exe),
            );
        }
        // chocolatey shim.
        out.push(PathBuf::from(r"C:\ProgramData\chocolatey\bin").join(exe));
    }

    #[cfg(not(target_os = "windows"))]
    {
        let exe = "gh";
        for dir in [
            "/opt/homebrew/bin",              // Apple Silicon Homebrew
            "/usr/local/bin",                 // Intel Homebrew / manual installs
            "/home/linuxbrew/.linuxbrew/bin", // Linuxbrew
            "/usr/bin",
            "/bin",
            "/snap/bin", // snap
        ] {
            out.push(PathBuf::from(dir).join(exe));
        }
        if let Ok(home) = std::env::var("HOME") {
            out.push(PathBuf::from(&home).join(".local").join("bin").join(exe));
            out.push(PathBuf::from(&home).join("bin").join(exe));
        }
    }

    out
}

/// Whether a usable `gh` binary was found (on PATH or a well-known location).
/// Cached for the lifetime of the process — gh getting installed mid-run is
/// rare enough that we don't need to re-probe on every install.
pub fn is_gh_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| gh_runs(&gh_program()))
}

/// Hosts that `gh` is authenticated against. Combines two sources so a host is
/// recognised even when one of them is unavailable:
///
///   1. `gh auth status` output (covers keyring-stored tokens, but needs `gh`
///      on PATH and parses human-readable text), and
///   2. the top-level hostname keys in gh's `hosts.yml` config file (works even
///      when `gh` isn't reachable from the GUI process, and is stable across gh
///      versions — the keys are present regardless of whether the token lives in
///      the file or the OS keyring).
///
/// Always includes `github.com` so `gh repo clone` on a public repo works even
/// when nothing else is configured.
pub fn gh_known_hosts() -> Vec<String> {
    static HOSTS: OnceLock<Vec<String>> = OnceLock::new();
    HOSTS
        .get_or_init(|| {
            let mut hosts: Vec<String> = Vec::new();

            // Source 1: `gh auth status`. Historically wrote to stderr, but
            // newer gh versions (≥ 2.x) print to stdout. Scan both so GHE hosts
            // are detected regardless of gh version.
            if let Ok(out) = Command::new(gh_program()).args(["auth", "status"]).output() {
                let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                text.push('\n');
                text.push_str(&String::from_utf8_lossy(&out.stderr));
                for line in text.lines() {
                    let trimmed = line.trim_end();
                    // Hostname lines are un-indented and contain no spaces, e.g.
                    //   "github.com"
                    //   "ghe.example.com"
                    if trimmed.is_empty()
                        || trimmed.starts_with(char::is_whitespace)
                        || trimmed.contains(' ')
                    {
                        continue;
                    }
                    let candidate = trimmed.trim_end_matches(':');
                    if candidate.contains('.') && !hosts.iter().any(|h| h == candidate) {
                        hosts.push(candidate.to_string());
                    }
                }
            }

            // Source 2: gh's hosts.yml. Reading the file directly does not
            // depend on `gh` being launchable from this process, so it rescues
            // the common GUI case where the app inherits a stripped PATH.
            for entry in crate::settings::read_gh_hosts_yml() {
                if !entry.hostname.is_empty() && !hosts.iter().any(|h| h == &entry.hostname) {
                    hosts.push(entry.hostname);
                }
            }

            if !hosts.iter().any(|h| h == "github.com") {
                hosts.push("github.com".to_string());
            }
            hosts.sort();
            hosts.dedup();
            hosts
        })
        .clone()
    }

/// Return the `gh` auth token for `host`, selecting the account `gh` considers
/// active for that host. When the user has multiple accounts / hosts logged in
/// (e.g. github.com plus a corporate GitHub Enterprise), this lets callers inject
/// the *correct* host's token into `git` instead of relying on whatever ambient
/// credential helper happens to answer first. Returns `None` when gh is missing,
/// not authenticated for `host`, or the token is empty.
pub fn gh_token_for_host(host: &str) -> Option<String> {
    if !is_gh_available() {
        return None;
    }
    // Strip `GITHUB_TOKEN` / `GH_TOKEN` from the child env so `gh` reads from its
    // own credential store (keyring / hosts.yml) for `host` instead of just
    // echoing back an ambient env var — which may belong to a *different* host
    // and would otherwise be injected as the wrong host's credential.
    let out = Command::new(gh_program())
        .args(["auth", "token", "--hostname", host])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let token = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

/// Build the env-var triple that injects an `Authorization` header for `host`
/// into a `git` invocation, using `token`. Returns
/// `[(GIT_CONFIG_COUNT, "1"), (GIT_CONFIG_KEY_0, ...), (GIT_CONFIG_VALUE_0, ...)]`.
///
/// We pass the credential through `GIT_CONFIG_*` env vars rather than `-c` flags
/// so the token never appears on the process command line. The header uses the
/// `AUTHORIZATION: basic base64("x-access-token:<token>")` form that GitHub's
/// smart-HTTP endpoint accepts (the same scheme `actions/checkout` uses).
pub fn git_auth_env(host: &str, token: &str) -> Vec<(String, String)> {
    use base64::Engine as _;
    let basic = base64::engine::general_purpose::STANDARD
        .encode(format!("x-access-token:{token}"));
    vec![
        ("GIT_CONFIG_COUNT".to_string(), "1".to_string()),
        (
            "GIT_CONFIG_KEY_0".to_string(),
            format!("http.https://{host}/.extraHeader"),
        ),
        (
            "GIT_CONFIG_VALUE_0".to_string(),
            format!("AUTHORIZATION: basic {basic}"),
        ),
    ]
}

/// Parsed GitHub-style URL components.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubRepoRef {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl GithubRepoRef {
    /// Build an `https://<host>/<owner>/<repo>.git` URL — useful when we have
    /// a parsed ref (e.g. from `owner/repo` shorthand) and need a form that
    /// plain `git clone` will accept.
    pub fn clone_url(&self) -> String {
        format!("https://{}/{}/{}.git", self.host, self.owner, self.repo)
    }
}

/// Parse a clone-style URL into `(host, owner, repo)`. Accepts:
///
///   https://github.com/owner/repo            (with or without `.git`)
///   https://github.com/owner/repo/tree/...   (subpath stripped)
///   git@github.com:owner/repo.git
///   https://ghe.company.com/owner/repo.git
///   owner/repo                                (shorthand → host="github.com")
pub fn parse_github_repo(source: &str) -> Option<GithubRepoRef> {
    let s = source.trim();

    // SSH form: git@host:owner/repo(.git)
    if let Some(rest) = s.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        let path = path.trim_end_matches('/').trim_end_matches(".git");
        let mut parts = path.splitn(2, '/');
        let owner = parts.next()?;
        let repo = parts.next()?;
        if owner.is_empty() || repo.is_empty() {
            return None;
        }
        return Some(GithubRepoRef {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.split('/').next()?.to_string(),
        });
    }

    // HTTPS / HTTP
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .or_else(|| {
            // Bare `owner/repo` shorthand
            if !s.contains("://") && !s.contains(' ') && s.matches('/').count() == 1 {
                Some(s)
            } else {
                None
            }
        })?;

    let (host, path) = if rest.contains("://") || !rest.contains('/') {
        return None;
    } else if !s.contains("://") {
        // shorthand path
        ("github.com", rest)
    } else {
        rest.split_once('/')?
    };

    if !is_known_github_host(host) {
        return None;
    }

    let path = path.trim_end_matches('/');
    let mut parts = path.split('/');
    let owner = parts.next()?;
    let repo_raw = parts.next()?;
    let repo = repo_raw.trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(GithubRepoRef {
        host: host.to_string(),
        owner: owner.to_string(),
        repo: repo.to_string(),
    })
}

/// Heuristic: is `host` either github.com or a host gh is authenticated against?
pub fn is_known_github_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("github.com") {
        return true;
    }
    // A host the user has actually authenticated against (via `gh auth status`
    // or gh's hosts.yml) is authoritative — accept it regardless of whether the
    // hostname looks GitHub-ish. `gh_known_hosts()` reads hosts.yml directly, so
    // this works even when `gh` can't be launched from this (GUI) process.
    for known in gh_known_hosts() {
        if known.eq_ignore_ascii_case(host) {
            return true;
        }
    }
    // Best-effort name heuristic for hosts we have no config for; used only to
    // *attempt* gh/git, with a fall back to curl on failure.
    let lower = host.to_lowercase();
    lower.starts_with("github.") || lower.contains(".github.") || lower.contains("ghe.")
}

/// Run `gh repo clone <owner/repo> <dest> -- --depth=1`. Returns Ok on success,
/// Err with stderr on failure. The caller is responsible for falling back to
/// `git clone` if this fails.
pub async fn gh_clone(
    repo: &GithubRepoRef,
    dest: &Path,
    extra_git_args: &[&str],
) -> Result<(), String> {
    if !is_gh_available() {
        return Err("gh CLI not available".to_string());
    }
    let dest_str = dest.to_string_lossy().into_owned();
    let target = format!("{}/{}", repo.owner, repo.repo);

    // gh respects per-host auth automatically when given a full URL or when
    // `gh auth login --hostname <host>` was previously run. We pass the full
    // https URL so non-default hosts route correctly even without gh's
    // `--hostname` flag (which `gh repo clone` doesn't accept).
    let url = format!("https://{}/{}", repo.host, target);

    let mut args: Vec<String> = vec!["repo".into(), "clone".into(), url, dest_str];
    if !extra_git_args.is_empty() {
        args.push("--".into());
        for a in extra_git_args {
            args.push((*a).to_string());
        }
    }

    log::info!("gh_helper: gh {}", args.join(" "));
    let output = tokio::process::Command::new(gh_program())
        .args(&args)
        .output()
        .await
        .map_err(|e| format!("Failed to spawn gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh repo clone failed (exit {:?})", output.status.code())
        } else {
            format!("gh repo clone failed: {stderr}")
        });
    }
    Ok(())
}

/// Fetch the raw content of a single file in a GitHub repo via `gh api`.
/// Works for both github.com and GHE (when authenticated). Returns the file
/// content as a String, or Err on failure.
///
/// `path_in_repo` should be like `"skills/foo/SKILL.md"`. `branch` is e.g.
/// `"main"`.
pub fn gh_fetch_raw(
    repo: &GithubRepoRef,
    branch: &str,
    path_in_repo: &str,
) -> Result<String, String> {
    if !is_gh_available() {
        return Err("gh CLI not available".to_string());
    }
    let endpoint = format!(
        "repos/{}/{}/contents/{}?ref={}",
        repo.owner, repo.repo, path_in_repo, branch
    );

    let mut cmd = Command::new(gh_program());
    cmd.arg("api");
    if !repo.host.eq_ignore_ascii_case("github.com") {
        cmd.args(["--hostname", &repo.host]);
    }
    cmd.args(["-H", "Accept: application/vnd.github.raw", &endpoint]);

    log::info!(
        "gh_helper: gh api {} (host={})",
        endpoint,
        repo.host
    );

    let output = cmd.output().map_err(|e| format!("Failed to spawn gh: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh api failed (exit {:?})", output.status.code())
        } else {
            format!("gh api failed: {stderr}")
        });
    }
    String::from_utf8(output.stdout).map_err(|e| format!("UTF-8 decode error: {e}"))
}

/// Resolve a repo's default branch via `gh api repos/<owner>/<repo>`.
///
/// Synchronous counterpart to the `git ls-remote --symref` probe. Used as a
/// fallback when plain `git` can't authenticate to a private / GHE repo from a
/// GUI process: `gh` carries its own per-host credentials (keyring or
/// hosts.yml) and reliably reaches hosts the inherited git credential helper
/// cannot. Returns `None` when gh is missing, not authed for the host, or the
/// response has no `default_branch`.
pub fn gh_default_branch(repo: &GithubRepoRef) -> Option<String> {
    if !is_gh_available() {
        return None;
    }
    let endpoint = format!("repos/{}/{}", repo.owner, repo.repo);
    let mut cmd = Command::new(gh_program());
    cmd.arg("api");
    if !repo.host.eq_ignore_ascii_case("github.com") {
        cmd.args(["--hostname", &repo.host]);
    }
    cmd.args(["--jq", ".default_branch", &endpoint])
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN");

    log::info!("gh_helper: gh api {} --jq .default_branch (host={})", endpoint, repo.host);

    let output = cmd.output().ok()?;
    if !output.status.success() {
        log::warn!(
            "gh_helper: gh api default_branch failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        return None;
    }
    let branch = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if branch.is_empty() || branch == "null" {
        None
    } else {
        Some(branch)
    }
}

/// Synchronous `gh repo clone <https-url> <dest> -- <extra_git_args>`.
///
/// Fallback for when plain `git clone` can't authenticate to a private / GHE
/// repo from a GUI process. `gh` injects the correct per-host credential itself,
/// so this succeeds for hosts the inherited git credential helper can't reach.
/// Returns Ok on success, Err with stderr otherwise.
pub fn gh_clone_sync(
    repo: &GithubRepoRef,
    dest: &Path,
    extra_git_args: &[&str],
) -> Result<(), String> {
    if !is_gh_available() {
        return Err("gh CLI not available".to_string());
    }
    let dest_str = dest.to_string_lossy().into_owned();
    let url = format!("https://{}/{}/{}", repo.host, repo.owner, repo.repo);

    let mut args: Vec<String> = vec!["repo".into(), "clone".into(), url, dest_str];
    if !extra_git_args.is_empty() {
        args.push("--".into());
        for a in extra_git_args {
            args.push((*a).to_string());
        }
    }

    log::info!("gh_helper: gh {}", args.join(" "));
    let output = Command::new(gh_program())
        .args(&args)
        .env_remove("GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .output()
        .map_err(|e| format!("Failed to spawn gh: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("gh repo clone failed (exit {:?})", output.status.code())
        } else {
            format!("gh repo clone failed: {stderr}")
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_github_url() {
        let r = parse_github_repo("https://github.com/owner/repo").unwrap();
        assert_eq!(r.host, "github.com");
        assert_eq!(r.owner, "owner");
        assert_eq!(r.repo, "repo");
    }

    #[test]
    fn parses_https_with_dot_git() {
        let r = parse_github_repo("https://github.com/owner/repo.git").unwrap();
        assert_eq!(r.repo, "repo");
    }

    #[test]
    fn parses_tree_url_strips_subpath() {
        let r = parse_github_repo("https://github.com/o/r/tree/main/x/y").unwrap();
        assert_eq!(r.owner, "o");
        assert_eq!(r.repo, "r");
    }

    #[test]
    fn parses_ssh_form() {
        let r = parse_github_repo("git@github.com:owner/repo.git").unwrap();
        assert_eq!(r.host, "github.com");
        assert_eq!(r.owner, "owner");
        assert_eq!(r.repo, "repo");
    }

    #[test]
    fn parses_owner_repo_shorthand() {
        let r = parse_github_repo("owner/repo").unwrap();
        assert_eq!(r.host, "github.com");
        assert_eq!(r.owner, "owner");
        assert_eq!(r.repo, "repo");
    }

    #[test]
    fn rejects_non_github() {
        assert!(parse_github_repo("https://gitlab.com/owner/repo").is_none());
        assert!(parse_github_repo("https://example.com/foo").is_none());
        assert!(parse_github_repo("/local/path/plugin").is_none());
        assert!(parse_github_repo("not a url at all").is_none());
    }

    #[test]
    fn accepts_ghe_host_pattern() {
        // github.* hosts are accepted as candidates even if gh isn't auth'd
        // for them (caller still falls back on failure).
        let r = parse_github_repo("https://github.company.com/o/r");
        assert!(r.is_some());
    }
}
