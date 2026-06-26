# Role-named binaries, DEBUG/VERBOSE flag, targeted logging

Date: 2026-06-06
Status: Draft (awaiting user review)

## Summary

Three small, related changes to the build / run / observability story:

1. **Role-named binaries.** Stop shipping the generic `omnilauncher` file as a third copy. Builds produce only `omnilauncher-frontend` and `omnilauncher-backend`.
2. **`DEBUG=1` / `VERBOSE=1` on start and restart.** Adds a Make-level toggle that passes `--debug` through to the launched binary. The two variable names are aliases so users can type whichever feels natural; under the hood they map to the same existing `--debug` CLI flag.
3. **Targeted logs on hot paths.** Add startup banners, HTTP request/response logs in the split backend, AI-client boundary logs, and a thin frontend logger wrapping `console.*` with a level gate.

## Motivation

- The current build leaves three identical binaries on disk (`omnilauncher`, `omnilauncher-frontend`, `omnilauncher-backend`). Users see the bare `omnilauncher` and wonder which one to run; `status` and stop commands have to special-case it.
- Today the only way to start a release binary with debug logging is `make prod-debug-*`, which is its own target tree and easy to forget. Adding `DEBUG=1` to the same start/restart targets people already use removes the cognitive overhead.
- When something misbehaves in production-style runs, the backend silently handles a request and we have no per-request trace. The frontend uses bare `console.log` with no levels, so debug output drowns warnings.

## Non-goals

- No Cargo workspace split. We keep one crate, one `[[bin]]`.
- No new log rotation, no log-level config UI.
- No global sweep replacing every `console.log` — only HTTP and startup paths get the new logger.
- The existing `prod-debug-*` Make targets stay (deprecation is a future call).

---

## Design

### 1. Role-named binaries

**Current behavior** (`scripts/ops.sh` `prepare_binaries`):

```
cargo build --release  →  target/release/omnilauncher
prepare-binaries        →  cp omnilauncher omnilauncher-frontend
                           cp omnilauncher omnilauncher-backend
                           # all three files remain on disk
```

**New behavior:**

`prepare_binaries` becomes role-aware and takes an argument:

```
prepare-binaries frontend  →  cp omnilauncher omnilauncher-frontend
                              rm omnilauncher
prepare-binaries backend   →  cp omnilauncher omnilauncher-backend
                              rm omnilauncher
prepare-binaries both      →  cp omnilauncher omnilauncher-frontend
                              cp omnilauncher omnilauncher-backend
                              rm omnilauncher
```

Rules:

- The bare `omnilauncher` source file is removed after copying.
- `prepare-binaries <role>` is idempotent: if the source `omnilauncher` is missing but the target role-file exists, it's a no-op success. Only an error if neither exists.
- `remove-binary` continues to remove all three names if present (safe cleanup).
- `ensure_role_binaries` still calls `prepare_binaries both` as today's fallback when both role files are missing.

**Makefile call-sites updated** to pass the role:

- `build-frontend` → `$(OPS) prepare-binaries frontend`
- `build-backend` → `$(OPS) prepare-binaries backend`
- `maybe-rebuild-frontend` → `$(OPS) prepare-binaries frontend`
- `maybe-rebuild-backend` → `$(OPS) prepare-binaries backend`
- `restart-frontend` body → `$(OPS) prepare-binaries frontend`
- `restart-backend` body → `$(OPS) prepare-binaries backend`
- `restart` body (still does both) → `$(OPS) prepare-binaries both`

Both `scripts/ops.sh` and `scripts/ops.ps1` are updated symmetrically.

### 2. `DEBUG=1` / `VERBOSE=1` toggle

**Make variable.** Add a Make-level computed string:

```make
DEBUG   ?= 0
VERBOSE ?= 0

# Either knob switches the binary into --debug mode.
ifeq ($(DEBUG),1)
  DEBUG_FLAG := --debug
else ifeq ($(VERBOSE),1)
  DEBUG_FLAG := --debug
else
  DEBUG_FLAG :=
endif
```

