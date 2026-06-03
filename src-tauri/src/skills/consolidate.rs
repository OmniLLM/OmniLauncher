//! LLM-driven skill consolidation (phase 2 of the Hermes-style curator).
//!
//! Hermes runs a periodic curator that asks an LLM to look at the full skill
//! library and suggest *consolidations* — merging two skills that overlap,
//! rewriting a stale skill that's been outgrown by a sibling, or archiving
//! something that's clearly redundant.
//!
//! **Invariant: this module never writes anything on its own.** It produces
//! [`Proposal`] values that the UI surfaces to the user; only after explicit
//! per-proposal approval does [`apply`] mutate disk. Every write makes a
//! timestamped backup under `<data_dir>/skill_backups/` so any apply is
//! reversible.
//!
//! This is the "confirm-before-write" half of the curator. The rule-based
//! half (lifecycle states, daily digest) lives in `curator.rs` and runs on a
//! schedule; this half is strictly user-initiated.

use crate::ai::client::{AiClient, Message};
use crate::ai::errors::AiError;
use crate::path_config;
use crate::skills::{Skill, SkillManager};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// A consolidation suggestion from the LLM. Tagged enum so the frontend can
/// switch on `kind` and render a tailored confirm dialog per type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Proposal {
    /// Merge `secondary` into `primary`: write `merged_body` to primary's
    /// SKILL.md and delete the secondary skill folder.
    Merge {
        primary: String,
        secondary: String,
        rationale: String,
        merged_body: String,
    },
    /// Rewrite a single skill in place — replace its SKILL.md body with
    /// `new_body`. Used when the LLM thinks a skill has drifted, has stale
    /// instructions, or could be substantially tightened.
    Rewrite {
        name: String,
        rationale: String,
        new_body: String,
    },
    /// Suggest archiving `name` even though the rule-based curator hasn't
    /// (e.g. content-based judgement: "this is obsolete because skill X
    /// covers it more cleanly"). Apply just calls [`curator::set_pinned`]'s
    /// inverse equivalent — flips state to Archived without deleting files.
    Archive { name: String, rationale: String },
}

/// Result of [`apply`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyOutcome {
    pub message: String,
    /// Backup path(s) written before the mutation, for the UI to surface.
    pub backups: Vec<String>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn backup_dir() -> PathBuf {
    path_config::data_dir().join("skill_backups")
}

/// Copy a skill's `SKILL.md` to the backup dir with a timestamped name.
/// Returns the backup path on success. Best-effort: if no SKILL.md exists,
/// returns `Ok(None)` and the caller proceeds (e.g. apply on a phantom name).
fn backup_skill_md(name: &str) -> Result<Option<PathBuf>, String> {
    let src = SkillManager::skill_dir().join(name).join("SKILL.md");
    if !src.exists() {
        return Ok(None);
    }
    let dir = backup_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("backup mkdir failed: {e}"))?;
    let dst = dir.join(format!("{name}-{}.md", now_secs()));
    std::fs::copy(&src, &dst).map_err(|e| format!("backup copy failed: {e}"))?;
    Ok(Some(dst))
}

/// Build the system + user messages we send to the LLM. Kept as a free
/// function so tests can pin the prompt without making real network calls.
pub fn build_messages(skills: &[Skill]) -> Vec<Message> {
    let mut catalog = String::new();
    for s in skills {
        catalog.push_str(&format!(
            "### {}\n- description: {}\n- triggers: {}\n- tags: {}\n- body (first 800 chars):\n{}\n\n",
            s.meta.name,
            s.meta.description,
            s.meta.triggers.join(", "),
            s.meta.tags.join(", "),
            s.body.chars().take(800).collect::<String>(),
        ));
    }

    let system = "You are a skill librarian for OmniLauncher. \
You look at the user's installed skills and suggest *consolidations* that \
reduce redundancy and keep the library tight. You NEVER write anything \
yourself — your output is a JSON array of proposals that the user reviews \
and approves one at a time.\n\n\
Output rules:\n\
1. Respond with ONLY a JSON array. No prose, no markdown fences, no \
preamble. The first character of your reply must be `[`.\n\
2. Each item has a `kind` of `merge`, `rewrite`, or `archive`.\n\
3. `merge` items have fields: kind, primary, secondary, rationale, \
merged_body. `merged_body` is the full SKILL.md body (without YAML \
frontmatter — just the markdown after `---`).\n\
4. `rewrite` items have fields: kind, name, rationale, new_body. `new_body` \
is the full body without frontmatter.\n\
5. `archive` items have fields: kind, name, rationale.\n\
6. Be CONSERVATIVE: only propose a change when there is clear redundancy or \
clear staleness. If everything looks healthy, return `[]`.\n\
7. Never propose merging or archiving a skill if its body looks load-bearing \
and irreplaceable. When in doubt, skip it.";

    let user = format!(
        "Here is the current skill library ({} skills):\n\n{}\n\
Return your JSON array of consolidation proposals.",
        skills.len(),
        catalog
    );

    vec![Message::system(system), Message::user(&user)]
}

