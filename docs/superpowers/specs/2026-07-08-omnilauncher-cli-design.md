# OmniLauncher `ol` CLI — Design

**Date:** 2026-07-08
**Status:** Approved (pending final spec review)
**Scope:** Turn OmniLauncher into a self-contained binary with a first-class terminal CLI (`ol`), owning all lifecycle/ops commands and exposing the existing plugin/slash-command surface from the terminal, with polished, consistent output.

---

## 1. Motivation

OmniLauncher today is a Tauri desktop app. A single Rust binary (`omnilauncher`) runs in one of two modes decided by a flat argv scan in `main.rs`:

- no args → desktop GUI shell (`run()`)
- `--server` → HTTP API backend
- `--debug` → enable file logging (orthogonal)

All operational tasks (start/stop/status/logs) live in external scripts (`scripts/ops.sh`, `scripts/ops.ps1`, `scripts/status.*`, `scripts/logs.*`) driven by the `Makefile`. The build (`scripts/ops.sh prepare-binaries`) copies the bare `omnilauncher` into two **byte-for-byte identical** role copies (`omnilauncher-frontend`, `omnilauncher-backend`) and deletes the bare binary.

A large operation surface already exists **as a library**: ~45 plugins and a slash-command catalog (`/run`, `/find`, `/grep`, `/ls`, `/cat`, `/git`, `/calc`, `/todo`, `/web`, `/ps`, `/kill`, `/ip`, `/ports`, `/color`, `/env`, `/sys`, `/clip`, `/skill`, …) routed through `Router::slash_command`. These are reachable only from the GUI or over HTTP — **not from a terminal**.

This project adds a terminal CLI that:

1. Ships as `ol` (a symlink to `omnilauncher`) on the user's `PATH`.
2. Provides interactive operation: a REPL **and** one-shot subcommands.
3. Makes the binary **self-contained** — it owns all lifecycle/ops commands (`start`, `stop`, `restart`, `status`, `logs`, `serve`, `gui`, `doctor`). External scripts shrink to **build-only**.
4. Polishes output across all CLI surfaces into one consistent visual system.

## 2. Goals / Non-goals

**Goals**
- `ol <subcommand>` one-shot execution and `ol` interactive REPL.
- In-process execution of local operations (no backend required, works offline).
- Self-dispatching single binary; `ops.sh`/`Makefile` reduced to building + symlinking.
- Consistent, colored, aligned, TTY-aware output with `--json`, `--no-color`, `-q/--quiet`.
- Preserve every existing launch path: GUI double-click, `--server`, `--debug`, split-machine deployment.

**Non-goals**
- No change to plugin behavior, router logic, AI routing, or settings schema.
- No new HTTP client path for the CLI (local ops run in-process; AI uses the in-process AI client).
- No rework of the frontend/React app.
- Not removing the `--server`/`--debug` flags (kept as back-compat aliases).

## 3. Key decisions (locked during brainstorming)

| Decision | Choice |
|---|---|
| CLI shape | REPL **and** one-shot subcommands |
| Execution model | In-process — call the library directly; no backend needed for local ops |
| `ol` symlink location | `~/.local/bin/ol → <repo>/src-tauri/target/release/omnilauncher` |
| Self-contained ops | Binary owns start/stop/restart/status/logs/serve/gui/doctor; scripts become build-only |
| Binary model | **Single** self-dispatching binary; drop the frontend/backend role-copy split |
| Split-machine support | Preserved: `ol serve` on Linux host + `ol gui` on Windows host (role binaries were always identical) |
| CLI framework | `clap` (subcommands, help, global flags) + `rustyline` (REPL history/editing) |
| Colors | Internal ANSI helper ported from ops.sh palette (no new color crate) |

**Rationale for single binary:** `omnilauncher-frontend` and `omnilauncher-backend` are identical copies of one binary; only the launch invocation differs (`--server` vs none). Collapsing to one binary that self-dispatches loses nothing and makes "self-contained" real. Split-machine deployment is a *runtime mode*, not a separate artifact.

## 4. Architecture

### 4.1 Multi-call dispatch

The entrypoint becomes a clap-based dispatcher. The default no-arg action depends on `argv[0]` basename (busybox-style multi-call):

