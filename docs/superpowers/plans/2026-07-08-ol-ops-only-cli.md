# `ol` Ops-Only CLI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Narrow the `ol` terminal CLI so it only operates the omnilauncher binary (lifecycle/ops verbs), removing the interactive REPL and the launcher-query surface, with one source of truth for the ops verb list.

**Architecture:** Delete `cli/repl.rs` and `cli/query.rs`; strip the query/AI/search/REPL routing out of `cli/mod.rs` and replace the hardcoded help list with a table-driven `OPS_COMMANDS` const that both help and the unknown-command path derive from. Remove the functions in `cli/render.rs` and `cli/process.rs` that only the deleted code used. The GUI launcher (`launcher_config.rs`, `src/**`) and `cli/ops.rs` are untouched.

**Tech Stack:** Rust (Tauri backend, `src-tauri/`), hand-rolled argv dispatcher (no clap derive), `cargo test`.

**Reference spec:** `docs/superpowers/specs/2026-07-08-ol-ops-only-cli-design.md`

---

## File Structure

| File | Responsibility after change |
|---|---|
| `src-tauri/src/cli/mod.rs` | Argv dispatch for ops verbs only; `OPS_COMMANDS` table; table-driven help; unknown-command error. |
| `src-tauri/src/cli/ops.rs` | Lifecycle implementations (unchanged). |
| `src-tauri/src/cli/render.rs` | `Output` presentation (color/glyph/json). Result-table rendering removed. |
| `src-tauri/src/cli/process.rs` | PID/port/health helpers. REPL-history path removed. |
| `src-tauri/src/cli/repl.rs` | **Deleted.** |
| `src-tauri/src/cli/query.rs` | **Deleted.** |

**Ordering rationale:** We edit `mod.rs` first to remove all references to `repl`/`query` (Task 1), which lets those modules be deleted (Task 2) without breaking the build at any intermediate commit. Then we remove the newly-orphaned helpers in `render.rs`/`process.rs` (Task 3). Each task ends green and compiles clean.

---

## Task 1: Make `cli/mod.rs` ops-only (table-driven help + dispatch)

**Files:**
- Modify: `src-tauri/src/cli/mod.rs`

This task removes every reference to the `repl` and `query` modules from `mod.rs`, adds the `OPS_COMMANDS` source of truth, rewrites `print_help`, simplifies the unknown-command path, and updates the tests. After this task `mod.rs` no longer references `repl::` or `query::`, but the module files still exist (deleted in Task 2) — the code compiles because nothing calls them.

- [ ] **Step 1: Update the tests first (they encode the new behavior)**

In `src-tauri/src/cli/mod.rs`, find the `#[cfg(test)] mod tests` block. **Delete** these two tests entirely (they assert behavior we are removing):

Delete `ai_without_text_is_usage_error`:
```rust
    #[test]
    fn ai_without_text_is_usage_error() {
        match dispatch("ol", &s(&["ai"])) {
            Dispatch::Handled(code) => assert_eq!(code, 2),
            _ => panic!("`ai` with no text should be usage error 2"),
        }
    }
```

Delete `shell_duplicate_command_is_rejected_with_redirect`:
```rust
    #[test]
    fn shell_duplicate_command_is_rejected_with_redirect() {
        // `ol grep ...` is no longer a command — it should be a usage error (2),
        // not silently run a reimplementation.
        match dispatch("ol", &s(&["grep", "TODO"])) {
            Dispatch::Handled(code) => assert_eq!(code, 2),
            _ => panic!("shell-duplicating command should be a handled usage error"),
        }
    }
```

Then **add** these three tests inside the same `mod tests` block (e.g. right after `unknown_command_is_usage_error`):
```rust
    #[test]
    fn query_commands_are_unknown_now() {
        // The launcher-query surface (calc/web/…), ai, search, and repl are no
        // longer CLI commands. Each must be a handled usage error (exit 2), not
        // routed anywhere.
        for variant in [
            &["calc", "2+2"][..],
            &["ai", "hi"][..],
            &["search", "x"][..],
            &["repl"][..],
            &["grep", "TODO"][..],
        ] {
            match dispatch("ol", &s(variant)) {
                Dispatch::Handled(code) => assert_eq!(code, 2, "{variant:?} should be usage error 2"),
                _ => panic!("`{variant:?}` should be a handled usage error"),
            }
        }
    }

    #[test]
    fn bare_ol_prints_help_not_repl() {
        // Bare `ol` no longer launches a REPL; it prints ops help and exits 0.
        match dispatch("ol", &[]) {
            Dispatch::Handled(code) => assert_eq!(code, 0),
            _ => panic!("bare `ol` should print help and exit 0"),
        }
    }

    #[test]
    fn help_lists_every_ops_command() {
        // Every OPS_COMMANDS verb name appears in the rendered help text.
        let out = Output::resolve(false, true, false); // no-color for stable matching
        let help = render_help_to_string(&out);
        for c in OPS_COMMANDS {
            assert!(help.contains(c.name), "help missing ops verb '{}'", c.name);
        }
    }
```