/// Parse the LLM's JSON-array reply into [`Proposal`] values. Tolerates
/// minor noise: leading/trailing whitespace, accidental ```json fences, and
/// items that fail to parse (those are skipped rather than aborting).
pub fn parse_proposals(reply: &str) -> Vec<Proposal> {
    let trimmed = reply.trim();
    // Strip ```json ... ``` fences if the model added them despite the
    // instruction to omit them.
    let cleaned = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_start())
        .unwrap_or(trimmed);
    let cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned).trim();

    // First try strict parse.
    if let Ok(v) = serde_json::from_str::<Vec<Proposal>>(cleaned) {
        return v;
    }
    // Fall back: parse as array of arbitrary values, drop the bad ones.
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(cleaned) {
        return arr
            .into_iter()
            .filter_map(|v| serde_json::from_value::<Proposal>(v).ok())
            .collect();
    }
    Vec::new()
}

/// Ask the LLM for proposals. Read-only — no disk mutation. Returns an
/// empty vec if the model declined to suggest anything (the desired
/// "library is healthy" response).
pub async fn propose(skills: &[Skill], ai: &AiClient) -> Result<Vec<Proposal>, AiError> {
    if skills.is_empty() {
        return Ok(Vec::new());
    }
    let messages = build_messages(skills);
    let reply = ai.chat(messages).await?;
    Ok(parse_proposals(&reply))
}

/// Apply a single user-approved proposal. Always backs up before writing.
/// After mutation, the caller should [`SkillManager::reload`] to reflect
/// the change in memory.
pub fn apply(proposal: &Proposal, mgr: &mut SkillManager) -> Result<ApplyOutcome, String> {
    match proposal {
        Proposal::Merge {
            primary,
            secondary,
            merged_body,
            ..
        } => apply_merge(primary, secondary, merged_body, mgr),
        Proposal::Rewrite { name, new_body, .. } => apply_rewrite(name, new_body, mgr),
        Proposal::Archive { name, .. } => apply_archive(name),
    }
}

fn apply_merge(
    primary: &str,
    secondary: &str,
    merged_body: &str,
    mgr: &mut SkillManager,
) -> Result<ApplyOutcome, String> {
    if primary == secondary {
        return Err("merge: primary and secondary must differ".into());
    }
    let primary_md = SkillManager::skill_dir().join(primary).join("SKILL.md");
    if !primary_md.exists() {
        return Err(format!("merge: primary skill '{primary}' has no SKILL.md"));
    }
    let mut backups = Vec::new();
    if let Some(p) = backup_skill_md(primary)? {
        backups.push(p.to_string_lossy().into_owned());
    }
    if let Some(p) = backup_skill_md(secondary)? {
        backups.push(p.to_string_lossy().into_owned());
    }

    // Preserve frontmatter on primary; replace only the body.
    let original = std::fs::read_to_string(&primary_md)
        .map_err(|e| format!("merge: read primary failed: {e}"))?;
    let new_content = swap_body(&original, merged_body)?;
    std::fs::write(&primary_md, new_content)
        .map_err(|e| format!("merge: write primary failed: {e}"))?;

    // Delete the secondary skill via the manager (best-effort: missing
    // secondary just means the merge already happened — succeed loudly).
    let del_msg = match mgr.delete_skill(secondary) {
        Ok(m) => m,
        Err(e) if e.contains("not found") => format!("(secondary '{secondary}' was already gone)"),
        Err(e) => return Err(format!("merge: delete secondary failed: {e}")),
    };

    mgr.reload();
    Ok(ApplyOutcome {
        message: format!("Merged '{secondary}' into '{primary}'. {del_msg}"),
        backups,
    })
}