- Invoked as **`omnilauncher`** with no command → GUI (unchanged; desktop launch is safe).
- Invoked as **`ol`** with no command → REPL (when stdin is a TTY; otherwise prints help).
- Both names accept all subcommands and global flags.

```
omnilauncher [GLOBAL FLAGS] [COMMAND] [ARGS...]

Back-compat (unchanged behavior):
  omnilauncher                  → GUI desktop shell
  omnilauncher --server         → backend API server (alias of `serve`)
  <any> --debug                 → enable file logging (global flag, orthogonal)

Lifecycle / ops:
  serve       Run the backend API server in the foreground (the `--server` body)
  gui         Launch the desktop shell (foreground; --detached to background)
  start       Spawn `self serve` detached, track PID, wait for health
  stop        Stop the detached backend (SIGTERM→SIGKILL, free port)
  restart     stop + start
  status      Rich health/process/port/binary view
  logs        Print/tail the log file (-f follow, -n N)
  doctor      Diagnostics: config, token, AI reachability, optional deps

Query (generated from launcher_config::SLASH_COMMANDS):
  run open app find grep cat ls git calc todo web ip ports ps kill env color sys clip skill
  ai <text>   Route through Router::ai_route (in-process AI client)
  search <text>  Bare launcher search (pm.query_all)

Interactive:
  repl        Enter the interactive prompt (default when invoked as `ol`, TTY)
```

### 4.2 Module layout

`main.rs` is already ~2100 lines. New CLI code lands in a dedicated module tree; `main.rs`'s `fn main()` shrinks to *init logging → `cli::dispatch()`*. The GUI `run()` and `--server` bodies are moved behind `cli::ops::gui()` / `cli::ops::serve()` with their current internals preserved.

```
src-tauri/src/cli/
  mod.rs      clap command definitions, global flags, dispatch entry, argv[0] handling
  ops.rs      start/stop/restart/status/logs/serve/gui/doctor
  process.rs  PID files, detached spawn, port probing (sysinfo), health probe
  query.rs    map query subcommands + REPL input → Router::slash_command / ai_route / search
  repl.rs     rustyline loop, history file, prompt, tab-completion
  render.rs   output formatting: tables, colors, glyphs, success/error, --json
```

### 4.3 Data flow (query command)

```
ol grep "TODO" src/
  └─ cli::dispatch → clap parses subcommand `grep`, args ["TODO","src/"]
     └─ cli::query re-prefixes to "/grep TODO src/"
        └─ spin up a tokio runtime (router is async)
           └─ Router::slash_command("/grep TODO src/", &pm, &mut skill_mgr)
              → AiResponse { content, tools_used, results: Vec<QueryResult>, is_ai }
                 └─ cli::render formats `results` (table) or `content` (text) to stdout
```

The command table is **generated from `launcher_config::SLASH_COMMANDS`** (the same source of truth the GUI uses), so subcommand names, shortcuts (`/r`, `/g`, `/f`, …), and help strings never drift. A new slash command automatically gains a CLI subcommand.

## 5. Component detail

### 5.1 `cli::ops` — self-contained lifecycle

Ports the behavior of `scripts/ops.sh` into Rust. Uses the `sysinfo` crate (already a dependency) for process/port control instead of `pkill`/`lsof`/`ss`, so ops work identically on Linux, macOS, and Windows.

