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

use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Whether the `gh` binary is available on PATH. Cached for the lifetime of the
/// process — gh getting installed mid-run is rare enough that we don't need to
/// re-probe on every install.
pub fn is_gh_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        Command::new("gh")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
}

/// Hosts that `gh` is authenticated against. Parsed from `gh auth status`
/// (which writes to **stderr**). Returns at minimum `["github.com"]` if gh is
/// available, even when `auth status` parsing fails — `gh repo clone` on a
/// public repo doesn't strictly need auth.
pub fn gh_known_hosts() -> Vec<String> {
    static HOSTS: OnceLock<Vec<String>> = OnceLock::new();
    HOSTS
        .get_or_init(|| {
            let out = match Command::new("gh").args(["auth", "status"]).output() {
                Ok(o) => o,
                Err(_) => return vec!["github.com".to_string()],
            };
            // `gh auth status` writes to stderr.
            let text = String::from_utf8_lossy(&out.stderr);
            let mut hosts: Vec<String> = text
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim_end();
                    // Hostname lines are un-indented and contain no spaces, e.g.
                    //   "github.com"
                    //   "ghe.example.com"
                    if trimmed.is_empty()
                        || trimmed.starts_with(char::is_whitespace)
                        || trimmed.contains(' ')
                    {
                        return None;
                    }
                    let candidate = trimmed.trim_end_matches(':');
                    if candidate.contains('.') {
                        Some(candidate.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if !hosts.iter().any(|h| h == "github.com") {
                hosts.push("github.com".to_string());
            }
            hosts.sort();
            hosts.dedup();
            hosts
        })
        .clone()
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
    // Treat GHE-style hostnames as candidates if gh knows them, OR if the
    // hostname starts with "github." / contains "ghe" (best-effort, used only
    // to *attempt* gh; on failure we fall back to git/curl).
    if is_gh_available() {
        for known in gh_known_hosts() {
            if known.eq_ignore_ascii_case(host) {
                return true;
            }
        }
    }
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
    let output = tokio::process::Command::new("gh")
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

    let mut cmd = Command::new("gh");
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
