# Design Patterns: State + Template Method Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the State pattern in `pomodoro.rs` (replace string mode dispatch with a typed enum + trait) and the Template Method pattern in `external.rs` (extract the repeated subprocess-call skeleton into a shared private method).

**Architecture:** Two independent refactors. Task 1 replaces `PomodoroState.mode: String` with `PomodoroMode` enum carrying per-state data via a `phase()` method; JSON serialization stays backward-compatible. Task 2 extracts the copy-pasted timeout/error-log skeleton from three `ExternalPlugin` methods into a single `call_op()` helper; no behavior changes.

**Tech Stack:** Rust 2021, `serde` with `#[serde(rename_all = "snake_case")]`, `tokio::time::timeout`, `async_trait`

---

## File Map

| File | Change |
|------|--------|
| `src-tauri/src/plugins/pomodoro.rs` | Replace `mode: String` with `PomodoroMode` enum; add `PomodoroPhase` struct; rewrite `query()` and `execute_tool()` to call `mode.phase()` |
| `src-tauri/src/plugins/external.rs` | Add private `call_op()` method; rewrite `query()`, `execute_tool()`, `execute_action()` as thin wrappers |

---

## Task 1: State Pattern — PomodoroPlugin

**Files:**
- Modify: `src-tauri/src/plugins/pomodoro.rs`

- [ ] **Step 1: Write the failing tests**

Add this `#[cfg(test)]` block at the bottom of `src-tauri/src/plugins/pomodoro.rs`:

```rust
#[cfg(test)]
mod pomodoro_state_tests {
    use super::*;

    #[test]
    fn test_work_phase_duration() {
        assert_eq!(PomodoroMode::Work.phase().duration_secs, 25 * 60);
    }

    #[test]
    fn test_short_break_phase_duration() {
        assert_eq!(PomodoroMode::ShortBreak.phase().duration_secs, 5 * 60);
    }

    #[test]
    fn test_long_break_phase_duration() {
        assert_eq!(PomodoroMode::LongBreak.phase().duration_secs, 15 * 60);
    }

    #[test]
    fn test_mode_serde_round_trip() {
        let serialized = serde_json::to_string(&PomodoroMode::Work).unwrap();
        assert_eq!(serialized, "\"work\"");
        let deserialized: PomodoroMode = serde_json::from_str("\"work\"").unwrap();
        assert_eq!(deserialized, PomodoroMode::Work);

        let serialized = serde_json::to_string(&PomodoroMode::ShortBreak).unwrap();
        assert_eq!(serialized, "\"short_break\"");

        let serialized = serde_json::to_string(&PomodoroMode::LongBreak).unwrap();
        assert_eq!(serialized, "\"long_break\"");
    }

    #[test]
    fn test_work_phase_labels() {
        let p = PomodoroMode::Work.phase();
        assert_eq!(p.label, "🍅 Work");
        assert_eq!(p.icon, "🍅");
        assert!(p.done_title.contains("Pomodoro"));
    }

    #[test]
    fn test_short_break_phase_labels() {
        let p = PomodoroMode::ShortBreak.phase();
        assert_eq!(p.label, "☕ Short Break");
        assert_eq!(p.icon, "☕");
    }

    #[test]
    fn test_long_break_phase_labels() {
        let p = PomodoroMode::LongBreak.phase();
        assert_eq!(p.label, "🛋️ Long Break");
        assert_eq!(p.icon, "🛋️");
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher\src-tauri
cargo test --lib pomodoro_state_tests 2>&1 | Select-Object -Last 10
```

Expected: compile error — `PomodoroMode` and `PomodoroPhase` not defined yet.

- [ ] **Step 3: Replace `PomodoroState` struct and add `PomodoroMode` + `PomodoroPhase`**

In `src-tauri/src/plugins/pomodoro.rs`, replace the existing `PomodoroState` struct definition (lines 20–26) with:

