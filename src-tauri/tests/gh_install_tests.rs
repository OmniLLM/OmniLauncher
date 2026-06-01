//! End-to-end install smoke tests for gh-CLI integration.
//!
//! These tests hit the real network (GitHub) and require:
//!   - `gh` on PATH and authenticated for github.com
//!   - `git` on PATH
//!
//! They install one Flow.Launcher plugin, one Raycast plugin, and one skill,
//! and assert that the install succeeded and produced the expected files.

use std::path::PathBuf;

use omnilauncher_lib::plugins::plugin_manager_cmd::install_plugin;
use omnilauncher_lib::skills::SkillManager;

fn make_tempdir(label: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ol-ghtest-{label}-{now}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn skip_if_no_network() -> bool {
    // Crude reachability probe — if curl can't hit github.com in 5s, skip.
    let out = std::process::Command::new("curl")
        .args([
            "-fsS",
            "-o",
            "/dev/null",
            "--max-time",
            "5",
            "https://github.com",
        ])
        .status();
    !matches!(out, Ok(s) if s.success())
}

/// Helper: set the plugin base dir env var and run `install_plugin` against it.
async fn install_into(label: &str, source: &str) -> Result<(PathBuf, String), String> {
    let dir = make_tempdir(label);
    // SAFETY: tests are serialized via `--test-threads=1` below; env-var races
    // are avoided because each call sets, runs, and unsets within one async fn.
    unsafe {
        std::env::set_var("OMNILAUNCHER_PLUGIN_BASE_DIR", &dir);
    }
    let res = install_plugin(source.to_string(), None).await;
    unsafe {
        std::env::remove_var("OMNILAUNCHER_PLUGIN_BASE_DIR");
    }
    res.map(|msg| (dir, msg))
}

// ─── Plugin install: bare GitHub URL → gh path ──────────────────────────────

#[tokio::test]
async fn install_flow_launcher_plugin_via_gh() {
    if skip_if_no_network() {
        eprintln!("skipping: no network");
        return;
    }
    // Small public Flow.Launcher plugin (Python — works cross-platform).
    let source = "https://github.com/MoAlSeifi/Flow.Launcher.Plugin.VimCheatSheet";

    let (dir, msg) = match install_into("flow", source).await {
        Ok(v) => v,
        Err(e) => panic!("install_plugin failed: {e}"),
    };
    eprintln!("install_plugin returned: {msg}");

    // Expect at least one subdirectory was created
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    assert!(!entries.is_empty(), "no plugin dir created in {:?}", dir);

    // Expect a plugin.json was synthesized somewhere under the install dir
    let plugin_json_count = walk_count(&dir, "plugin.json");
    assert!(
        plugin_json_count >= 1,
        "expected synthesized plugin.json, found {plugin_json_count} in {:?}",
        dir
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn install_raycast_extension_via_gh() {
    if skip_if_no_network() {
        eprintln!("skipping: no network");
        return;
    }
    // Raycast monorepo subdir URL — exercises the sparse-checkout path.
    // `1loc` is a small JS-snippet extension.
    let source = "https://github.com/raycast/extensions/tree/main/extensions/1loc";

    let (dir, msg) = match install_into("raycast", source).await {
        Ok(v) => v,
        Err(e) => panic!("install_plugin failed: {e}"),
    };
    eprintln!("install_plugin returned: {msg}");

    let plugin_json_count = walk_count(&dir, "plugin.json");
    assert!(
        plugin_json_count >= 1,
        "expected plugin.json, found {plugin_json_count} in {:?}",
        dir
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ─── Skill install: GitHub blob URL → gh api path ───────────────────────────

#[tokio::test]
async fn install_skill_from_github_via_gh() {
    if skip_if_no_network() {
        eprintln!("skipping: no network");
        return;
    }
    // Public anthropics/skills repo, frontend-design SKILL.md
    let url = "https://github.com/anthropics/skills/blob/main/skills/skill-creator/SKILL.md";

    let cfg_dir = make_tempdir("skill");
    unsafe {
        std::env::set_var("OMNILAUNCHER_CONFIG_DIR", &cfg_dir);
    }
    let mut mgr = SkillManager::new();
    let res = mgr.install_from_url(url);
    unsafe {
        std::env::remove_var("OMNILAUNCHER_CONFIG_DIR");
    }

    let msg = res.unwrap_or_else(|e| panic!("install_from_url failed: {e}"));
    eprintln!("install_from_url returned: {msg}");

    // Find SKILL.md somewhere under cfg_dir
    let count = walk_count(&cfg_dir, "SKILL.md");
    assert!(count >= 1, "expected SKILL.md in {:?}", cfg_dir);

    let _ = std::fs::remove_dir_all(&cfg_dir);
}

// ─── helpers ────────────────────────────────────────────────────────────────

fn walk_count(dir: &PathBuf, filename: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                count += walk_count(&p, filename);
            } else if p.file_name().and_then(|n| n.to_str()) == Some(filename) {
                count += 1;
            }
        }
    }
    count
}