- [ ] **Step 2: Run the tests to verify the three new ones fail to compile/pass**

Run: `cd src-tauri && cargo test --lib cli::mod 2>&1 | head -30`
Expected: FAIL — compilation error, because `OPS_COMMANDS` and `render_help_to_string` don't exist yet, and `dispatch("ol", ["ai","hi"])` currently routes `ai` rather than erroring.

- [ ] **Step 3: Add the `OPS_COMMANDS` table**

In `src-tauri/src/cli/mod.rs`, add this immediately above the `pub fn print_help` definition (currently near line 235):
```rust
/// One operational verb: the name as typed on the CLI plus its one-line help.
/// Both `print_help` and the unknown-command path derive from this single list,
/// so the advertised command set and its help text cannot drift apart.
struct OpsCommand {
    name: &'static str,
    desc: &'static str,
}

/// The lifecycle/ops verbs `ol` exposes. `help`/`version` are handled separately
/// (they are not lifecycle verbs). Adding a verb here + a `match` arm in
/// `dispatch_command` is all that's needed to surface a new ops command.
const OPS_COMMANDS: &[OpsCommand] = &[
    OpsCommand { name: "serve",   desc: "run the backend API server (foreground)" },
    OpsCommand { name: "gui",     desc: "launch the desktop shell (--detached to background)" },
    OpsCommand { name: "start",   desc: "start the backend detached and wait for health" },
    OpsCommand { name: "stop",    desc: "stop the backend (--gui shell, --all both)" },
    OpsCommand { name: "restart", desc: "stop then start" },
    OpsCommand { name: "status",  desc: "health / process / port / binary view" },
    OpsCommand { name: "health",  desc: "probe the backend /health endpoint (exit 0 if ok)" },
    OpsCommand { name: "logs",    desc: "print/tail the log file (-f follow, -n N)" },
    OpsCommand { name: "doctor",  desc: "diagnostics: config, token, AI, deps" },
];
```

- [ ] **Step 4: Rewrite `print_help` to be table-driven and add a testable string renderer**

In `src-tauri/src/cli/mod.rs`, replace the entire existing `pub fn print_help` function (currently lines ~235-272, from `pub fn print_help(out: &Output) {` through its closing brace) with:
```rust
/// Print the top-level help / command list to stdout.
pub fn print_help(out: &Output) {
    print!("{}", render_help_to_string(out));
}

/// Render the help text to a string (so it can be asserted in tests).
fn render_help_to_string(out: &Output) -> String {
    let mut s = String::new();
    s.push_str(&format!("{}\n", out.cyan("ol — OmniLauncher CLI")));
    s.push('\n');
    s.push_str(&format!("{}\n", out.dim("USAGE")));
    s.push_str("  ol [FLAGS] [COMMAND] [ARGS...]\n");
    s.push('\n');
    s.push_str(&format!("{}\n", out.dim("GLOBAL FLAGS")));
    s.push_str("  --json        machine-readable JSON output\n");
    s.push_str("  --no-color    disable ANSI color (also NO_COLOR / non-TTY)\n");
    s.push_str("  -q, --quiet   errors only\n");
    s.push_str("  --debug       enable file logging (~/.omnilauncher/omnilauncher.log)\n");
    s.push('\n');
    s.push_str(&format!("{}\n", out.dim("COMMANDS")));
    for c in OPS_COMMANDS {
        s.push_str(&format!("  {:<9} {}\n", c.name, out.dim(c.desc)));
    }
    s.push_str(&format!("  {:<9} {}\n", "help", out.dim("show this help")));
    s.push_str(&format!("  {:<9} {}\n", "version", out.dim("print version")));
    s
}
```

- [ ] **Step 5: Remove the query/AI/search/repl dispatch arms and simplify the unknown-command path**

In `src-tauri/src/cli/mod.rs`, in `dispatch_command`:

Delete the `repl` arm (currently line ~170):
```rust
        "repl" => Dispatch::Handled(repl::run(out)),
```

Delete the entire `ai` and `search` arms block (currently lines ~172-188), from the `// ── Query surface ──` comment through the end of the `search` arm:
```rust
        // ── Query surface ────────────────────────────────────────────────
        "ai" => {
            let prompt = args.join(" ");
            if prompt.trim().is_empty() {
                out.failure("usage: ol ai <text>");
                return Dispatch::Handled(2);
            }
            Dispatch::Handled(query::run_ai(out, &prompt))
        }
        "search" => {
            let text = args.join(" ");
            if text.trim().is_empty() {
                out.failure("usage: ol search <text>");
                return Dispatch::Handled(2);
            }
            Dispatch::Handled(query::run_search(out, &text))
        }
```

