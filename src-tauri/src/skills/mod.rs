use crate::path_config;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub mod curator;
pub mod consolidate;

// ─── Data structures ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub version: String,
    pub triggers: Vec<String>,
    pub tags: Vec<String>,
    pub tools_hint: Vec<String>,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub meta: SkillMeta,
    pub body: String,
}

pub struct SkillManager {
    skills: Vec<Skill>,
}

// ─── Frontmatter parser ───────────────────────────────────────────────────────

/// Parse a SKILL.md file content into (SkillMeta, body).
/// Returns None if frontmatter is missing or malformed.
fn parse_skill_file(content: &str, path: PathBuf) -> Option<Skill> {
    // Must start with ---
    if !content.trim_start().starts_with("---") {
        return None;
    }

    let re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)$").ok()?;
    let caps = re.captures(content.trim_start())?;

    let frontmatter = caps.get(1)?.as_str();
    let body = caps.get(2)?.as_str().trim().to_string();

    // Parse frontmatter key: value pairs
    let mut name = String::new();
    let mut description = String::new();
    let mut version = "1.0.0".to_string();
    let mut triggers: Vec<String> = vec![];
    let mut tags: Vec<String> = vec![];
    let mut tools_hint: Vec<String> = vec![];

    let list_re = Regex::new(r"\[([^\]]*)\]").ok()?;

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, val)) = line.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "name" => name = val.to_string(),
                "description" => description = val.to_string(),
                "version" => version = val.to_string(),
                "triggers" => triggers = parse_list(val, &list_re),
                "tags" => tags = parse_list(val, &list_re),
                "tools" => tools_hint = parse_list(val, &list_re),
                _ => {}
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(Skill {
        meta: SkillMeta {
            name,
            description,
            version,
            triggers,
            tags,
            tools_hint,
            path,
        },
        body,
    })
}

/// Parse `[item1, item2, item3]` or `item1, item2` into a Vec<String>
fn parse_list(val: &str, list_re: &Regex) -> Vec<String> {
    let inner = if let Some(caps) = list_re.captures(val) {
        caps.get(1).map(|m| m.as_str()).unwrap_or(val)
    } else {
        val
    };
    inner
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn normalize_skill_url(url: &str) -> String {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return trimmed.to_string();
    }

    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() >= 7 {
        let domain = parts[2];
        let owner = parts[3];
        let repo = parts[4];
        let action = parts[5];
        let branch = parts[6];
        let path_parts = &parts[7..];

        if domain.to_lowercase() == "github.com" {
            if action == "blob" {
                return format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/{}",
                    owner,
                    repo,
                    branch,
                    path_parts.join("/")
                );
            } else if action == "tree" {
                let mut path = path_parts.join("/");
                if !path.ends_with("SKILL.md") {
                    if path.is_empty() {
                        path = "SKILL.md".to_string();
                    } else if path.ends_with('/') {
                        path = format!("{}SKILL.md", path);
                    } else {
                        path = format!("{}/SKILL.md", path);
                    }
                }
                return format!(
                    "https://raw.githubusercontent.com/{}/{}/{}/{}",
                    owner, repo, branch, path
                );
            }
        } else if action == "blob" || action == "tree" {
            // Enterprise or self-hosted GitHub with similar URL structure
            let mut path = path_parts.join("/");
            if action == "tree" && !path.ends_with("SKILL.md") {
                if path.is_empty() {
                    path = "SKILL.md".to_string();
                } else if path.ends_with('/') {
                    path = format!("{}SKILL.md", path);
                } else {
                    path = format!("{}/SKILL.md", path);
                }
            }
            let scheme = parts[0].trim_end_matches(':');
            return format!(
                "{}://{}/{}/{}/raw/{}/{}",
                scheme, domain, owner, repo, branch, path
            );
        }
    }

    trimmed.to_string()
}

/// Parse a GitHub `blob/<branch>/<path>` or `tree/<branch>/<path>` URL into
/// `(repo, branch, path_in_repo)` for use with `gh api repos/.../contents/...`.
/// `tree/` URLs get `SKILL.md` appended when the path doesn't already end in
/// it, mirroring `normalize_skill_url`.
fn parse_github_blob_or_tree(
    url: &str,
) -> Option<(crate::gh_helper::GithubRepoRef, String, String)> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() < 7 {
        return None;
    }
    let host = parts[2];
    let owner = parts[3];
    let repo = parts[4].trim_end_matches(".git");
    let action = parts[5];
    let branch = parts[6];
    if owner.is_empty() || repo.is_empty() || branch.is_empty() {
        return None;
    }
    if action != "blob" && action != "tree" {
        return None;
    }
    if !crate::gh_helper::is_known_github_host(host) {
        return None;
    }

    let path_parts = &parts[7..];
    let mut path = path_parts.join("/");
    if action == "tree" && !path.ends_with("SKILL.md") {
        path = if path.is_empty() {
            "SKILL.md".to_string()
        } else if path.ends_with('/') {
            format!("{}SKILL.md", path)
        } else {
            format!("{}/SKILL.md", path)
        };
    }
    if path.is_empty() {
        return None;
    }

    Some((
        crate::gh_helper::GithubRepoRef {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        },
        branch.to_string(),
        path,
    ))
}

