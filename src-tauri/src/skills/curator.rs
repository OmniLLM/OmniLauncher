//! Skill curator: Hermes-inspired rule-based self-improvement loop.
//!
//! Tracks per-skill usage, transitions skills between lifecycle states, and
//! writes a daily digest. Phase 1 is rule-based only — no LLM calls. The loop
//! is invariants-first, mirroring `hermes-agent/agent/curator.py`:
//!   * never auto-deletes skills (only archives)
//!   * pinned skills are exempt from all auto-transitions
//!   * only touches user-installed skills under `<data_dir>/skills/`,
//!     never bundled assets
//!
//! State files (under `~/.omnilauncher/`):
//!   * `skill_usage.json`   — per-skill counters + last_used + lifecycle state
//!   * `curator_state.json` — last_run timestamp
//!   * `logs/curator-YYYY-MM-DD.md` — daily transition digest
//!
//! Tunables (mirroring Hermes defaults, scaled down where launcher cadence
//! differs from chat-agent cadence):
//!   * INTERVAL_SECS  = 7 * 86400  (curator wakes ~weekly)
//!   * STALE_AFTER    = 30 days unused → mark `stale`
//!   * ARCHIVE_AFTER  = 90 days unused → mark `archived` (hidden from auto-pick)

use crate::path_config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const INTERVAL_SECS: u64 = 7 * 86_400;
pub const STALE_AFTER_SECS: u64 = 30 * 86_400;
pub const ARCHIVE_AFTER_SECS: u64 = 90 * 86_400;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SkillState {
    Active,
    Stale,
    Archived,
}