```rust
/// Per-state data for a Pomodoro timer phase — the "state" in the State pattern.
pub struct PomodoroPhase {
    pub duration_secs: u64,
    pub label: &'static str,
    pub icon: &'static str,
    pub done_title: &'static str,
    pub done_msg: &'static str,
}

/// All timer modes. Serializes as snake_case to stay compatible with existing
/// persisted JSON files (`"work"`, `"short_break"`, `"long_break"`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroMode {
    Work,
    ShortBreak,
    LongBreak,
}

impl PomodoroMode {
    /// Return the phase data for this mode — duration, labels, notification text.
    pub fn phase(&self) -> PomodoroPhase {
        match self {
            PomodoroMode::Work => PomodoroPhase {
                duration_secs: 25 * 60,
                label: "🍅 Work",
                icon: "🍅",
                done_title: "🍅 Pomodoro done!",
                done_msg: "Time for a break!",
            },
            PomodoroMode::ShortBreak => PomodoroPhase {
                duration_secs: 5 * 60,
                label: "☕ Short Break",
                icon: "☕",
                done_title: "☕ Break over!",
                done_msg: "Back to work!",
            },
            PomodoroMode::LongBreak => PomodoroPhase {
                duration_secs: 15 * 60,
                label: "🛋️ Long Break",
                icon: "🛋️",
                done_title: "🛋️ Long break over!",
                done_msg: "Ready to focus again?",
            },
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PomodoroState {
    mode: PomodoroMode,
    started_at: i64,
    duration_secs: u64,
    session_count: u32,
}
```

- [ ] **Step 4: Run tests to confirm the new types compile and tests pass**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher\src-tauri
cargo test --lib pomodoro_state_tests 2>&1 | Select-Object -Last 15
```

Expected: all 7 tests pass.

- [ ] **Step 5: Update `query()` to use `mode.phase()`**

In `src-tauri/src/plugins/pomodoro.rs`, replace the entire `async fn query` method body with the following. This removes all `match s.mode.as_str()` arms and replaces them with `s.mode.phase()` calls:

```rust
    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let raw = q.raw.trim();
        let state = load_state();
        let sp = state_path().to_string_lossy().to_string();

        // Build status result from current state (if any)
        let status_result = if let Some(ref s) = state {
            let phase = s.mode.phase();
            let elapsed = elapsed_secs(s.started_at).max(0) as u64;
            let done = elapsed >= s.duration_secs;
            let (icon, subtitle) = if done {
                (
                    "✅",
                    format!("{} — DONE! (session #{})", phase.label, s.session_count),
                )
            } else {
                (
                    "⏱️",
                    format!(
                        "{} — {} remaining (session #{})",
                        phase.label,
                        format_remaining(s.duration_secs, s.started_at),
                        s.session_count
                    ),
                )
            };
            Some(QueryResult {
                id: "pomo:status".to_string(),
                title: format!("{} Pomodoro Status", icon),
                subtitle: Some(subtitle),
                icon: Some(icon.to_string()),
                score: 95,
                action_type: "none".to_string(),
                action_data: String::new(),
            })
        } else {
            None
        };

        let sub = raw.strip_prefix("pomo").unwrap_or("").trim();
        let mut results = vec![];

        if sub.is_empty() || sub.starts_with("st") {
            if let Some(sr) = status_result {
                results.push(sr);
            }
            if sub.is_empty() || "start".starts_with(sub) {
                let phase = PomodoroMode::Work.phase();
                let new_state = PomodoroState {
                    mode: PomodoroMode::Work,
                    started_at: now_secs(),
                    duration_secs: phase.duration_secs,
                    session_count: state.as_ref().map(|s| s.session_count + 1).unwrap_or(1),
                };
                save_state(&new_state);
                let cmd = timer_shell(phase.duration_secs, phase.done_title, phase.done_msg, &sp);
                results.push(QueryResult {
                    id: "pomo:start".to_string(),
                    title: format!("{} Start Pomodoro (25 min)", phase.icon),
                    subtitle: Some("Begin a focused work session".to_string()),
                    icon: Some(phase.icon.to_string()),
                    score: 90,
                    action_type: "shell_bg".to_string(),
                    action_data: cmd,
                });
            }
            if "stop".starts_with(sub) || sub == "stop" {
                results.push(QueryResult {
                    id: "pomo:stop".to_string(),
                    title: "⏹️ Stop Pomodoro".to_string(),
                    subtitle: Some("Cancel the current timer".to_string()),
                    icon: Some("⏹️".to_string()),
                    score: 80,
                    action_type: "callback".to_string(),
                    action_data: "pomo:stop".to_string(),
                });
            }
        }

        if sub.is_empty() || "short".starts_with(sub) {
            let phase = PomodoroMode::ShortBreak.phase();
            let cmd = timer_shell(phase.duration_secs, phase.done_title, phase.done_msg, &sp);
            results.push(QueryResult {
                id: "pomo:short".to_string(),
                title: format!("{} Short Break (5 min)", phase.icon),
                subtitle: Some("Quick breather".to_string()),
                icon: Some(phase.icon.to_string()),
                score: 85,
                action_type: "shell_bg".to_string(),
                action_data: cmd,
            });
        }

        if sub.is_empty() || "long".starts_with(sub) {
            let phase = PomodoroMode::LongBreak.phase();
            let cmd = timer_shell(phase.duration_secs, phase.done_title, phase.done_msg, &sp);
            results.push(QueryResult {
                id: "pomo:long".to_string(),
                title: format!("{} Long Break (15 min)", phase.icon),
                subtitle: Some("After 4 pomodoros".to_string()),
                icon: Some(phase.icon.to_string()),
                score: 82,
                action_type: "shell_bg".to_string(),
                action_data: cmd,
            });
        }

        results
    }