Replace the entire generated-subcommand fallthrough `other => { … }` arm (currently lines ~200-222, from `// ── Generated query subcommands from SLASH_COMMANDS ──` through the arm's closing `}`) with:
```rust
        // ── Unknown ──────────────────────────────────────────────────────
        other => {
            out.failure(&format!(
                "unknown command '{other}' — run `ol help` for the command list"
            ));
            Dispatch::Handled(2)
        }
```

- [ ] **Step 6: Replace the bare-`ol` REPL branch with help**

In `src-tauri/src/cli/mod.rs`, in `dispatch`, find this block (currently lines ~121-131):
```rust
    let Some(command) = tokens.first().cloned() else {
        if invoked_as_ol(argv0) {
            if std::io::stdin().is_terminal() {
                return Dispatch::Handled(repl::run(&out));
            }
            print_help(&out);
            return Dispatch::Handled(0);
        }
        // `omnilauncher` with no args → GUI (unchanged).
        return Dispatch::Gui;
    };
```
Replace it with:
```rust
    let Some(command) = tokens.first().cloned() else {
        if invoked_as_ol(argv0) {
            // Bare `ol` (TTY or not) prints the ops help and exits. There is no
            // interactive REPL: `ol` only operates the omnilauncher binary.
            print_help(&out);
            return Dispatch::Handled(0);
        }
        // `omnilauncher` with no args → GUI (unchanged).
        return Dispatch::Gui;
    };
```
Note: `use std::io::IsTerminal;` (line ~22) may now be unused. If `cargo build` warns about it in Step 8, remove that `use` line.

- [ ] **Step 7: Remove the `repl` and `query` module declarations**

In `src-tauri/src/cli/mod.rs`, near the top (lines ~15-19), delete these two lines:
```rust
pub mod query;
```
```rust
pub mod repl;
```
Keep `pub mod ops;`, `pub mod process;`, and `pub mod render;`.

- [ ] **Step 8: Build and run the tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "error|warning: unused" | head -20`
Expected: no `error` lines. If `warning: unused import: std::io::IsTerminal` appears, remove that `use` line from Step 6's file and rebuild. Some warnings about unused functions in `render.rs`/`process.rs` are EXPECTED here (fixed in Task 3) — leave them.

Run: `cd src-tauri && cargo test --lib cli:: 2>&1 | tail -20`
Expected: PASS (all `cli::mod` tests green, including the three new ones).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/cli/mod.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "refactor(cli): make ol ops-only; drop query/repl dispatch, table-driven help"
```

---

## Task 2: Delete the `repl.rs` and `query.rs` modules

**Files:**
- Delete: `src-tauri/src/cli/repl.rs`
- Delete: `src-tauri/src/cli/query.rs`

Task 1 removed every reference to these modules, so they are dead and can be deleted.

- [ ] **Step 1: Confirm nothing references the modules**

Run: `cd src-tauri && grep -rn "repl::\|query::\|mod repl\|mod query" src/`
Expected: no output (empty). If anything prints, it must be resolved before deleting.

- [ ] **Step 2: Delete the files**

```bash
git rm src-tauri/src/cli/repl.rs src-tauri/src/cli/query.rs
```

- [ ] **Step 3: Build to confirm nothing broke**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no `error` lines. (Unused-function warnings from `render.rs`/`process.rs` are still expected — fixed next task.)

- [ ] **Step 4: Commit**

```bash
git add -A
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "refactor(cli): delete repl and query modules (ops-only ol)"
```

---

## Task 3: Remove the now-orphaned helpers in `render.rs` and `process.rs`

**Files:**
- Modify: `src-tauri/src/cli/render.rs`
- Modify: `src-tauri/src/cli/process.rs`

Only `query.rs` called `render_results`/`render_result_table` (and their private helpers `display_width`/`is_wide`), and only `repl.rs` called `repl_history_file`. All are now unused. Remove them and the newly-unused `QueryResult` import.

- [ ] **Step 1: Let the compiler list the orphans**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "never used|unused import" | head`
Expected: warnings for `render_results`, `render_result_table`, `display_width`, `is_wide`, `repl_history_file`, and the `QueryResult` import. This is the authoritative list of what to remove.

- [ ] **Step 2: Remove the result-rendering functions from `render.rs`**

In `src-tauri/src/cli/render.rs`, delete these four items in full:
- `pub fn render_results(...)` (currently lines ~134-161, including its doc comment starting `/// Render an `AiResponse`-shaped result...`)
- `pub fn render_result_table(...)` (currently lines ~163-189, including its doc comment)
- `fn display_width(...)` (currently lines ~191-196, including its doc comment)
- `fn is_wide(...)` (currently lines ~198-203)

- [ ] **Step 3: Remove the now-unused `QueryResult` import from `render.rs`**

In `src-tauri/src/cli/render.rs`, delete line 10:
```rust
use omnilauncher_lib::QueryResult;
```

- [ ] **Step 4: Remove the orphaned `display_width` test from `render.rs`**

In `src-tauri/src/cli/render.rs`, in the `#[cfg(test)] mod tests` block, delete this test (it referenced the removed `display_width`):
```rust
    #[test]
    fn display_width_counts_wide_glyphs_as_two() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("中"), 2); // CJK ideograph is double-width
        assert_eq!(display_width("🚀"), 2); // emoji in the wide range
    }
```
Keep the other three tests in that block (`no_color_glyphs_fall_back_to_ascii`, `no_color_paints_are_plain`, `json_disables_color_on_resolve`).

- [ ] **Step 5: Remove `repl_history_file` from `process.rs`**

In `src-tauri/src/cli/process.rs`, delete this function and its doc comment (currently lines ~31-34):
```rust
/// Persistent REPL history file: `~/.omnilauncher/repl_history`.
pub fn repl_history_file() -> PathBuf {
    omnilauncher_lib::path_config::data_dir().join("repl_history")
}
```
Also update the `run_dir` doc comment (line ~14) which mentions REPL history — change:
```rust
/// Directory holding PID files and the REPL history: `~/.omnilauncher/run/`.
```
to:
```rust
/// Directory holding the PID files: `~/.omnilauncher/run/`.
```

- [ ] **Step 6: Build clean — no warnings, no errors**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "error|warning" | head`
Expected: empty output (a fully clean build). If any `never used`/`unused import` warning remains, remove that item too and rebuild.

- [ ] **Step 7: Run the full backend test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -20`
Expected: PASS (`test result: ok`), no failures.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/cli/render.rs src-tauri/src/cli/process.rs
git commit --author="James Zhu <zhujian0805@gmail.com>" -m "refactor(cli): drop result-table rendering and repl-history helpers (orphaned)"
```

---

## Task 4: Rebuild, reinstall, and smoke-test the real `ol`

**Files:** none (build + manual verification). This also cures the original stale-binary symptom by refreshing the installed binary.

- [ ] **Step 1: Release build**

Run: `cd src-tauri && cargo build --release 2>&1 | tail -5`
Expected: `Finished` with no errors/warnings.

- [ ] **Step 2: Refresh the installed `ol` symlink**

Run: `make install-cli`
Expected: `linked $HOME/.local/bin/ol -> src-tauri/target/release/omnilauncher`.

- [ ] **Step 3: Smoke-test the new surface**

Run: `ol help`
Expected: a COMMANDS section listing only `serve gui start restart stop status health logs doctor help version` — and NONE of `calc web ps ip ports kill color sys clip todo skill open app find grep cat ls git run env ai search repl`.

Run: `ol`
Expected: identical help output, exit 0 (no `omni>` prompt).

Run: `ol calc 2+2 ; echo "exit=$?"`
Expected: `unknown command 'calc' — run \`ol help\` for the command list` and `exit=2`.

Run: `ol repl ; echo "exit=$?"`
Expected: `unknown command 'repl' …` and `exit=2`.

Run: `ol status`
Expected: the normal status view (works exactly as before).

- [ ] **Step 4: Final full-suite gate**

Run: `cd src-tauri && cargo test 2>&1 | tail -5`
Expected: `test result: ok`.

No commit needed (no source changes in this task). If `make install-cli` produced any tracked change, do not commit it.

---

## Self-Review Notes

- **Spec coverage:** §3 behavior → Task 1 (Steps 5-6) + Task 4 smoke tests; §4 OPS_COMMANDS SoT → Task 1 (Steps 3-4); §5 file table → Task 1 (`mod.rs`), Task 2 (`repl.rs`/`query.rs`), Task 3 (`render.rs`/`process.rs`); §6 testing → Task 1 (Step 1) + Task 3 (Step 4); §7 verification → Task 4. `ops.rs`, `launcher_config.rs`, `src/**` never touched — confirmed by task file lists.
- **Type consistency:** `OpsCommand { name, desc }` defined in Task 1 Step 3 and used identically in Steps 4 and Task 1 Step 1's `help_lists_every_ops_command`. `render_help_to_string(&Output) -> String` defined in Step 4, called in Step 1's test and by `print_help`.
- **No placeholders:** every code step shows full code; every run step shows the command and expected output.