/// Fetch a single `SKILL.md` via a shallow sparse `git clone`.
///
/// This is the first choice for installs because plain `git` works for every
/// public repo with no extra dependencies and honours the user's existing git
/// credential helpers (so it also covers many private / GitHub Enterprise
/// repos transparently). Returns `None` when the URL isn't a recognisable
/// GitHub blob/tree URL or when any git step fails, so the caller can fall back
/// to `gh` / `curl`.
/// Result of a git-based skill fetch: the parsed `SKILL.md` content plus the
/// on-disk locations holding the *entire* skill folder (`SKILL.md` and any
/// sibling scripts/assets). The caller owns `clone_root` and must delete it
/// once the files have been copied out.
struct GitFetchedSkill {
    content: String,
    /// Top-level temp clone directory; remove this to clean up.
    clone_root: PathBuf,
    /// Directory inside `clone_root` that holds the full skill folder.
    skill_dir: PathBuf,
}

fn git_fetch_skill(url: &str) -> Option<GitFetchedSkill> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let (repo, branch, path_in_repo) = parse_github_blob_or_tree(url)?;
    let clone_url = repo.clone_url();

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!(
        "omnilauncher-skill-{}-{}",
        std::process::id(),
        ts
    ));
    if stage.exists() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    let stage_str = stage.to_string_lossy().into_owned();

    // Directory containing the SKILL.md (sparse-checkout works on dirs, not files).
    let dir_in_repo = path_in_repo
        .rsplit_once('/')
        .map(|(dir, _)| dir.to_string())
        .unwrap_or_default();

    log::info!(
        "install_from_url: git clone --depth=1 --filter=blob:none --no-checkout --branch {} {} {}",
        branch, clone_url, stage_str
    );

    let mut clone_cmd = std::process::Command::new("git");
    clone_cmd.args([
        "clone",
        "--depth=1",
        "--filter=blob:none",
        "--no-checkout",
        "--no-tags",
        "--single-branch",
        "--branch",
        &branch,
        &clone_url,
        &stage_str,
    ]);
    // When multiple gh accounts/hosts are configured, inject the token gh
    // considers active for *this* host so git authenticates as the right
    // account instead of relying on whatever ambient credential answers first.
    if let Some(token) = crate::gh_helper::gh_token_for_host(&repo.host) {
        for (k, v) in crate::gh_helper::git_auth_env(&repo.host, &token) {
            clone_cmd.env(k, v);
        }
    }
    let clone = clone_cmd.output().ok()?;
    if !clone.status.success() {
        log::warn!(
            "install_from_url: git clone failed: {}",
            String::from_utf8_lossy(&clone.stderr).trim()
        );
        let _ = std::fs::remove_dir_all(&stage);
        return None;
    }

    let run_git = |args: &[&str]| -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&stage)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };

    let sparse_ok = run_git(&["sparse-checkout", "init", "--cone"])
        && (dir_in_repo.is_empty() || run_git(&["sparse-checkout", "set", &dir_in_repo]))
        && run_git(&["checkout"]);
    if !sparse_ok {
        log::warn!("install_from_url: git sparse-checkout failed");
        let _ = std::fs::remove_dir_all(&stage);
        return None;
    }

    let content = match std::fs::read_to_string(stage.join(&path_in_repo)) {
        Ok(c) => c,
        Err(_) => {
            let _ = std::fs::remove_dir_all(&stage);
            return None;
        }
    };

    // Directory on disk that holds the full skill folder (SKILL.md + siblings).
    let skill_dir = if dir_in_repo.is_empty() {
        stage.clone()
    } else {
        stage.join(&dir_in_repo)
    };

    Some(GitFetchedSkill {
        content,
        clone_root: stage,
        skill_dir,
    })
}