```

- [ ] **Step 6: Update `execute_tool()` to use `mode.phase()`**

Replace the `execute_tool` method body with:

```rust
    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let action = args["command"].as_str().or_else(|| args["action"].as_str()).unwrap_or("");
        match action {
            "stop" => {
                clear_state();
                "Pomodoro timer stopped".to_string()
            }
            "status" => {
                if let Some(s) = load_state() {
                    let phase = s.mode.phase();
                    let remaining = format_remaining(s.duration_secs, s.started_at);
                    format!(
                        "Mode: {}, Remaining: {}, Session: #{}",
                        phase.label, remaining, s.session_count
                    )
                } else {
                    "No active pomodoro".to_string()
                }
            }
            _ => "Unknown action".to_string(),
        }
    }
```

- [ ] **Step 7: Run full test suite to verify no regressions**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher\src-tauri
cargo test --lib 2>&1 | Select-Object -Last 15
```

Expected: all tests pass (the one pre-existing `test_db_add_list_delete` flake only fails when run alongside other scheduler tests due to a shared env-var race — run it in isolation to confirm it passes).

- [ ] **Step 8: Commit**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher
git add src-tauri/src/plugins/pomodoro.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "refactor(pomodoro): State pattern — typed PomodoroMode enum replaces string mode dispatch"
```

---

## Task 2: Template Method Pattern — ExternalPlugin

**Files:**
- Modify: `src-tauri/src/plugins/external.rs`

- [ ] **Step 1: Write the failing test**

Add this `#[cfg(test)]` block at the bottom of `src-tauri/src/plugins/external.rs`:

```rust
#[cfg(test)]
mod external_template_tests {
    use super::*;

    /// Verify that call_op with an instant-failing process returns None
    /// and doesn't panic. We use a manifest pointing to a nonexistent entry
    /// so the spawn fails immediately.
    #[tokio::test]
    async fn test_call_op_returns_none_on_spawn_failure() {
        let plugin = ExternalPlugin::new(
            std::path::PathBuf::from("/nonexistent/dir"),
            PluginManifest {
                name: "test".to_string(),
                description: "test".to_string(),
                version: "0.1.0".to_string(),
                keyword: None,
                icon: None,
                entry: "run.sh".to_string(),
                entry_windows: None,
                tool_schema: None,
            },
        );
        let result = plugin.call_op(
            serde_json::json!({"op": "query", "query": "test"}),
            1,
            "query",
        ).await;
        assert!(result.is_none());
    }
}
```

- [ ] **Step 2: Run test to confirm it fails**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher\src-tauri
cargo test --lib external_template_tests 2>&1 | Select-Object -Last 10
```

Expected: compile error — `call_op` method not defined yet.

- [ ] **Step 3: Add `call_op` template method to `ExternalPlugin`**

In `src-tauri/src/plugins/external.rs`, add the following method to the `impl ExternalPlugin` block (after the existing `call` method, before the closing `}`):

```rust
    /// Template skeleton shared by query / execute_tool / execute_action.
    ///
    /// Sends `request` as JSON on stdin, waits up to `timeout_secs`, and
    /// returns the raw stdout string. Returns `None` on spawn failure, process
    /// error, or timeout — logging a warning in each case.
    async fn call_op(
        &self,
        request: serde_json::Value,
        timeout_secs: u64,
        op_name: &str,
    ) -> Option<String> {
        match timeout(Duration::from_secs(timeout_secs), self.call(&request.to_string())).await {
            Ok(Some(output)) => Some(output),
            Ok(None) => {
                log::warn!(
                    "External plugin '{}' {} failed",
                    self.manifest.name, op_name
                );
                None
            }
            Err(_) => {
                log::warn!(
                    "External plugin '{}' {} timed out ({timeout_secs}s)",
                    self.manifest.name, op_name
                );
                None
            }
        }
    }