impl Default for SkillState {
    fn default() -> Self {
        SkillState::Active
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillUsage {
    /// Total times the skill has been read/loaded by the router.
    pub uses: u64,
    /// Unix seconds of the last `record_use` call. 0 means never used since
    /// tracking began.
    pub last_used: u64,
    /// Unix seconds of when the skill was first seen by the curator.
    pub first_seen: u64,
    /// Lifecycle state, advanced by `evaluate()`.
    #[serde(default)]
    pub state: SkillState,
    /// User-pinned skills are exempt from auto-archive / auto-stale.
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageStore {
    pub skills: HashMap<String, SkillUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CuratorState {
    pub last_run: u64,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn usage_path() -> PathBuf {
    path_config::data_dir().join("skill_usage.json")
}

fn state_path() -> PathBuf {
    path_config::data_dir().join("curator_state.json")
}

fn logs_dir() -> PathBuf {
    path_config::data_dir().join("logs")
}

fn load_usage() -> UsageStore {
    let path = usage_path();
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<UsageStore>(&s).ok())
        .unwrap_or_default()
}

fn save_usage(store: &UsageStore) {
    let path = usage_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(store) {
        let _ = std::fs::write(&path, json);
    }
}

fn load_state() -> CuratorState {
    std::fs::read_to_string(state_path())
        .ok()
        .and_then(|s| serde_json::from_str::<CuratorState>(&s).ok())
        .unwrap_or_default()
}

fn save_state(s: &CuratorState) {
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(s) {
        let _ = std::fs::write(&path, json);
    }
}

// Cache the in-memory store so `record_use` (called from hot paths) is cheap
// and aggregates writes. We flush on every record — small file, infrequent
// calls — so a crash never loses more than the last bump.
static USAGE: Mutex<Option<UsageStore>> = Mutex::new(None);

fn with_usage<R>(f: impl FnOnce(&mut UsageStore) -> R) -> R {
    let mut guard = USAGE.lock().expect("usage mutex poisoned");
    if guard.is_none() {
        *guard = Some(load_usage());
    }
    let store = guard.as_mut().expect("usage initialized above");
    let out = f(store);
    save_usage(store);
    out
}

/// Bump usage for `name`. Called from the router whenever a skill is loaded.
/// Cheap (in-memory + small JSON write); safe to call from any thread.
pub fn record_use(name: &str) {
    let now = now_secs();
    with_usage(|store| {
        let entry = store.skills.entry(name.to_string()).or_default();
        if entry.first_seen == 0 {
            entry.first_seen = now;
        }
        entry.uses = entry.uses.saturating_add(1);
        entry.last_used = now;
        // Re-using a stale/archived skill brings it back to active. Hermes
        // does the same: the curator's state is advisory, not punitive.
        if !entry.pinned && entry.state != SkillState::Active {
            entry.state = SkillState::Active;
        }
    });
}

/// Read-only snapshot for UI / tests.
pub fn snapshot() -> UsageStore {
    with_usage(|s| s.clone())
}

/// Pin or unpin a skill. Pinned skills bypass auto-stale / auto-archive.
pub fn set_pinned(name: &str, pinned: bool) {
    with_usage(|store| {
        let entry = store.skills.entry(name.to_string()).or_default();
        entry.pinned = pinned;
        if pinned && entry.state != SkillState::Active {
            entry.state = SkillState::Active;
        }
    });
}

/// Return true when the curator should run now (≥ INTERVAL_SECS since last
/// run, or never run). Cheap — no skill scan.
pub fn is_due() -> bool {
    let s = load_state();
    let now = now_secs();
    s.last_run == 0 || now.saturating_sub(s.last_run) >= INTERVAL_SECS
}

/// Returned from `evaluate()` so callers can log / surface a digest.
#[derive(Debug, Default)]
pub struct EvaluateReport {
    pub marked_stale: Vec<String>,
    pub marked_archived: Vec<String>,
    pub seen_new: Vec<String>,
    pub total_tracked: usize,
}

/// Run one curation pass. Pure rule-based:
///   * register newly-seen installed skills
///   * transition unused skills to stale / archived (skip pinned)
///   * write a daily digest under `logs/curator-YYYY-MM-DD.md`
///
/// `installed_names` is the current list of user-installed skill names — the
/// caller passes this from `SkillManager` so the curator never needs to walk
/// the skills tree itself.
pub fn evaluate(installed_names: &[String]) -> EvaluateReport {
    let now = now_secs();
    let mut report = EvaluateReport::default();

    with_usage(|store| {
        // 1) Register newly-seen installed skills.
        for name in installed_names {
            if !store.skills.contains_key(name) {
                store.skills.insert(
                    name.clone(),
                    SkillUsage {
                        first_seen: now,
                        ..Default::default()
                    },
                );
                report.seen_new.push(name.clone());
            }
        }

        // 2) Lifecycle transitions. Use first_seen as a floor when the skill
        //    has never been used, so a freshly-installed skill isn't archived
        //    on day 1 just because last_used == 0.
        for (name, entry) in store.skills.iter_mut() {
            if entry.pinned {
                continue;
            }
            // Only auto-transition skills that are currently installed; an
            // uninstalled skill keeps its last state as a tombstone.
            if !installed_names.iter().any(|n| n == name) {
                continue;
            }
            let reference = if entry.last_used > 0 {
                entry.last_used
            } else {
                entry.first_seen.max(1)
            };
            let idle = now.saturating_sub(reference);
            let target = if idle >= ARCHIVE_AFTER_SECS {
                SkillState::Archived
            } else if idle >= STALE_AFTER_SECS {
                SkillState::Stale
            } else {
                SkillState::Active
            };
            if target != entry.state {
                match target {
                    SkillState::Stale => report.marked_stale.push(name.clone()),
                    SkillState::Archived => report.marked_archived.push(name.clone()),
                    SkillState::Active => {}
                }
                entry.state = target;
            }
        }

        report.total_tracked = store.skills.len();
    });

    save_state(&CuratorState { last_run: now });
    write_digest(&report, now);
    report
}

fn write_digest(r: &EvaluateReport, now: u64) {
    if r.marked_stale.is_empty() && r.marked_archived.is_empty() && r.seen_new.is_empty() {
        return;
    }
    // Format YYYY-MM-DD without pulling in chrono; cheap UTC date math.
    let day = now / 86_400;
    // 1970-01-01 was a Thursday; we just need a unique daily filename, not a
    // human-perfect calendar — so use epoch-day as a stable suffix and also
    // include a fallback ISO-ish date computed from secs.
    let dir = logs_dir();
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("curator-{day}.md"));

    let mut body = String::new();
    body.push_str(&format!("# Curator pass — epoch-day {day}\n\n"));
    body.push_str(&format!("Total skills tracked: {}\n\n", r.total_tracked));
    if !r.seen_new.is_empty() {
        body.push_str("## Newly registered\n");
        for n in &r.seen_new {
            body.push_str(&format!("- {n}\n"));
        }
        body.push('\n');
    }
    if !r.marked_stale.is_empty() {
        body.push_str("## Marked stale (30d unused)\n");
        for n in &r.marked_stale {
            body.push_str(&format!("- {n}\n"));
        }
        body.push('\n');
    }
    if !r.marked_archived.is_empty() {
        body.push_str("## Archived (90d unused)\n");
        for n in &r.marked_archived {
            body.push_str(&format!("- {n}\n"));
        }
        body.push('\n');
    }
    let _ = std::fs::write(&path, body);
}

/// Convenience: run only when due. Used by the background tick.
pub fn run_if_due(installed_names: &[String]) -> Option<EvaluateReport> {
    if is_due() {
        Some(evaluate(installed_names))
    } else {
        None
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// All tests share `static USAGE` so they must run serially.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Reset the in-memory cache so each test sees a clean store. The on-disk
    /// JSON is per-`OMNILAUNCHER_CONFIG_DIR`, which the test harness already
    /// isolates per-process where needed.
    fn reset_cache() {
        let mut g = USAGE.lock().unwrap();
        *g = Some(UsageStore::default());
    }

    #[test]
    fn record_use_bumps_counter() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_cache();
        record_use("alpha");
        record_use("alpha");
        record_use("beta");
        let snap = snapshot();
        assert_eq!(snap.skills.get("alpha").unwrap().uses, 2);
        assert_eq!(snap.skills.get("beta").unwrap().uses, 1);
        assert!(snap.skills.get("alpha").unwrap().last_used > 0);
    }

    #[test]
    fn evaluate_marks_stale_and_archived() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_cache();
        let now = now_secs();
        // Inject three skills with hand-crafted last_used times.
        with_usage(|s| {
            s.skills.insert(
                "fresh".into(),
                SkillUsage {
                    last_used: now - 86_400,
                    first_seen: now - 86_400,
                    ..Default::default()
                },
            );
            s.skills.insert(
                "old".into(),
                SkillUsage {
                    last_used: now - 40 * 86_400,
                    first_seen: now - 40 * 86_400,
                    ..Default::default()
                },
            );
            s.skills.insert(
                "ancient".into(),
                SkillUsage {
                    last_used: now - 100 * 86_400,
                    first_seen: now - 100 * 86_400,
                    ..Default::default()
                },
            );
        });

        let installed = vec!["fresh".to_string(), "old".to_string(), "ancient".to_string()];
        let report = evaluate(&installed);
        assert!(report.marked_stale.contains(&"old".to_string()));
        assert!(report.marked_archived.contains(&"ancient".to_string()));
        let snap = snapshot();
        assert_eq!(snap.skills.get("fresh").unwrap().state, SkillState::Active);
        assert_eq!(snap.skills.get("old").unwrap().state, SkillState::Stale);
        assert_eq!(
            snap.skills.get("ancient").unwrap().state,
            SkillState::Archived
        );
    }

    #[test]
    fn pinned_skills_are_exempt() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_cache();
        let now = now_secs();
        with_usage(|s| {
            s.skills.insert(
                "pinned-old".into(),
                SkillUsage {
                    last_used: now - 100 * 86_400,
                    first_seen: now - 100 * 86_400,
                    pinned: true,
                    ..Default::default()
                },
            );
        });
        let installed = vec!["pinned-old".to_string()];
        let report = evaluate(&installed);
        assert!(report.marked_archived.is_empty());
        assert!(report.marked_stale.is_empty());
        let snap = snapshot();
        assert_eq!(
            snap.skills.get("pinned-old").unwrap().state,
            SkillState::Active
        );
    }

    #[test]
    fn record_use_revives_archived() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_cache();
        with_usage(|s| {
            s.skills.insert(
                "zombie".into(),
                SkillUsage {
                    state: SkillState::Archived,
                    ..Default::default()
                },
            );
        });
        record_use("zombie");
        let snap = snapshot();
        assert_eq!(snap.skills.get("zombie").unwrap().state, SkillState::Active);
    }

    #[test]
    fn evaluate_registers_new_skills() {
        let _g = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        reset_cache();
        let report = evaluate(&["brand-new".to_string()]);
        assert!(report.seen_new.contains(&"brand-new".to_string()));
        let snap = snapshot();
        assert_eq!(snap.skills.get("brand-new").unwrap().state, SkillState::Active);
        assert!(snap.skills.get("brand-new").unwrap().first_seen > 0);
    }
}