/// Recursively copy the contents of `src` into `dst`, creating `dst` if needed.
/// The repo's `.git` metadata is skipped so installed skills stay clean.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        if file_name == ".git" {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&file_name);
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Parse a `tree/<branch>/<dir>` URL into `(repo, branch, dir_in_repo)` WITHOUT
/// appending `SKILL.md`. Used to install a *directory* of skills (a folder that
/// contains multiple `<name>/SKILL.md` entries). Returns `None` for `blob` URLs,
/// for tree URLs that already point at a `SKILL.md`, or for non-GitHub hosts.
fn parse_github_tree_dir(
    url: &str,
) -> Option<(crate::gh_helper::GithubRepoRef, String, String)> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() < 7 {
        return None;
    }
    let host = parts[2];
    let owner = parts[3];
    let repo = parts[4].trim_end_matches(".git");
    let action = parts[5];
    let branch = parts[6];
    if action != "tree" || owner.is_empty() || repo.is_empty() || branch.is_empty() {
        return None;
    }
    if !crate::gh_helper::is_known_github_host(host) {
        return None;
    }
    let dir_in_repo = parts[7..].join("/");
    let dir_in_repo = dir_in_repo.trim_end_matches('/').to_string();
    if dir_in_repo.ends_with("SKILL.md") {
        return None;
    }
    Some((
        crate::gh_helper::GithubRepoRef {
            host: host.to_string(),
            owner: owner.to_string(),
            repo: repo.to_string(),
        },
        branch.to_string(),
        dir_in_repo,
    ))
}

/// Shallow sparse `git clone` of a single directory inside a repo. Returns the
/// temp clone root on success (the caller owns it and must delete it). Mirrors
/// `git_fetch_skill`'s per-host token injection so private/GHE repos work.
fn git_sparse_clone_dir(
    repo: &crate::gh_helper::GithubRepoRef,
    branch: &str,
    dir_in_repo: &str,
) -> Option<PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let clone_url = repo.clone_url();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let stage = std::env::temp_dir().join(format!(
        "omnilauncher-skilldir-{}-{}",
        std::process::id(),
        ts
    ));
    if stage.exists() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    let stage_str = stage.to_string_lossy().into_owned();

    let mut clone_cmd = std::process::Command::new("git");
    clone_cmd.args([
        "clone",
        "--depth=1",
        "--filter=blob:none",
        "--no-checkout",
        "--no-tags",
        "--single-branch",
        "--branch",
        branch,
        &clone_url,
        &stage_str,
    ]);
    if let Some(token) = crate::gh_helper::gh_token_for_host(&repo.host) {
        for (k, v) in crate::gh_helper::git_auth_env(&repo.host, &token) {
            clone_cmd.env(k, v);
        }
    }
    let clone = clone_cmd.output().ok()?;
    if !clone.status.success() {
        log::warn!(
            "git_sparse_clone_dir: git clone failed: {}",
            String::from_utf8_lossy(&clone.stderr).trim()
        );
        let _ = std::fs::remove_dir_all(&stage);
        return None;
    }

    let run_git = |args: &[&str]| -> bool {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&stage)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };

    let sparse_ok = run_git(&["sparse-checkout", "init", "--cone"])
        && (dir_in_repo.is_empty() || run_git(&["sparse-checkout", "set", dir_in_repo]))
        && run_git(&["checkout"]);
    if !sparse_ok {
        log::warn!("git_sparse_clone_dir: git sparse-checkout failed");
        let _ = std::fs::remove_dir_all(&stage);
        return None;
    }

    Some(stage)
}


// ─── SkillManager ─────────────────────────────────────────────────────────────

impl SkillManager {
    pub fn new() -> Self {
        Self { skills: Vec::new() }
    }

    /// Load all skills: bundled first, then user skills dir.
    pub fn load_all(&mut self) {
        // Ensure user skills dir exists
        let user_dir = Self::skill_dir();
        if !user_dir.exists() {
            let _ = std::fs::create_dir_all(&user_dir);
        }

        // Load bundled skills
        if let Some(bundled) = Self::bundled_dir() {
            if bundled.exists() {
                self.load_from_dir(&bundled);
            }
        }

        // Load user skills (can override bundled by same name)
        if user_dir.exists() {
            self.load_from_dir(&user_dir);
        }
    }