```

- [ ] **Step 4: Run test to confirm it passes**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher\src-tauri
cargo test --lib external_template_tests 2>&1 | Select-Object -Last 10
```

Expected: `test external_template_tests::test_call_op_returns_none_on_spawn_failure ... ok`

- [ ] **Step 5: Rewrite `query()` to use `call_op`**

In `src-tauri/src/plugins/external.rs`, replace the entire `async fn query` method in the `impl Plugin for ExternalPlugin` block with:

```rust
    async fn query(&self, q: &Query) -> Vec<QueryResult> {
        let request = serde_json::json!({ "op": "query", "query": q.raw });
        let Some(output) = self.call_op(request, 3, "query").await else {
            return vec![];
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => val["results"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|item| {
                            Some(QueryResult {
                                id: {
                                    let raw_id = item["id"].as_str()?.to_string();
                                    if item["action_type"].as_str() == Some("plugin_execute") {
                                        format!("{}::{}", self.manifest.name, raw_id)
                                    } else {
                                        raw_id
                                    }
                                },
                                title: item["title"].as_str()?.to_string(),
                                subtitle: item["subtitle"].as_str().map(|s| s.to_string()),
                                icon: item["icon"]
                                    .as_str()
                                    .map(|s| s.to_string())
                                    .or_else(|| self.manifest.icon.clone()),
                                score: item["score"].as_i64().unwrap_or(50) as i32,
                                action_type: item["action_type"]
                                    .as_str()
                                    .unwrap_or("shell")
                                    .to_string(),
                                action_data: item["action_data"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default(),
            Err(e) => {
                log::warn!(
                    "External plugin '{}' returned invalid JSON: {e}",
                    self.manifest.name
                );
                vec![]
            }
        }
    }
```

- [ ] **Step 6: Rewrite `execute_tool()` to use `call_op`**

Replace the entire `async fn execute_tool` method with:

```rust
    async fn execute_tool(&self, args: serde_json::Value) -> String {
        let request = serde_json::json!({ "op": "tool_call", "args": args });
        let Some(output) = self.call_op(request, 10, "execute_tool").await else {
            return String::new();
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => val["output"].as_str().unwrap_or(&output).to_string(),
            Err(_) => output,
        }
    }
```

- [ ] **Step 7: Rewrite `execute_action()` to use `call_op`**

Replace the entire `async fn execute_action` method with:

```rust
    async fn execute_action(&self, id: &str, action_data: &str) -> Option<String> {
        let request = serde_json::json!({ "op": "execute", "id": id, "action_data": action_data });
        let Some(output) = self.call_op(request, 10, "execute_action").await else {
            return None;
        };
        match serde_json::from_str::<serde_json::Value>(&output) {
            Ok(val) => Some(val["output"].as_str().unwrap_or("").to_string()),
            Err(_) => Some(output),
        }
    }
```

- [ ] **Step 8: Run full test suite**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher\src-tauri
cargo test --lib 2>&1 | Select-Object -Last 15
```

Expected: all tests pass.

- [ ] **Step 9: Verify the old per-method timeout/warn blocks are fully removed**

```powershell
Select-String -Path C:\Users\jzhu\repos\OmniLauncher\src-tauri\src\plugins\external.rs -Pattern "timed out|execute failed|query failed" | Select-Object LineNumber, Line
```

Expected: only the two lines inside `call_op` match — no duplicates elsewhere.

- [ ] **Step 10: Commit**

```powershell
Set-Location C:\Users\jzhu\repos\OmniLauncher
git add src-tauri/src/plugins/external.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "refactor(external): Template Method pattern — extract call_op skeleton, eliminate 3x duplication"
```

---

## Self-Review

**Spec coverage:**
- ✅ State pattern: `PomodoroMode` enum with `phase()` method — Task 1
- ✅ `PomodoroPhase` struct with `duration_secs`, `label`, `icon`, `done_title`, `done_msg` — Task 1 Step 3
- ✅ Serde backward-compat (`snake_case`) — Task 1 Step 3
- ✅ `query()` uses `mode.phase()` — Task 1 Step 5
- ✅ `execute_tool()` uses `mode.phase()` — Task 1 Step 6
- ✅ Template Method: `call_op` added — Task 2 Step 3
- ✅ All three Plugin methods rewritten as thin wrappers — Task 2 Steps 5–7
- ✅ No behavior changes in either task

**Placeholder scan:** None found.

**Type consistency:** `PomodoroMode`, `PomodoroPhase`, and `call_op` defined in the same task they are first used. `call_op` signature consistent across definition (Step 3) and usage (Steps 5–7).