`DEBUG` and `VERBOSE` are aliases. Both map to the existing `--debug` CLI flag of the `omnilauncher` binary, which already enables trace-level file logging at `~/.omnilauncher/omnilauncher.log` (see `init_debug_logging` in `src-tauri/src/main.rs`). We do not introduce a second log level today.

**Passthrough flag.** `scripts/ops.sh` and `ops.ps1` learn one new accepted flag on the relevant start/debug actions:

- `start-frontend`, `start-backend`, `prod-debug-frontend`, `prod-debug-backend` accept `--debug` (boolean). When present, the launched binary is invoked with `--debug`.
- The flag is forwarded to `start_detached` as a trailing arg in `$@`.
- Backend keeps `--split-backend` as its first arg; `--debug` is appended after.

**Makefile target bodies.** Each affected target becomes:

```make
start-backend: stop-backend maybe-rebuild-backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" $(DEBUG_FLAG)
```

Targets touched:

- `start-frontend`, `start-backend`, `start`
- `restart-frontend`, `restart-backend`, `restart`

`start` and `restart` are composites; the variable flows down to the per-side targets automatically because Make exports recursive variables to recipe lines.

**Usage:**

```bash
make start-backend  DEBUG=1
make restart        VERBOSE=1
make restart-frontend DEBUG=1 REBUILD=1
```

**`make help` text.** Add to the `[Start]` and `[Restart]` blocks:

```
  DEBUG=1 or VERBOSE=1   run binary with --debug (trace logging to file)
```

### 3. Targeted logging additions

#### Backend (`log` crate, already wired)

**Startup banner.** In `src-tauri/src/main.rs`:

- After CLI arg parsing, before either `run()` or `split_server::spawn_split_server`, emit one `log::info!` banner line:
  ```
  OmniLauncher starting role={tauri|split-backend} version={CARGO_PKG_VERSION} debug={true|false} log_file={path or "stderr"}
  ```
- The existing `log::info!("split backend listening on http://{host}:{port}")` at the top of `spawn_split_server` already covers the bind-success case; no new log there.

**Per-request logs in `src-tauri/src/split_server.rs`.**

The split server is a hand-rolled TCP loop in `spawn_split_server` (around lines 222-287), not an Axum/Hyper stack — there's no middleware seam. Today it already emits `log::debug!` lines on request and response (around lines 249-256 and 276-283). Changes:

- Capture `let started_at = std::time::Instant::now();` right after parsing method/path.
- Promote the existing request line from `debug` to `info`, prefix with `→`:
  ```
  log::info!("→ {method} {path} from={addr} bytes={read_len}")
  ```
- Promote the response line from `debug` to `info`, prefix with `←`, add `elapsed_ms`:
  ```
  log::info!("← {method} {path} status={status} body_bytes={n} elapsed_ms={ms}")
  ```
- The early-return SSE branch (`event_name_from_path` match) keeps its existing `debug!` subscribe line and gets a new `info!` exit line with `elapsed_ms` before the `stream.shutdown()`.
- The early read-error / zero-byte returns stay at `debug` — they're noise from probes.

**AI client boundaries** in `src-tauri/src/ai/client.rs`:

These already exist at `debug` (`AI request →` at ~line 244, `AI response ←` at ~line 299) with `elapsed_ms`. Change: **promote both lines from `log::debug!` to `log::info!`** so they're visible at the default info threshold (e.g. when the user runs `make start-backend` without `DEBUG=1`, errors will still log; with `DEBUG=1` everything else also goes to the file).

**Live server** in `src-tauri/src/live_server.rs`:

The bind / start log at ~line 102 (`live server listening on http://127.0.0.1:{port}`) is already `log::info!`. No new logs needed here today.

**Frontend logger** — already partially exists.

`src/lib/runtime.ts` already has a `frontendLog(level, message)` and HTTP boundary logs at debug. Changes:

- **Extract** `frontendLog`, `FrontendLogLevel`, and `summarizeArgs` from `runtime.ts` into a new `src/lib/logger.ts` so they're reusable from `App.tsx` and elsewhere without a circular import.
- **Add a level gate**: `logger` checks `import.meta.env.DEV ? 'debug' : 'info'`, overridden by `?log=<level>` URL query or `localStorage.OMNI_LOG_LEVEL`. Below-threshold calls are no-ops (no `console.log`, no `tauriInvoke`). The `frontend_log` Tauri command continues to receive only messages at or above the active level — keeps the backend log quiet in production.
- **`runtime.ts`** imports `logger` from `./logger` and replaces local `frontendLog` calls with `logger.<level>(...)`. No behavior change beyond gating.
- **`src/App.tsx`** emits one `logger.info` line on initial mount: `"OmniLauncher UI mounted backend=<url> mode=<tauri|http|mock> dev=<bool>"`.
- **No changes** to `src/tauri-api.ts`. It's a dev-mode mock shim; the real path goes through `runtime.ts`.

### Out of scope

- Cargo workspace / split crates.
- Log rotation, log-level config UI, structured (JSON) logs.
- Replacing existing `console.log` calls outside `tauri-api.ts` and `App.tsx`.
- Deprecating or removing `prod-debug-*` Make targets.
- Adding `--verbose` as a distinct second log level on the Rust side. `VERBOSE=1` is a Make-level alias for `DEBUG=1` only.

---

## File-by-file change list

| File | Change |
|---|---|
| `Makefile` | Add `DEBUG`/`VERBOSE` variables and `DEBUG_FLAG`; append `$(DEBUG_FLAG)` to 6 start/restart `$(OPS)` calls; update `prepare-binaries` calls to pass role; update `help` text. |
| `scripts/ops.sh` | `prepare_binaries` takes a role arg (`frontend`/`backend`/`both`); deletes bare `omnilauncher` after copy; `start-frontend`/`start-backend`/`prod-debug-*` accept and forward `--debug`. |
| `scripts/ops.ps1` | Same changes as `ops.sh`, in PowerShell. |
| `src-tauri/src/main.rs` | Startup banner `log::info!` in both arms; bind-address log in `--split-backend` arm. |
| `src-tauri/src/split_server.rs` | Promote existing per-request debug logs to info; add `elapsed_ms`; add SSE exit log. |
| `src-tauri/src/ai/client.rs` | Promote existing `AI request →` and `AI response ←` lines from `debug` to `info`. |
| `src-tauri/src/live_server.rs` | No change. Start log already at info. |
| `src/lib/logger.ts` | New. Extracted from `runtime.ts`; adds a runtime level gate. |
| `src/lib/runtime.ts` | Import `logger`; replace local `frontendLog` calls. |
| `src/App.tsx` | Emit startup banner via `logger.info` on initial mount. |

Approximate diff size: ~50 lines of new infrastructure (`logger.ts`), ~40 lines of log call-sites, ~30 lines of Makefile / ops script edits.

## Testing

- `make build-frontend && ls src-tauri/target/release/omnilauncher*` — only `omnilauncher-frontend` should exist (no bare `omnilauncher`).
- `make build-backend && ls src-tauri/target/release/omnilauncher*` — `omnilauncher-frontend` and `omnilauncher-backend` both present, no bare `omnilauncher`.
- `make restart-backend DEBUG=1` then `tail ~/.omnilauncher/omnilauncher.log` — should see startup banner and per-request lines.
- `make restart-backend VERBOSE=1` — same behavior as `DEBUG=1`.
- `make restart-backend` (no flag) — log file not created or not appended to (existing behavior).
- `curl http://127.0.0.1:1422/health` with backend in debug mode — log file shows `→ GET /health` and `← GET /health status=200 elapsed_ms=…`.
- Open the frontend, watch devtools console — banner line on mount; every backend call logged at info; debug entries only when `?log=debug` set.
- `cargo test` and `npm test` still green (no behavior changes outside new log calls).

## Risks

- **Per-request logs at info.** On a busy backend this could become chatty. Mitigation: keep entry/exit lines short and one-line; if it becomes an issue, drop the entry line to `debug` later.
- **Removing bare `omnilauncher`.** Anything outside the Makefile that referenced the bare name will break. We will grep the repo during planning and update or call it out.
- **Two-knob aliasing.** `DEBUG=1 VERBOSE=0` is unambiguous (debug wins). `DEBUG=0 VERBOSE=1` is also unambiguous (verbose triggers `--debug`). No conflicting state is possible.