fn apply_rewrite(
    name: &str,
    new_body: &str,
    mgr: &mut SkillManager,
) -> Result<ApplyOutcome, String> {
    let path = SkillManager::skill_dir().join(name).join("SKILL.md");
    if !path.exists() {
        return Err(format!("rewrite: skill '{name}' has no SKILL.md"));
    }
    let mut backups = Vec::new();
    if let Some(p) = backup_skill_md(name)? {
        backups.push(p.to_string_lossy().into_owned());
    }
    let original =
        std::fs::read_to_string(&path).map_err(|e| format!("rewrite: read failed: {e}"))?;
    let new_content = swap_body(&original, new_body)?;
    std::fs::write(&path, new_content).map_err(|e| format!("rewrite: write failed: {e}"))?;

    mgr.reload();
    Ok(ApplyOutcome {
        message: format!("Rewrote '{name}'."),
        backups,
    })
}

fn apply_archive(name: &str) -> Result<ApplyOutcome, String> {
    // Use the curator's state machine — same path the rule-based pass uses.
    crate::skills::curator::set_state_archived(name);
    Ok(ApplyOutcome {
        message: format!("Archived '{name}' (files untouched)."),
        backups: Vec::new(),
    })
}

/// Replace the body of a SKILL.md file (everything after the closing `---`
/// of the YAML frontmatter) with `new_body`. Preserves the frontmatter
/// verbatim so triggers/tags/version are not lost.
fn swap_body(original: &str, new_body: &str) -> Result<String, String> {
    // Locate frontmatter: `---\n...\n---\n`
    let trimmed = original.trim_start_matches('\u{feff}'); // strip BOM if present
    if !trimmed.starts_with("---") {
        return Err("swap_body: file has no YAML frontmatter".into());
    }
    // Skip past the opening `---` line.
    let after_open = match trimmed.find('\n') {
        Some(i) => &trimmed[i + 1..],
        None => {
            return Err("swap_body: malformed frontmatter (no newline after opening ---)".into())
        }
    };
    // Find the closing `---` line.
    let close_rel = after_open
        .find("\n---")
        .ok_or_else(|| "swap_body: malformed frontmatter (no closing ---)".to_string())?;
    let frontmatter_body_end = close_rel + after_open[close_rel..].find('\n').unwrap_or(4);
    let frontmatter = &trimmed[..(trimmed.len() - after_open.len() + frontmatter_body_end)];

    let mut out = String::with_capacity(frontmatter.len() + new_body.len() + 2);
    out.push_str(frontmatter);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !new_body.starts_with('\n') {
        out.push('\n');
    }
    out.push_str(new_body);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strict_array() {
        let s = r#"[{"kind":"archive","name":"foo","rationale":"obsolete"}]"#;
        let v = parse_proposals(s);
        assert_eq!(v.len(), 1);
        match &v[0] {
            Proposal::Archive { name, rationale } => {
                assert_eq!(name, "foo");
                assert_eq!(rationale, "obsolete");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_with_fences_and_whitespace() {
        let s = "```json\n[\n  {\"kind\":\"rewrite\",\"name\":\"a\",\"rationale\":\"r\",\"new_body\":\"b\"}\n]\n```";
        let v = parse_proposals(s);
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn parse_drops_bad_items_keeps_good() {
        let s = r#"[
            {"kind":"archive","name":"good","rationale":"ok"},
            {"kind":"unknown_kind","name":"bad"},
            {"kind":"merge","primary":"p","secondary":"s","rationale":"r","merged_body":"b"}
        ]"#;
        let v = parse_proposals(s);
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn parse_empty_returns_empty() {
        assert!(parse_proposals("[]").is_empty());
        assert!(parse_proposals("not json at all").is_empty());
        assert!(parse_proposals("").is_empty());
    }

    #[test]
    fn swap_body_preserves_frontmatter() {
        let original = "---\nname: foo\nversion: 1\n---\nold body\n";
        let out = swap_body(original, "new body line").unwrap();
        assert!(out.contains("name: foo"));
        assert!(out.contains("version: 1"));
        assert!(out.contains("new body line"));
        assert!(!out.contains("old body"));
    }

    #[test]
    fn swap_body_rejects_no_frontmatter() {
        assert!(swap_body("just a body\n", "new").is_err());
    }

    #[test]
    fn build_messages_includes_all_skills() {
        use crate::skills::SkillMeta;
        let s = vec![Skill {
            meta: SkillMeta {
                name: "alpha".into(),
                description: "the alpha skill".into(),
                version: "1".into(),
                triggers: vec!["trig".into()],
                tags: vec!["t".into()],
                tools_hint: vec![],
                path: PathBuf::from("/tmp"),
            },
            body: "body of alpha".into(),
        }];
        let msgs = build_messages(&s);
        assert_eq!(msgs.len(), 2);
        let user = msgs[1].content_str();
        assert!(user.contains("alpha"));
        assert!(user.contains("the alpha skill"));
        assert!(user.contains("body of alpha"));
    }
}