**Paths & state** (moved out of the repo so `ol` works from any directory now that it's on `PATH`):

| Item | Path |
|---|---|
| Backend PID file | `~/.omnilauncher/run/omnilauncher-backend.pid` |
| GUI PID file | `~/.omnilauncher/run/omnilauncher-gui.pid` |
| REPL history | `~/.omnilauncher/repl_history` |
| Log file | `~/.omnilauncher/omnilauncher.log` (unchanged) |
| Token / settings | `~/.config/omnilauncher/…` (unchanged) |

| Command | Behavior |
|---|---|
| `serve` | Run backend in foreground (current `--server` body). `--host`/`--port` flags, else env (`OMNILAUNCHER_SERVER_HOST/PORT`), else `0.0.0.0:1422`. |
| `gui` | Launch desktop shell (current `run()`), foreground; `--detached` backgrounds + writes GUI PID file. |
| `start` | Spawn `self serve` detached (Rust `Command` + `process_group`/`CREATE_NEW_PROCESS_GROUP`), write PID file, poll `/health` until green or ~5s timeout, print result. |
| `stop` | Read PID file → SIGTERM → wait → SIGKILL fallback; then free the port via `sysinfo` if anything is still camped on it. |
| `restart` | `stop` then `start`. |
| `status` | Binary path/version, backend process (PID + mem via `sysinfo`), port LISTENING check, `/health` probe. Ported from `show_status`. |
| `logs` | Print log path; `-f/--follow` tails; `-n N` prints last N lines. Replaces `logs.sh`. |
| `doctor` | Diagnostics, each line OK/WARN/FAIL: settings.json parses; token present; AI provider URL reachable (`/v1/models` probe); optional deps (`scrot` for vision, bundled python) present. |

**Split-machine:** `ol serve` on the Linux backend host + `ol gui` on the Windows GUI host, pointed at each other via backend URL / token (unchanged mechanism from the README). The Windows-only WSL launch shim in `ops.ps1` is not needed.

### 5.2 `cli::query` — query commands & AI

- Query subcommands re-prefix to the slash form and call `Router::slash_command` in-process on a short-lived tokio runtime, reusing `create_plugin_manager` and a `SkillManager`.
- `ol ai "<text>"` (and `ol "? <text>"`) routes through `Router::ai_route` with the in-process `AiClient` built from settings; tokens stream to stdout.
- `ol search <text>` runs bare launcher search via `pm.query_all`.

### 5.3 `cli::repl` — interactive prompt

Entered via `ol` (no args, TTY) or `ol repl`. Grammar:

```
omni> ps                    bare word  → query / slash command
omni> /grep TODO src/       explicit slash also accepted
omni> ? explain lifetimes   AI mode (also `ai …`)
omni> :start   :status      ops commands use a ':' prefix
omni> help                  command list (from SLASH_COMMANDS)
omni> exit  /  Ctrl-D       quit
```

- **rustyline** provides persistent history (`~/.omnilauncher/repl_history`), arrow-key recall, Ctrl-R search, line editing.
- **Tab-completion** on command names from the `SLASH_COMMANDS` table.
- Grammar rule: bare words are always queries; ops are disambiguated with a leading `:` (so `status` as a query term is never confused with the `status` op).

### 5.4 `cli::render` — output system

All commands funnel through one renderer so output reads as one system.

**Global flags**
- `--json` — serialize `QueryResult` / status structs directly; suppresses color/decoration.
- `--no-color` — plain text; auto-enabled when stdout is not a TTY or `NO_COLOR` is set (respects the NO_COLOR standard).
- `-q/--quiet` — errors only; suppresses success chrome and headers.

**Human output examples**
```
ol ps
  PID     CPU%   MEM       COMMAND
  1234    12.3   340 MB    firefox
  5678     4.1    88 MB    code
  3 processes

ol calc "2+2*10"
  = 22

ol grep TODO src/
  src/main.rs:42   // TODO: refactor dispatch
  src/cli/repl.rs:8   // TODO: tab-complete paths
  2 matches in 2 files

ol status
  OmniLauncher  v2.0.0
  backend   ● running   pid 1234   48 MB
  port      ● 1422 listening
  health    ● ok
  gui       ○ stopped

ol start
  ✓ backend started   pid 1234   http://127.0.0.1:1422

ol stop
  ✗ backend not running
```

**Rules**
- **Tables:** computed column widths, right-aligned numerics, dim header row. List-type results render from `QueryResult` rows (title/subtitle → columns). Scalar results (calc, ip, env) print a single `= value` line.
- **Status glyphs:** `●` green = up, `○` dim = down, `●` red = error. Fallback `[OK]`/`[--]`/`[XX]` under `--no-color`.
- **Success/error:** `✓` green / `✗` red prefix (ASCII `+`/`x` under `--no-color`); error text to **stderr**; non-zero exit on failure.
- **Colors:** internal ANSI helper ported from the ops.sh palette (no new color crate). AI streaming prints tokens as they arrive, then a trailing newline.

**Exit codes:** `0` success · `1` command failed (process not found, etc.) · `2` usage error (clap-provided).

## 6. Build & packaging changes

- **`Cargo.toml`:** add `clap` (derive feature) and `rustyline`. Drop nothing.
- **Build output:** `cargo build --release` produces one binary `omnilauncher`. The `prepare-binaries` role-copy/delete step is **removed**; the bare `omnilauncher` is the artifact.
- **`ol` symlink:** a new `make install-cli` target creates `~/.local/bin/ol → <repo>/src-tauri/target/release/omnilauncher` (creating `~/.local/bin` if absent; warns if not on `PATH`). A matching `make uninstall-cli` removes it.
- **`scripts/ops.sh` / `ops.ps1`:** trimmed to build/prepare helpers only, OR the Makefile calls `cargo build` directly and these are removed. Thin shims may remain so `make status` / `make logs` forward to `ol status` / `ol logs`.
- **`status.sh/.ps1`, `logs.sh/.ps1`:** superseded by `ol status` / `ol logs`; removed or shimmed.
- **`make/commands.mk`:** `build` targets simplified (no role flag); `start`/`stop`/`restart`/`status`/`logs` forward to the `ol` binary.

## 7. Compatibility & migration

- GUI double-click / `omnilauncher` no-arg launch: **unchanged**.
- `--server` and `--debug` flags: **still work** (aliases/global flag).
- Split-machine deployment: **unchanged mechanism**, now driven by `ol serve` / `ol gui`.
- PID files move from repo `.run/` to `~/.omnilauncher/run/`; `make stop` on an old running instance is a one-time manual concern (documented).
- Anything external referencing `omnilauncher-frontend` / `omnilauncher-backend` by name must switch to `omnilauncher` (or `ol`). These were identical copies; behavior is unchanged.

## 8. Testing strategy

- **Rust unit tests** (`cargo test --lib`):
  - `cli::mod` — argv[0] dispatch (`ol` → REPL default, `omnilauncher` → GUI default), subcommand parsing, back-compat `--server`/`--debug`.
  - `cli::query` — subcommand → slash-string mapping for the full `SLASH_COMMANDS` catalog (table-driven; guards against drift).
  - `cli::render` — table alignment, `--no-color`/`NO_COLOR` fallback glyphs, `--json` shape, exit-code mapping.
  - `cli::process` — PID-file round-trip; port-probe against a bound test socket.
- **Behavioral checks** (extend `scripts/smoke-endpoints.sh` or a new `ol`-focused smoke test):
  - `ol start` → `ol status` shows running + `/health` ok → `ol stop` → status shows stopped.
  - `ol calc "2+2"` prints `= 4`; `ol --json ps` emits valid JSON; `ol grep` in a temp tree finds a known match.
- **Manual/verify:** `ol` REPL round-trip (history persists, tab-complete, `:status`, `exit`); split-machine `serve`+`gui` smoke on two hosts if available.
- Keep `make test` green (`cargo test --lib` + `npm test`).

## 9. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Detached spawn / process-group semantics differ across OS | Centralize in `cli::process`; unit-test PID round-trip; use `sysinfo` for cross-platform kill/port. |
| Moving PID files orphans a running old instance | Document one-time `make stop` before upgrading; `ol stop` also frees the port by scan. |
| Binary size / cold-start from clap+rustyline | Both are lightweight and lazy; GUI/serve paths don't touch rustyline. Measure with `scripts/bench-cold-start.sh`. |
| GUI accidentally launched headless in CI via `ol` default | Default REPL only when stdin is a TTY; otherwise print help and exit 0. |
| Removing role binaries breaks external tooling | Documented migration; names were identical copies, so `omnilauncher` is a drop-in. |

## 10. Implementation order (for the plan)

1. Scaffold `cli/` module + `clap`/`rustyline` deps; wire `main()` → `cli::dispatch()` with GUI/serve/debug back-compat (no behavior change yet).
2. `cli::process` + `cli::ops` (serve/gui/start/stop/restart/status/logs/doctor) with `~/.omnilauncher/run/` state.
3. `cli::render` (tables, colors, glyphs, flags, JSON, exit codes).
4. `cli::query` (slash mapping from `SLASH_COMMANDS`, `ai`, `search`).
5. `cli::repl` (rustyline loop, history, completion, `:` ops grammar).
6. Build/Makefile: drop role split, add `install-cli`/`uninstall-cli`, forward `make` ops to `ol`, trim scripts.
7. Tests + docs (README CLI section).
