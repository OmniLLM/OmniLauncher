use crate::path_config;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    if let Some(path) = trimmed.strip_prefix("https://github.com/") {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() >= 5 && parts[2] == "blob" {
            return format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                parts[0],
                parts[1],
                parts[3],
                parts[4..].join("/")
            );
        }

        if parts.len() >= 5 && parts[2] == "tree" {
            let skill_path = parts[4..].join("/").trim_end_matches('/').to_string();
            let suffix = if skill_path.ends_with("SKILL.md") {
                skill_path
            } else {
                format!("{}/SKILL.md", skill_path)
            };
            return format!(
                "https://raw.githubusercontent.com/{}/{}/{}/{}",
                parts[0], parts[1], parts[3], suffix
            );
        }
    }

    trimmed.to_string()
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

    /// Find skills relevant to the query by matching triggers and name.
    pub fn find_relevant(&self, query: &str) -> Vec<&Skill> {
        let query_lower = query.to_lowercase();
        self.skills
            .iter()
            .filter(|skill| {
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
            .collect()
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Skill> {
        self.skills.iter().find(|s| s.meta.name == name)
    }

    /// Download and install a skill from a URL.
    pub fn install_from_url(&mut self, url: &str) -> Result<String, String> {
        let download_url = normalize_skill_url(url);

        // Use reqwest in blocking mode via std::process or tokio — but we're in a sync context.
        // We'll delegate to curl/wget as a simple approach.
        let output = std::process::Command::new("curl")
            .args(["-fsSL", &download_url])
            .output()
            .map_err(|e| format!("curl failed: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Download failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let content =
            String::from_utf8(output.stdout).map_err(|e| format!("UTF-8 decode error: {}", e))?;

        // Parse to extract name
        let tmp_path = PathBuf::from("/tmp/SKILL.md");
        let skill = parse_skill_file(&content, tmp_path)
            .ok_or_else(|| "Invalid SKILL.md format".to_string())?;

        let name = skill.meta.name.clone();
        let dest_dir = Self::skill_dir().join(&name);
        std::fs::create_dir_all(&dest_dir).map_err(|e| format!("mkdir failed: {}", e))?;

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
        let url = std::fs::read_to_string(&source_file)
            .map_err(|_| format!("Skill '{}' has no update source (was not installed from a URL).", name))?;
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
            Err(format!("Skill '{}' not found in user skills directory.", name))
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
}
