# `ol` Ops-Only CLI — Design

**Date:** 2026-07-08
**Status:** Approved (pending implementation)
**Scope:** Narrow the `ol` terminal CLI so it *only* operates the omnilauncher binary/backend (lifecycle/ops verbs). Remove the interactive REPL and the entire launcher-query surface (`ai`, `search`, and the generated `/calc`, `/web`, `/ps`, … subcommands) from the CLI. The GUI launcher is untouched.

> **Relationship to prior design:** This reverses one goal of
> `2026-07-08-omnilauncher-cli-design.md`, which exposed the GUI's plugin/slash-command
> catalog from the terminal and added a REPL. In practice those query features are noise
> in a terminal — the user already has a shell — so the CLI is refocused on what only the
> binary can do: manage its own lifecycle. The self-contained-binary and consistent-output
> goals of the prior design are **kept**.

---

## 1. Motivation

`ol` currently derives its command surface from the shared GUI catalog
(`launcher_config::SLASH_COMMANDS`): the CLI and REPL expose `calc`, `web`, `ps`, `ip`,
`ports`, `kill`, `color`, `sys`, `clip`, `todo`, `skill`, `open`, `app`, `find`, plus
`ai`/`search` and an interactive `omni>` REPL. These make sense in the GUI (there is no
shell there) but are redundant in a terminal, where the user already has `grep`, `cat`,
`ps`, a calculator, a browser, etc.

The symptom that surfaced this: `ol` → `help` listed shell-duplicating commands
(`/run`, `/grep`, `/cat`, `/ls`, `/git`, `/env`) that a later commit had already hidden —
a stale binary made the drift visible. The deeper issue is that the CLI's identity is
wrong: it should be a **controller for the omnilauncher binary**, not a second-class
re-implementation of shell utilities.

## 2. Goals / Non-goals

**Goals**
- `ol` exposes only lifecycle/ops verbs: `serve`, `gui`, `start`, `stop`, `restart`,
  `status`, `health`, `logs`, `doctor`, plus `help`/`version`.
- Bare `ol` on a TTY prints the ops help and exits (no interactive REPL).
- One source of truth for the ops verb list, consumed by both help and (for the unknown-
  command path) dispatch, so the command list and its help text cannot drift.
- Delete the now-dead query/REPL code rather than leaving it unrouted.

**Non-goals**
- No change to the GUI launcher or its slash-command palette.
- No change to the HTTP backend, `--server`, `--debug`, or split-machine launch paths.
- No change to `ops.rs` behavior (start/stop/status/etc. keep working as-is).
- Not re-litigating the prior design's build/symlink/logging decisions.

## 3. Behavior change

**Kept (all lifecycle verbs):** `serve`, `gui`, `start`, `stop` (incl. `--gui`/`--all`),
`restart`, `status`, `health`, `logs` (incl. `-f`/`-n`), `doctor`, `help`/`--help`/`-h`,
`version`/`--version`/`-V`.

**Removed from the CLI:**
- The interactive REPL (`ol` bare on a TTY, and the `repl` subcommand).
- `ai` and `search` subcommands.
- Every generated launcher-query subcommand derived from `SLASH_COMMANDS`
  (`calc`, `todo`, `web`, `ip`, `ports`, `ps`, `kill`, `color`, `sys`, `clip`, `skill`,
  `open`, `app`, `find`).
- The `SHELL_DUPLICATE_COMMANDS` redirect machinery (`/run`, `/grep`, `/cat`, `/ls`,
  `/git`, `/env`) — it only existed to explain those away in the REPL/CLI.

**Bare `ol` (no subcommand):**
- Invoked as `ol` on a TTY → print ops help, exit 0 (previously: launch REPL).
- Invoked as `ol` non-TTY → print ops help, exit 0 (unchanged).
- Invoked as `omnilauncher` with no args → GUI (unchanged).

**Unknown command:** `ol <anything-else>` → `failure("unknown command '<x>' — run \`ol help\`
for the command list")`, exit 2. No catalog lookup, no shell-duplicate special case.

**GUI:** Unaffected. The frontend reads its catalog via `launcher_config.rs` →
`get_launcher_config` (`src/launcherConfig.ts`); no CLI query code is shared with it.
Verified: `ops.rs` never imports `query`.

## 4. Design: single source of truth for ops verbs (Approach B)

Introduce one table in `cli/mod.rs` consumed by help:

```rust
/// One operational verb: the name as typed on the CLI plus its one-line help.
/// Both `print_help` and the unknown-command error path derive from this list,
/// so the advertised command set and its help text cannot drift.
struct OpsCommand {
    name: &'static str,
    desc: &'static str,
}

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

- `print_help` iterates `OPS_COMMANDS` for its LIFECYCLE section (replacing the current
  hardcoded array), and drops the QUERY section entirely. Global-flags section is kept.
- `dispatch_command` keeps its explicit `match` arms per verb — each has distinct
  flags/args (`stop --all`, `logs -n`), so a data-driven dispatch would obscure rather
  than clarify. Only the fallthrough (`other =>`) is simplified to the unknown-command
  error above.
- Adding a future ops verb = one `OPS_COMMANDS` entry + one `match` arm.

`help`/`version` remain handled directly (they are not lifecycle verbs and don't belong in
the LIFECYCLE list).

## 5. Files touched

| File | Change |
|---|---|
| `src-tauri/src/cli/repl.rs` | **Delete** (~346 lines). REPL removed. |
| `src-tauri/src/cli/query.rs` | **Delete** (~269 lines). Query/AI/search routing, `cli_commands()`, `is_cli_exposed`, `UI_ONLY_COMMANDS`, `SHELL_DUPLICATE_COMMANDS`, `to_slash_string`. |
| `src-tauri/src/cli/mod.rs` | Remove `pub mod repl;` / `pub mod query;`; remove `ai`/`search`/`repl`/generated-slash match arms; remove REPL branch in `dispatch` (bare `ol` → `print_help`); add `OPS_COMMANDS`; rewrite `print_help` (table-driven LIFECYCLE, no QUERY); simplify unknown-command path; drop now-invalid tests, add new ones (§6). |
| `src-tauri/src/cli/render.rs` | Remove `render_results`, `render_result_table`, `display_width`, the `use omnilauncher_lib::QueryResult;` import, and the `display_width_counts_wide_glyphs_as_two` test (all orphaned once `query.rs` is gone). Keep `Output` and its color/glyph helpers + their tests. |
| `src-tauri/src/cli/process.rs` | Remove `repl_history_file` (orphaned). Keep all pid/port/health helpers. |
| `src-tauri/src/launcher_config.rs` | **Untouched.** |
| `src/**` (frontend) | **Untouched.** |
| `src-tauri/src/cli/ops.rs` | **Untouched** (no dependency on `query`). |

## 6. Testing

**Remove** (assert behavior that no longer exists), in `cli/mod.rs`:
- `shell_duplicate_command_is_rejected_with_redirect` — `grep` is no longer a special case.
- `ai_without_text_is_usage_error` — `ai` is removed.

**Verify (expected: no change):** `foreground_cli_classification` — bare `ol` is still
foreground (prints help, not REPL), so the existing assertions still hold. No `repl`
subcommand expectation exists in this test today; confirm none needs adding.

**Add**, in `cli/mod.rs`:
- `help_lists_every_ops_command` — rendered help contains every `OPS_COMMANDS[*].name`.
- `query_commands_are_unknown_now` — `dispatch("ol", ["calc","2+2"])`, `["ai","hi"]`,
  `["search","x"]`, `["repl"]`, `["grep","TODO"]` each return `Handled(2)`.
- `bare_ol_prints_help_not_repl` — `dispatch("ol", [])` returns `Handled(0)` (no REPL).

**Keep:** all lifecycle/dispatch/globals/version tests that still apply
(`stop_variants_are_handled`, `serve_subcommand_routes_to_serve`,
`omnilauncher_no_args_is_gui`, `globals_are_extracted_and_stripped`, etc.).

## 7. Verification / rollout

1. `cargo build --release` — must compile with **no** dead-code / unused-import warnings
   (proves the orphan removal in §5 is complete).
2. `cargo test` (backend) — updated suite green.
3. `make install-cli` — refresh the `~/.local/bin/ol` symlink to the freshly built binary.
   (This also resolves the original stale-binary symptom.)
4. Manual smoke: `ol help` shows only lifecycle verbs; `ol` (bare) prints the same help
   and exits; `ol calc 2+2` / `ol ai hi` / `ol repl` each error with exit 2; `ol status`
   still works.

## 8. Risks & mitigations

- **Muscle memory / scripts calling `ol calc`, `ol repl`, etc.** — These now error with a
  clear message and exit 2. Acceptable: the query surface was newly added earlier today and
  has no established dependents; the shell equivalents already exist.
- **Orphaned code missed** — Compiler warnings + the clean-build gate in §7 catch any
  function left unused after the deletions.
- **Accidental GUI coupling** — Ruled out by inspection: the frontend consumes
  `launcher_config.rs`, not the `cli::query` module; `SLASH_COMMANDS` stays intact.

## 9. Estimated size

~615 lines deleted (`repl.rs` + `query.rs` + trimmed render/process/tests), ~40 added
(`OPS_COMMANDS`, rewritten `print_help`, new tests). Net strongly negative.
