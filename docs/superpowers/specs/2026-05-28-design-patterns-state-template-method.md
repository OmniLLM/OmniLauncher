# Design: State + Template Method Patterns
**Date:** 2026-05-28
**Status:** Approved

---

## Goal

Implement two GoF design patterns that have genuine natural fit in the OmniLauncher codebase:

1. **State** — `plugins/pomodoro.rs`: replace string-based mode dispatch with a typed enum + trait
2. **Template Method** — `plugins/external.rs`: extract the repeated call-subprocess skeleton into a shared private method

---

## Pattern 1: State — PomodoroPlugin

### Problem

`PomodoroState.mode` is a `String` (`"work"`, `"short_break"`, `"long_break"`, `"idle"`). All per-mode data (duration, label, icon, notification text) lives in scattered `match s.mode.as_str()` arms across `query()` and `execute_tool()`. Adding a new mode or changing a duration means hunting through the entire file.

### Design

Replace the `String` mode with a `PomodoroMode` enum. Add a `phase()` method on `PomodoroMode` that returns a `PomodoroPhase` struct carrying all per-state data. All string match arms in `query()` collapse to a single `mode.phase()` call.

```rust
/// All timer modes with their associated phase data.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PomodoroMode {
    Work,
    ShortBreak,
    LongBreak,
}

/// Per-state data returned by PomodoroMode::phase().
pub struct PomodoroPhase {
    pub duration_secs: u64,
    pub label: &'static str,
    pub icon: &'static str,
    pub done_title: &'static str,
    pub done_msg: &'static str,
}

impl PomodoroMode {
    pub fn phase(&self) -> PomodoroPhase { ... }
}
```

`PomodoroState.mode` becomes `PomodoroMode`. Serialization stays backward-compatible (`"work"`, `"short_break"`, `"long_break"`) via `#[serde(rename_all = "snake_case")]`.

The idle state (no active timer) is represented by `Option<PomodoroMode>` in `PomodoroState` (or `load_state()` returning `None`), removing the `"idle"` string case entirely.

### Files Changed

- `src-tauri/src/plugins/pomodoro.rs` — full rewrite of the state struct and `query()`/`execute_tool()` logic

### Tests

- Unit test: `PomodoroMode::Work.phase().duration_secs == 25 * 60`
- Unit test: `PomodoroMode::ShortBreak.phase().duration_secs == 5 * 60`
- Unit test: serde round-trip `Work` → `"work"` → `Work`
- Existing behavior preserved: all query results and execute_tool responses unchanged

---

## Pattern 2: Template Method — ExternalPlugin

### Problem

`query()`, `execute_tool()`, and `execute_action()` all follow the same skeleton:

1. Build a JSON request
2. Call `self.call(&input)` with a timeout
3. Log a warning and return empty on timeout or failure
4. Parse the JSON response (different field per method)

Steps 2–3 are copy-pasted verbatim across all three methods (~30 lines each, ~90 lines total of duplication).

### Design

Extract steps 2–3 into a private `call_op` method. Each Plugin method becomes a thin wrapper that handles only steps 1 and 4.

```rust
impl ExternalPlugin {
    /// Template skeleton: send a JSON op to the subprocess, handle timeout/failure.
    /// Returns the raw stdout string, or None on error/timeout.
    async fn call_op(
        &self,
        request: serde_json::Value,
        timeout_secs: u64,
        op_name: &str,
    ) -> Option<String> {
        match timeout(Duration::from_secs(timeout_secs), self.call(&request.to_string())).await {
            Ok(Some(output)) => Some(output),
            Ok(None) => {
                log::warn!("External plugin '{}' {} failed", self.manifest.name, op_name);
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
}
```

`query()` shrinks to: build request → `call_op(..., 3, "query")` → parse `val["results"]`.
`execute_tool()` shrinks to: build request → `call_op(..., 10, "tool_call")` → parse `val["output"]`.
`execute_action()` shrinks to: build request → `call_op(..., 10, "execute")` → parse `val["output"]`.

### Files Changed

- `src-tauri/src/plugins/external.rs` — add `call_op`, rewrite the three Plugin impl methods

### Tests

- All existing external plugin integration tests continue to pass
- No behavior change — purely structural

---

## Execution Order

1. **Task 1:** State pattern in `pomodoro.rs` — self-contained, no dependencies
2. **Task 2:** Template Method in `external.rs` — self-contained, no dependencies

Each task is independently committable.