    /// Load all SKILL.md files from a directory (one level deep: dir/<name>/SKILL.md)
    pub fn load_from_dir(&mut self, dir: &Path) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let skill_file = entry.path().join("SKILL.md");
                if skill_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&skill_file) {
                        if let Some(skill) = parse_skill_file(&content, skill_file.clone()) {
                            // Remove existing skill with same name (dedup / override)
                            self.skills.retain(|s| s.meta.name != skill.meta.name);
                            self.skills.push(skill);
                        }
                    }
                }
            }
        }
    }

    /// Return metadata for all loaded skills.
    pub fn list_meta(&self) -> Vec<&SkillMeta> {
        self.skills.iter().map(|s| &s.meta).collect()
    }

    /// Owned snapshot of every installed skill as `(name, dir, body)` where
    /// `dir` is the skill's absolute install directory (parent of its
    /// `SKILL.md`). Lets callers (e.g. the AI router's `load_skill` tool) hand
    /// the model a skill's full instructions on demand without holding a borrow
    /// on the manager.
    pub fn snapshot(&self) -> Vec<(String, String, String)> {
        self.skills
            .iter()
            .map(|s| {
                let dir = s
                    .meta
                    .path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (s.meta.name.clone(), dir, s.body.clone())
            })
            .collect()
    }

    /// Find skills relevant to the query by matching triggers and name.
    /// Skills marked `archived` by the curator are filtered out unless the
    /// user explicitly named them (handled by the router via `get_by_name`).
    pub fn find_relevant(&self, query: &str) -> Vec<&Skill> {
        let query_lower = query.to_lowercase();
        let usage = curator::snapshot();
        let hits: Vec<&Skill> = self
            .skills
            .iter()
            .filter(|skill| {
                // Hide archived skills from auto-pick.
                if let Some(u) = usage.skills.get(&skill.meta.name) {
                    if u.state == curator::SkillState::Archived && !u.pinned {
                        return false;
                    }
                }
                // Check triggers
                let trigger_match = skill.meta.triggers.iter().any(|t| {
                    let t_lower = t.to_lowercase();
                    query_lower.contains(&t_lower) || t_lower.contains(&query_lower)
                });
                // Check name
                let name_match = query_lower.contains(&skill.meta.name.to_lowercase());
                // Check tags
                let tag_match = skill
                    .meta
                    .tags
                    .iter()
                    .any(|t| query_lower.contains(&t.to_lowercase()));
                trigger_match || name_match || tag_match
            })
            .collect();
        for s in &hits {
            curator::record_use(&s.meta.name);
        }
        hits
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Skill> {
        let hit = self.skills.iter().find(|s| s.meta.name == name);
        if hit.is_some() {
            curator::record_use(name);
        }
        hit
    }

    /// Names of every currently-installed user skill (i.e. the ones whose
    /// `path` is under the user data dir, not bundled assets). The curator
    /// uses this to scope its lifecycle transitions.
    pub fn user_skill_names(&self) -> Vec<String> {
        let user_root = Self::skill_dir();
        self.skills
            .iter()
            .filter(|s| s.meta.path.starts_with(&user_root))
            .map(|s| s.meta.name.clone())
            .collect()
    }

    /// Install one skill whose files are already staged on disk at
    /// `staged_skill_dir` (must contain a `SKILL.md`). Copies the whole folder
    /// into the user skills dir, records `source_url` in `.source` for updates,
    /// reloads it, and returns the skill's name.
    fn install_skill_from_staged_dir(
        &mut self,
        staged_skill_dir: &Path,
        source_url: &str,
    ) -> Result<String, String> {
        let skill_md = staged_skill_dir.join("SKILL.md");
        let content =
            std::fs::read_to_string(&skill_md).map_err(|e| format!("read failed: {}", e))?;
        let skill = parse_skill_file(&content, skill_md.clone())
            .ok_or_else(|| "Invalid SKILL.md format".to_string())?;

        let name = skill.meta.name.clone();
        let dest_dir = Self::skill_dir().join(&name);
        std::fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir failed: {}", e))?;

        if let Err(e) = copy_dir_recursive(staged_skill_dir, &dest_dir) {
            log::warn!("install_skill_from_staged_dir: failed to copy skill files: {e}");
        }

        let dest_file = dest_dir.join("SKILL.md");
        std::fs::write(&dest_file, &content).map_err(|e| format!("write failed: {}", e))?;
        let _ = std::fs::write(dest_dir.join(".source"), source_url);

        self.skills.retain(|s| s.meta.name != name);
        if let Some(s) = parse_skill_file(&content, dest_file) {
            self.skills.push(s);
        }
        Ok(name)
    }

    /// Download and install a skill from a URL.
    ///
    /// Priority: `git` → `gh api` → `curl`, mirroring the PluginManager. Plain
    /// `git` (shallow sparse clone) is tried first because it needs no extra
    /// dependencies, works for all public repos, and honours the user's git
    /// credential helpers. We fall back to `gh api` for private repos / GitHub
    /// Enterprise authenticated via `gh auth login`, and finally to `curl`
    /// against the raw URL for hosts where neither is configured.
    pub fn install_from_url(&mut self, url: &str) -> Result<String, String> {
        let download_url = normalize_skill_url(url);

        // 0) Directory-of-skills: a `tree` URL may point at a *folder* that
        //    contains several `<name>/SKILL.md` entries (e.g. a repo's
        //    `skills/` directory) rather than a single skill. Detect that case
        //    and install every skill found. When the folder is itself a single
        //    skill (has its own SKILL.md) or nothing is found, fall through to
        //    the single-skill logic below.
        if let Some((repo, branch, dir_in_repo)) = parse_github_tree_dir(url) {
            if let Some(clone_root) = git_sparse_clone_dir(&repo, &branch, &dir_in_repo) {
                let base = if dir_in_repo.is_empty() {
                    clone_root.clone()
                } else {
                    clone_root.join(&dir_in_repo)
                };

                // Only treat as a collection when the folder itself is NOT a
                // skill but contains immediate subfolders that are.
                if !base.join("SKILL.md").is_file() {
                    let mut installed: Vec<String> = Vec::new();
                    let mut errors: Vec<String> = Vec::new();
                    if let Ok(entries) = std::fs::read_dir(&base) {
                        let mut subdirs: Vec<_> = entries
                            .flatten()
                            .filter(|e| e.path().join("SKILL.md").is_file())
                            .collect();
                        subdirs.sort_by_key(|e| e.file_name());
                        for entry in subdirs {
                            // Per-skill source URL so individual updates work.
                            let sub_url = format!(
                                "{}/{}",
                                url.trim_end_matches('/'),
                                entry.file_name().to_string_lossy()
                            );
                            match self.install_skill_from_staged_dir(&entry.path(), &sub_url) {
                                Ok(name) => installed.push(name),
                                Err(e) => errors.push(e),
                            }
                        }
                    }
                    let _ = std::fs::remove_dir_all(&clone_root);
                    if !installed.is_empty() {
                        let mut msg = format!(
                            "Installed {} skill(s): {}",
                            installed.len(),
                            installed.join(", ")
                        );
                        if !errors.is_empty() {
                            msg.push_str(&format!(" ({} failed)", errors.len()));
                        }
                        return Ok(msg);
                    }
                    // Nothing installed — fall through to single-skill logic.
                } else {
                    let _ = std::fs::remove_dir_all(&clone_root);
                }
            }
        }

        // Helper: accept fetched content only when it parses as a real SKILL.md.
        // Some hosts (SSO-protected GitHub Enterprise) answer an unauthenticated
        // raw request with an HTML login page and HTTP 200, so a "successful"
        // fetch can still return garbage. Validating here lets us fall through
        // to the next strategy instead of failing outright.
        let parses_as_skill =
            |text: &str| parse_skill_file(text, PathBuf::from("/tmp/SKILL.md")).is_some();

        let mut content_opt: Option<String> = None;
        let mut last_err: Option<String> = None;
        // When the git path succeeds it stages the *whole* skill folder so we can
        // copy sibling scripts/assets, not just SKILL.md. Cleaned up at the end.
        let mut git_clone_root: Option<PathBuf> = None;
        let mut git_skill_dir: Option<PathBuf> = None;

        // 1) git: shallow sparse clone of the repo and read the SKILL.md.
        if let Some(fetched) = git_fetch_skill(url) {
            if parses_as_skill(&fetched.content) {
                content_opt = Some(fetched.content);
                git_clone_root = Some(fetched.clone_root);
                git_skill_dir = Some(fetched.skill_dir);
            } else {
                log::warn!("install_from_url: git fetch returned invalid SKILL.md content");
                let _ = std::fs::remove_dir_all(&fetched.clone_root);
                last_err = Some(
                    "git fetch did not return a valid SKILL.md (the host may require \
                     authentication)."
                        .to_string(),
                );
            }
        }

        // 2) gh api: handles private repos & GHE when authed.
        if content_opt.is_none() {
            if let Some((repo, branch, path_in_repo)) = parse_github_blob_or_tree(url) {
                if crate::gh_helper::is_gh_available() {
                    log::info!(
                        "install_from_url: git unusable, trying gh api {}/{}@{}:{}",
                        repo.owner, repo.repo, branch, path_in_repo
                    );
                    match crate::gh_helper::gh_fetch_raw(&repo, &branch, &path_in_repo) {
                        Ok(text) if parses_as_skill(&text) => content_opt = Some(text),
                        Ok(_) => log::warn!(
                            "install_from_url: gh fetch returned invalid SKILL.md content"
                        ),
                        Err(e) => {
                            log::warn!("install_from_url: gh fetch also failed: {e}");
                            last_err = Some(e);
                        }
                    }
                }
            }
        }

        // 3) curl: last resort against the raw URL.
        if content_opt.is_none() {
            log::info!("install_from_url: gh unusable, trying curl -fsSL {}", download_url);
            match std::process::Command::new("curl")
                .args(["-fsSL", &download_url])
                .output()
            {
                Ok(output) if output.status.success() => match String::from_utf8(output.stdout) {
                    Ok(text) if parses_as_skill(&text) => content_opt = Some(text),
                    Ok(_) => {
                        last_err = Some(
                            "Download did not return a valid SKILL.md (the host may require \
                             authentication). Try `gh auth login` for that host."
                                .to_string(),
                        );
                    }
                    Err(e) => last_err = Some(format!("UTF-8 decode error: {e}")),
                },
                Ok(output) => {
                    last_err = Some(format!(
                        "Download failed: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ));
                }
                Err(e) => last_err = Some(format!("curl failed: {e}")),
            }
        }

        let content = content_opt.ok_or_else(|| {
            last_err.unwrap_or_else(|| "Download failed: unknown error".to_string())
        })?;

        // Parse to extract name
        let tmp_path = PathBuf::from("/tmp/SKILL.md");
        let skill = parse_skill_file(&content, tmp_path)
            .ok_or_else(|| "Invalid SKILL.md format".to_string())?;

        let name = skill.meta.name.clone();
        let dest_dir = Self::skill_dir().join(&name);
        std::fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir failed: {}", e))?;

        // When the git path staged the full skill folder, copy every file
        // (scripts, assets, nested dirs) so the install matches the source repo.
        // Otherwise (gh/curl single-file fetch) we only have the SKILL.md text.
        if let Some(skill_dir) = &git_skill_dir {
            if let Err(e) = copy_dir_recursive(skill_dir, &dest_dir) {
                log::warn!("install_from_url: failed to copy skill files: {e}");
            }
        }
        if let Some(root) = &git_clone_root {
            let _ = std::fs::remove_dir_all(root);
        }

        // Always (re)write SKILL.md from the validated content so it reflects
        // exactly what we parsed, even if the recursive copy was skipped.
        let dest_file = dest_dir.join("SKILL.md");
        std::fs::write(&dest_file, &content).map_err(|e| format!("write failed: {}", e))?;

        // Persist source URL so update can re-fetch
        let source_file = dest_dir.join(".source");
        let _ = std::fs::write(&source_file, url);

        // Reload
        self.skills.retain(|s| s.meta.name != name);
        if let Some(s) = parse_skill_file(&content, dest_file) {
            self.skills.push(s);
        }

        Ok(format!("Installed skill: {}", name))
    }

    /// Update a skill by re-fetching from its stored source URL.
    pub fn update_skill(&mut self, name: &str) -> Result<String, String> {
        let source_file = Self::skill_dir().join(name).join(".source");
        let url = std::fs::read_to_string(&source_file).map_err(|_| {
            format!(
                "Skill '{}' has no update source (was not installed from a URL).",
                name
            )
        })?;
        let url = url.trim().to_string();
        self.install_from_url(&url)
            .map(|_| format!("Updated skill: {}", name))
    }

    /// Install a skill from a local file path.
    pub fn install_from_path(&mut self, path: &str) -> Result<String, String> {
        let src = PathBuf::from(path);
        let content = std::fs::read_to_string(&src).map_err(|e| format!("read failed: {}", e))?;

        let skill = parse_skill_file(&content, src.clone())
            .ok_or_else(|| "Invalid SKILL.md format".to_string())?;

        let name = skill.meta.name.clone();
        let dest_dir = Self::skill_dir().join(&name);
        std::fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir failed: {}", e))?;

        let dest_file = dest_dir.join("SKILL.md");
        std::fs::copy(&src, &dest_file).map_err(|e| format!("copy failed: {}", e))?;

        self.skills.retain(|s| s.meta.name != name);
        if let Some(s) = parse_skill_file(&content, dest_file) {
            self.skills.push(s);
        }

        Ok(format!("Installed skill: {}", name))
    }

    /// Hot-reload all skills without restarting.
    pub fn reload(&mut self) {
        self.skills.clear();
        self.load_all();
    }

    /// Delete a skill by name (removes its directory from user skills dir).
    pub fn delete_skill(&mut self, name: &str) -> Result<String, String> {
        let skill_dir = Self::skill_dir().join(name);
        if skill_dir.exists() {
            std::fs::remove_dir_all(&skill_dir)
                .map_err(|e| format!("Failed to delete skill '{}': {}", name, e))?;
            self.skills.retain(|s| s.meta.name != name);
            Ok(format!("Deleted skill: {}", name))
        } else {
            Err(format!(
                "Skill '{}' not found in user skills directory.",
                name
            ))
        }
    }

    /// Returns `~/.omnilauncher/skills/`
    pub fn skill_dir() -> PathBuf {
        path_config::data_dir().join("skills")
    }

    /// Returns the bundled assets/skills directory (relative to the binary).
    pub fn bundled_dir() -> Option<PathBuf> {
        // Try relative to the executable
        if let Ok(exe) = std::env::current_exe() {
            // In dev: <repo>/src-tauri/target/.../omnilauncher
            // Walk up to find assets/skills
            let mut dir = exe.parent()?;
            for _ in 0..6 {
                let candidate = dir.join("assets").join("skills");
                if candidate.exists() {
                    return Some(candidate);
                }
                dir = dir.parent()?;
            }
        }
        // Fallback: relative to cwd
        let cwd = std::env::current_dir().ok()?;
        let candidate = cwd.join("assets").join("skills");
        if candidate.exists() {
            return Some(candidate);
        }
        None
    }
}

impl Default for SkillManager {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Serializable SkillInfo for Tauri commands ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub version: String,
    pub triggers: Vec<String>,
    pub tags: Vec<String>,
    pub tools_hint: Vec<String>,
    pub path: String,
}

impl From<&SkillMeta> for SkillInfo {
    fn from(m: &SkillMeta) -> Self {
        SkillInfo {
            name: m.name.clone(),
            description: m.description.clone(),
            version: m.version.clone(),
            triggers: m.triggers.clone(),
            tags: m.tags.clone(),
            tools_hint: m.tools_hint.clone(),
            path: m.path.display().to_string(),
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"---
name: web-summarizer
description: Fetch and summarize web pages
version: 1.0.0
triggers: [summarize, tldr, summary]
tags: [web, reading]
tools: [web_fetch]
---

When the user asks to summarize a URL, do the following.
"#;

    #[test]
    fn test_parse_frontmatter() {
        let skill = parse_skill_file(SAMPLE, PathBuf::from("test/SKILL.md")).unwrap();
        assert_eq!(skill.meta.name, "web-summarizer");
        assert_eq!(skill.meta.version, "1.0.0");
        assert!(skill.meta.triggers.contains(&"summarize".to_string()));
        assert!(skill.meta.triggers.contains(&"tldr".to_string()));
        assert!(skill.meta.tags.contains(&"web".to_string()));
        assert!(skill.meta.tools_hint.contains(&"web_fetch".to_string()));
        assert!(skill.body.contains("summarize a URL"));
    }

    #[test]
    fn test_find_relevant() {
        let mut mgr = SkillManager::new();
        let skill = parse_skill_file(SAMPLE, PathBuf::from("test/SKILL.md")).unwrap();
        mgr.skills.push(skill);

        let found = mgr.find_relevant("please tldr this article");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.name, "web-summarizer");

        let none = mgr.find_relevant("launch chrome");
        assert_eq!(none.len(), 0);
    }

    #[test]
    fn test_normalize_github_tree_skill_url() {
        let url = "https://github.com/anthropics/skills/tree/main/skills/frontend-design";
        assert_eq!(
            normalize_skill_url(url),
            "https://raw.githubusercontent.com/anthropics/skills/main/skills/frontend-design/SKILL.md"
        );
    }

    #[test]
    fn test_normalize_github_blob_skill_url() {
        let url = "https://github.com/anthropics/skills/blob/main/skills/frontend-design/SKILL.md";
        assert_eq!(
            normalize_skill_url(url),
            "https://raw.githubusercontent.com/anthropics/skills/main/skills/frontend-design/SKILL.md"
        );
    }

    #[test]
    fn test_normalize_enterprise_github_tree_skill_url() {
        let url = "https://ghostshub.example.com/cloud-foundations/cloudbot/tree/dev/backend/skills/jira";
        assert_eq!(
            normalize_skill_url(url),
            "https://ghostshub.example.com/cloud-foundations/cloudbot/raw/dev/backend/skills/jira/SKILL.md"
        );
    }

    #[test]
    fn test_normalize_enterprise_github_blob_skill_url() {
        let url = "https://ghostshub.example.com/cloud-foundations/cloudbot/blob/dev/backend/skills/jira/SKILL.md";
        assert_eq!(
            normalize_skill_url(url),
            "https://ghostshub.example.com/cloud-foundations/cloudbot/raw/dev/backend/skills/jira/SKILL.md"
        );
    }

    #[test]
    fn test_parse_github_tree_dir_directory() {
        let url = "https://ghosthub.example.com/cloud-foundations/cloudbot/tree/dev/backend/skills";
        let (repo, branch, dir) = parse_github_tree_dir(url).expect("should parse tree dir");
        assert_eq!(repo.host, "ghosthub.example.com");
        assert_eq!(repo.owner, "cloud-foundations");
        assert_eq!(repo.repo, "cloudbot");
        assert_eq!(branch, "dev");
        assert_eq!(dir, "backend/skills");
    }

    #[test]
    fn test_parse_github_tree_dir_trailing_slash() {
        let url = "https://github.com/anthropics/skills/tree/main/skills/";
        let (_, branch, dir) = parse_github_tree_dir(url).expect("should parse");
        assert_eq!(branch, "main");
        assert_eq!(dir, "skills");
    }

    #[test]
    fn test_parse_github_tree_dir_rejects_blob_and_skillmd() {
        // blob URLs are single files, not directories.
        assert!(parse_github_tree_dir(
            "https://github.com/anthropics/skills/blob/main/skills/foo/SKILL.md"
        )
        .is_none());
        // tree URL already pointing at a SKILL.md is a single skill, not a dir.
        assert!(parse_github_tree_dir(
            "https://github.com/anthropics/skills/tree/main/skills/foo/SKILL.md"
        )
        .is_none());
    }

    #[test]
    fn test_install_from_path() {
        use std::io::Write;

        // Write a temp SKILL.md file
        let dir = tempfile::tempdir().expect("tmpdir");
        let skill_path = dir.path().join("SKILL.md");
        let mut f = std::fs::File::create(&skill_path).unwrap();
        write!(f, "{}", SAMPLE).unwrap();
        drop(f);

        let mut mgr = SkillManager::new();
        let result = mgr.install_from_path(skill_path.to_str().unwrap());
        assert!(result.is_ok(), "install_from_path failed: {:?}", result);
        assert!(result.unwrap().contains("web-summarizer"));

        // Skill should now be findable
        let found = mgr.find_relevant("tldr this page");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].meta.name, "web-summarizer");

        // Reinstall (overwrite) should also succeed
        let result2 = mgr.install_from_path(skill_path.to_str().unwrap());
        assert!(result2.is_ok());
        assert_eq!(mgr.list_meta().len(), 1); // No duplicates
    }

    /// Live install test against the internal GitHub Enterprise host. Requires
    /// network access and `gh auth login` / git credentials for
    /// `ghosthub.example.com`, so it's `#[ignore]`d by default. Run with:
    ///   cargo test -p omnilauncher install_jira_from_enterprise -- --ignored --nocapture
    #[test]
    #[ignore]
    fn install_jira_from_enterprise() {
        let url = "https://ghosthub.example.com/cloud-foundations/cloudbot/tree/dev/backend/skills/jira";
        let mut mgr = SkillManager::new();
        let result = mgr.install_from_url(url);
        assert!(result.is_ok(), "install_from_url failed: {:?}", result);
        let msg = result.unwrap();
        assert!(msg.contains("jira"), "unexpected message: {msg}");
        assert!(mgr.get_by_name("jira").is_some(), "jira skill not loaded");

        // Verify it was written to disk where the app expects it.
        let installed = SkillManager::skill_dir().join("jira").join("SKILL.md");
        assert!(installed.exists(), "SKILL.md not written to {installed:?}");
    }

    /// Live install of an entire *directory* of skills from the internal GitHub
    /// Enterprise host. Installs every `<name>/SKILL.md` under the folder.
    /// `#[ignore]`d (needs network + gh auth). Run with:
    ///   cargo test -p omnilauncher install_skill_dir_from_enterprise -- --ignored --nocapture
    #[test]
    #[ignore]
    fn install_skill_dir_from_enterprise() {
        let url = "https://ghosthub.example.com/cloud-foundations/cloudbot/tree/dev/backend/skills";
        let mut mgr = SkillManager::new();
        let result = mgr.install_from_url(url);
        assert!(result.is_ok(), "install_from_url failed: {:?}", result);
        let msg = result.unwrap();
        assert!(
            msg.contains("Installed") && msg.contains("skill"),
            "unexpected message: {msg}"
        );
        // The folder holds several skills; expect jira among them.
        assert!(mgr.get_by_name("jira").is_some(), "jira skill not loaded");
    }
}
