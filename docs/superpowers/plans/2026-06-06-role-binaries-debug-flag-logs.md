# Role-Named Binaries, DEBUG/VERBOSE Flag, Targeted Logs — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the desktop launcher's release artifacts as `omnilauncher-frontend` and `omnilauncher-backend` only (no leftover `omnilauncher` file), expose `DEBUG=1` / `VERBOSE=1` Make-level toggles on every `start*` and `restart*` target, and add a small set of high-value logs on the hot paths in both backend and frontend.

**Architecture:** Single Cargo crate continues to build one `omnilauncher` binary; the ops shell scripts (`scripts/ops.sh`, `scripts/ops.ps1`) become role-aware — they copy the artifact to the role-named file and then delete the bare source. The Makefile gains `DEBUG`/`VERBOSE` aliases that resolve to a single `DEBUG_FLAG := --debug` string appended to start/restart `$(OPS)` invocations. The new flag is plumbed through `ops.sh`/`ops.ps1` as a passthrough. Backend log changes are surgical: promote two existing `debug!` lines to `info!` in `split_server.rs`, two in `ai/client.rs`, add one startup banner in `main.rs`. Frontend logger work is an extract: pull the existing `frontendLog` helper out of `runtime.ts` into a new `src/lib/logger.ts`, gate it by level, then emit one banner from `App.tsx` on mount.

**Tech Stack:** Rust (`log` + `simplelog`), TypeScript/React, GNU Make, bash, PowerShell. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-06-06-role-binaries-debug-flag-logs-design.md`

---

## File Structure

**New files:**
- `src/lib/logger.ts` — level-gated frontend logger (extracted + extended from `runtime.ts`)

**Modified files:**
- `Makefile` — `DEBUG`/`VERBOSE` vars, role-aware `prepare-binaries` calls, `$(DEBUG_FLAG)` appended to start/restart targets, help text
- `scripts/ops.sh` — role-aware `prepare_binaries`, `--debug` passthrough on start actions
- `scripts/ops.ps1` — same as `ops.sh`, PowerShell equivalent
- `src-tauri/src/main.rs` — startup banner before each runtime arm
- `src-tauri/src/split_server.rs` — promote per-request `debug!` → `info!` + add `elapsed_ms`
- `src-tauri/src/ai/client.rs` — promote `AI request →` and `AI response ←` `debug!` → `info!`
- `src/lib/runtime.ts` — import `logger` from `./logger`, replace inline `frontendLog` calls
- `src/App.tsx` — emit `logger.info` banner on initial mount

**Unchanged but consulted:** `src-tauri/Cargo.toml`, `src/tauri-api.ts`, `src-tauri/src/live_server.rs`

---

## Task 1: Role-aware `prepare_binaries` in `scripts/ops.sh`

**Files:**
- Modify: `scripts/ops.sh:63-81` (the `prepare_binaries` and `ensure_role_binaries` functions)
- Modify: `scripts/ops.sh:322-347` (dispatch case in `case "$ACTION" in`)

- [ ] **Step 1: Read the current `prepare_binaries` to confirm exact line range**

Run: `sed -n '60,90p' scripts/ops.sh`
Expected: shows current `prepare_binaries`, `ensure_role_binaries`, `remove_binaries`.

- [ ] **Step 2: Rewrite `prepare_binaries` to accept a role**

Replace the existing `prepare_binaries`, `ensure_role_binaries`, and `remove_binaries` block (`scripts/ops.sh:63-85`) with:

```bash
# Copy the freshly-built omnilauncher binary into role-named file(s) and
# remove the generic source so we don't ship three identical files.
#   prepare_binaries frontend  -> only omnilauncher-frontend remains
#   prepare_binaries backend   -> only omnilauncher-backend remains
#   prepare_binaries both      -> both role files exist; bare omnilauncher gone
# Idempotent: if the bare omnilauncher is missing but the requested role
# file already exists, succeed silently.
prepare_binaries() {
    local role="${1:-both}"
    case "$role" in
        frontend|backend|both) ;;
        *)
            err "prepare_binaries: unknown role '$role' (expected frontend|backend|both)"
            return 2
            ;;
    esac

    if [ ! -f "$BASE_EXE" ]; then
        local have_fe=0 have_be=0
        [ -f "$FRONTEND_EXE" ] && have_fe=1
        [ -f "$BACKEND_EXE" ] && have_be=1
        case "$role" in
            frontend) [ "$have_fe" = "1" ] && return 0 ;;
            backend)  [ "$have_be" = "1" ] && return 0 ;;
            both)     [ "$have_fe" = "1" ] && [ "$have_be" = "1" ] && return 0 ;;
        esac
        err "Release binary not found at $BASE_EXE"
        err "Run: make build-frontend or make build-backend"
        exit 1
    fi

    case "$role" in
        frontend|both) cp -f "$BASE_EXE" "$FRONTEND_EXE" ;;
    esac
    case "$role" in
        backend|both)  cp -f "$BASE_EXE" "$BACKEND_EXE" ;;
    esac
    rm -f "$BASE_EXE"

    ok "Prepared role binaries (role=$role):"
    [ -f "$FRONTEND_EXE" ] && echo "  frontend: $FRONTEND_EXE"
    [ -f "$BACKEND_EXE" ]  && echo "  backend:  $BACKEND_EXE"
}

ensure_role_binaries() {
    if [ -f "$FRONTEND_EXE" ] && [ -f "$BACKEND_EXE" ]; then
        return 0
    fi
    prepare_binaries both
}

remove_binaries() {
    rm -f "$BASE_EXE" "$FRONTEND_EXE" "$BACKEND_EXE" 2>/dev/null || true
}
```

- [ ] **Step 3: Update the dispatch in the `case "$ACTION" in` block**

Find the line in `scripts/ops.sh` that reads:

```bash
    prepare-binaries)     prepare_binaries ;;
```

Replace with:

```bash
    prepare-binaries)     prepare_binaries "${1:-both}" ;;
```

The role is passed as the first remaining positional after `--Split*`/`--BackendUrl` are consumed. Confirm this works given the existing arg parsing (the `while [ $# -gt 0 ]` loop already `shift`s unknown flags out, leaving role positionals untouched).

- [ ] **Step 4: Verify the script is still valid bash**

Run: `bash -n scripts/ops.sh`
Expected: no output (syntax OK).

- [ ] **Step 5: Verify role-aware prepare works end-to-end**

Run: `cd src-tauri && cargo build --release 2>&1 | tail -3`
Expected: build succeeds, produces `target/release/omnilauncher`.

Run: `bash scripts/ops.sh prepare-binaries frontend`
Expected: prints `Prepared role binaries (role=frontend):` and only `frontend: …/omnilauncher-frontend` line.

Run: `ls -1 src-tauri/target/release/omnilauncher*`
Expected: lists `omnilauncher-frontend` only. The bare `omnilauncher` file is gone. (If `omnilauncher-backend` exists from a prior run, it stays — `prepare_binaries frontend` does not delete other roles.)

- [ ] **Step 6: Commit**

```bash
git add scripts/ops.sh
git commit -m "feat(ops): role-aware prepare_binaries in ops.sh (frontend|backend|both)"
```

---

## Task 2: Role-aware `Prepare-Binaries` in `scripts/ops.ps1`

**Files:**
- Modify: `scripts/ops.ps1:8-19` (extend `[ValidateSet]` to include new arg later — but actually no validation change here; role is passed via extra positional)
- Modify: `scripts/ops.ps1:32-55` (the three binary-management functions)
- Modify: `scripts/ops.ps1:199-217` (dispatch switch)

- [ ] **Step 1: Read current state**

Run: `sed -n '30,55p' scripts/ops.ps1`
Expected: shows the current `Prepare-Binaries`, `Ensure-RoleBinaries`, `Remove-Binaries` functions.

- [ ] **Step 2: Add a `[string]$Role = 'both'` parameter to the param block**

Find the `param(` block at `scripts/ops.ps1:8-24`. After the `[string]$BackendUrl = 'http://127.0.0.1:1422'` line, add a comma to its end and insert before the closing `)`:

```powershell
    [string]$Role         = 'both'
```

The complete param block becomes:

```powershell
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        'stop-frontend', 'stop-backend', 'stop-all',
        'start-frontend', 'start-backend',
        'start-wsl-backend', 'restart-wsl-backend',
        'prod-debug-backend', 'prod-debug-frontend', 'prod-debug',
        'test-backend', 'remove-binary', 'prepare-binaries',
        'clean-frontend', 'clean-backend', 'clean',
        'status'
    )]
    [string]$Action,

    [string]$SplitHost   = '0.0.0.0',
    [string]$SplitPort    = '1422',
    [string]$BackendUrl   = 'http://127.0.0.1:1422',
    [string]$Role         = 'both'
)
```

- [ ] **Step 3: Rewrite `Prepare-Binaries`**

Replace `scripts/ops.ps1:32-42` (the existing `function Prepare-Binaries { ... }`) with:

```powershell
# Copy the freshly-built omnilauncher binary into role-named file(s) and
# remove the generic source so we don't ship three identical files.
#   -Role frontend  -> only omnilauncher-frontend remains
#   -Role backend   -> only omnilauncher-backend remains
#   -Role both      -> both role files exist; bare omnilauncher gone
function Prepare-Binaries {
    param([string]$Which = 'both')
    if ($Which -notin @('frontend','backend','both')) {
        Write-Host "Prepare-Binaries: unknown role '$Which'" -ForegroundColor Red
        exit 2
    }

    if (-not (Test-Path $baseExe)) {
        $haveFe = Test-Path $frontendExe
        $haveBe = Test-Path $backendExe
        switch ($Which) {
            'frontend' { if ($haveFe) { return } }
            'backend'  { if ($haveBe) { return } }
            'both'     { if ($haveFe -and $haveBe) { return } }
        }
        Write-Host 'Release binary not found. Run: make build-frontend or make build-backend' -ForegroundColor Red
        exit 1
    }

    if ($Which -in @('frontend','both')) { Copy-Item -Force $baseExe $frontendExe }
    if ($Which -in @('backend','both'))  { Copy-Item -Force $baseExe $backendExe }
    Remove-Item -Force $baseExe -ErrorAction SilentlyContinue

    Write-Host "Prepared role binaries (role=$Which):" -ForegroundColor Green
    if (Test-Path $frontendExe) { Write-Host "  frontend: $frontendExe" }
    if (Test-Path $backendExe)  { Write-Host "  backend:  $backendExe" }
}
```

- [ ] **Step 4: Update `Ensure-RoleBinaries` to call `Prepare-Binaries -Which both`**

Replace the existing `Ensure-RoleBinaries` body (`scripts/ops.ps1:44-49`):

```powershell
function Ensure-RoleBinaries {
    if ((Test-Path $frontendExe) -and (Test-Path $backendExe)) {
        return
    }
    Prepare-Binaries -Which both
}
```

(`Remove-Binaries` at 51-55 is unchanged — it already wipes all three names.)

- [ ] **Step 5: Update the dispatch switch**

Find the line in the `switch ($Action) { … }` block:

```powershell
    'prepare-binaries'     { Prepare-Binaries }
```

Replace with:

```powershell
    'prepare-binaries'     { Prepare-Binaries -Which $Role }
```

- [ ] **Step 6: Validate the script parses**

Run (on a machine with PowerShell available — skip if unix-only env, validation will happen via Makefile call later):

```powershell
pwsh -NoProfile -Command "Get-Command -Syntax (Resolve-Path scripts/ops.ps1)"
```

If `pwsh` unavailable on this dev machine, skip — Step 5 of Task 3 below will exercise it.

- [ ] **Step 7: Commit**

```bash
git add scripts/ops.ps1
git commit -m "feat(ops): role-aware Prepare-Binaries in ops.ps1 mirroring ops.sh"
```

---

## Task 3: Plumb the role argument through the Makefile

**Files:**
- Modify: `Makefile:122-131` (build targets)
- Modify: `Makefile:148-149` (`prepare-binaries` passthrough target)
- Modify: `Makefile:155-168` (`maybe-rebuild-*` helpers)
- Modify: `Makefile:194-216` (restart bodies)

- [ ] **Step 1: Update `build-frontend` and `build-backend`**

Find at `Makefile:122-131`:

```make
build-frontend:
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries

build-backend:
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries

build: build-frontend build-backend
```

Replace with:

```make
build-frontend:
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries frontend

build-backend:
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries backend

build: build-frontend build-backend
```

For the bash `$(OPS)` (`bash scripts/ops.sh`) the trailing `frontend` is a positional that `ops.sh`'s arg loop ignores during flag parsing and the dispatch picks up as `"${1:-both}"`.

For the PowerShell `$(OPS)` (`powershell -NoProfile -File scripts/ops.ps1`), trailing positional args are not bound to a named parameter. **Fix:** change PowerShell calls to use `-Role`. Since the same Makefile line runs on both platforms, use a Make-level switch.

Add near the existing platform block (right after `else ... OPS = bash scripts/ops.sh ...` in `Makefile:51-55`) a new variable:

```make
ifeq ($(PLATFORM),windows)
  OPS_ROLE_FLAG = -Role
else
  OPS_ROLE_FLAG =
endif
```

And rewrite the build targets to:

```make
build-frontend:
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) frontend

build-backend:
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) backend

build: build-frontend build-backend
```

On Linux this expands to `bash scripts/ops.sh prepare-binaries frontend`; on Windows to `powershell ... scripts/ops.ps1 prepare-binaries -Role frontend`.

- [ ] **Step 2: Update `maybe-rebuild-*` helpers**

Find at `Makefile:155-168`:

```make
maybe-rebuild-backend:
ifeq ($(REBUILD),1)
	$(OPS) remove-binary
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries
endif

maybe-rebuild-frontend:
ifeq ($(REBUILD),1)
	$(OPS) remove-binary
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries
endif
```

Replace the two `$(OPS) prepare-binaries` lines respectively with:

```make
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) backend
```

```make
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) frontend
```

- [ ] **Step 3: Update `restart-frontend`, `restart-backend`, and `restart`**

Find at `Makefile:194-216`:

```make
restart-frontend:
	$(OPS) stop-frontend
	$(OPS) remove-binary
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)"

restart-backend:
	$(OPS) stop-backend
	$(OPS) remove-binary
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"

restart:
	$(OPS) stop-all
	$(OPS) remove-binary
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)"
```

Replace the three `$(OPS) prepare-binaries` lines, in order, with:

```make
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) frontend
```

```make
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) backend
```

```make
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) both
```

(The `restart` umbrella target rebuilds both UI and backend → both role binaries.)

- [ ] **Step 4: Update the explicit `prepare-binaries` Makefile target**

Find at `Makefile:148-149`:

```make
prepare-binaries:
	$(OPS) prepare-binaries
```

Replace with:

```make
prepare-binaries:
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) both
```

- [ ] **Step 5: Smoke-test the Makefile end-to-end**

Run: `make build-backend 2>&1 | tail -5`
Expected: cargo build succeeds, then `Prepared role binaries (role=backend):` line; only `omnilauncher-backend` printed.

Run: `ls -1 src-tauri/target/release/omnilauncher* | sort`
Expected:
- `…/omnilauncher-backend` only (no bare `omnilauncher`).
- If a stale `omnilauncher-frontend` from a previous run exists, it remains — that's correct, only `backend` was rebuilt.

Run: `bash scripts/ops.sh remove-binary && make build 2>&1 | tail -5`
Expected: both `omnilauncher-frontend` and `omnilauncher-backend` exist, no bare `omnilauncher`.

- [ ] **Step 6: Commit**

```bash
git add Makefile
git commit -m "feat(make): pass role to prepare-binaries; PowerShell uses -Role"
```

---

## Task 4: `DEBUG=1` / `VERBOSE=1` flag in the Makefile

**Files:**
- Modify: `Makefile:31-32` (where `REBUILD ?= 0` lives — add adjacent variables)
- Modify: `Makefile:172-180` (`start-frontend`, `start-backend`, `start`)
- Modify: `Makefile:194-216` (`restart-frontend`, `restart-backend`, `restart`)
- Modify: `Makefile:59-118` (help text)

- [ ] **Step 1: Add `DEBUG`, `VERBOSE`, and `DEBUG_FLAG` variables**

After the `REBUILD ?= 0` block at `Makefile:31`, add:

```make
# Set DEBUG=1 or VERBOSE=1 to start the binary with --debug.
# Both names are aliases for the same underlying --debug CLI flag, which
# enables trace-level file logging at ~/.omnilauncher/omnilauncher.log.
#   make restart-backend DEBUG=1
#   make start           VERBOSE=1
DEBUG   ?= 0
VERBOSE ?= 0

ifeq ($(DEBUG),1)
  DEBUG_FLAG := --debug
else ifeq ($(VERBOSE),1)
  DEBUG_FLAG := --debug
else
  DEBUG_FLAG :=
endif
```

- [ ] **Step 2: Append `$(DEBUG_FLAG)` to start targets**

Find at `Makefile:172-180`:

```make
start-frontend: stop-frontend maybe-rebuild-frontend
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)"

start-backend: stop-backend maybe-rebuild-backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"

start: start-backend start-frontend
```

Replace with:

```make
start-frontend: stop-frontend maybe-rebuild-frontend
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)

start-backend: stop-backend maybe-rebuild-backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" $(DEBUG_FLAG)

start: start-backend start-frontend
```

`start` composes the two; `DEBUG`/`VERBOSE` automatically flow because Make exports recursive variables to recipe lines.

- [ ] **Step 3: Append `$(DEBUG_FLAG)` to restart targets**

In the restart bodies modified in Task 3 step 3, append `$(DEBUG_FLAG)` to the `start-*` lines so they become:

```make
restart-frontend:
	$(OPS) stop-frontend
	$(OPS) remove-binary
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) frontend
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)

restart-backend:
	$(OPS) stop-backend
	$(OPS) remove-binary
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" $(DEBUG_FLAG)

restart:
	$(OPS) stop-all
	$(OPS) remove-binary
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) both
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" $(DEBUG_FLAG)
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)
```

- [ ] **Step 4: Update help text**

In the `help:` target (`Makefile:59-118`), find the `[Start]` and `[Restart]` sections. After the existing `$(info [Start] use REBUILD=1 to rebuild first)` line, change the header to:

```make
	$(info [Start] use REBUILD=1 to rebuild first; DEBUG=1 or VERBOSE=1 for --debug logging)
```

And the restart header from `$(info [Restart] stop + remove binary + rebuild + start)` to:

```make
	$(info [Restart] stop + remove binary + rebuild + start; DEBUG=1 or VERBOSE=1 for --debug logging)
```

Also under the `Variables:` block (around `Makefile:89-94`) add:

```make
	$(info     DEBUG=1 or VERBOSE=1           start binary with --debug)
```

immediately after `$(info     REBUILD=1                      rebuild before start)`.

- [ ] **Step 5: Verify Make syntax**

Run: `make -n start-backend DEBUG=1 2>&1 | tail -3`
Expected (no actual execution due to `-n`):
- final line ends with `... --SplitPort "1422" --debug`

Run: `make -n start-backend 2>&1 | tail -3`
Expected: final line ends with `... --SplitPort "1422"` (no trailing flag, no double space matters).

Run: `make -n restart VERBOSE=1 2>&1 | grep start-`
Expected: both `start-backend` and `start-frontend` `$(OPS)` calls end with `--debug`.

- [ ] **Step 6: Commit**

```bash
git add Makefile
git commit -m "feat(make): add DEBUG=1 / VERBOSE=1 aliases on start and restart"
```

---

## Task 5: `--debug` passthrough in `scripts/ops.sh`

**Files:**
- Modify: `scripts/ops.sh:36-43` (arg parser)
- Modify: `scripts/ops.sh:166-194` (`start_frontend`, `start_backend`, `start_prod_debug_*`)

- [ ] **Step 1: Add `--debug` to the arg parser**

Find at `scripts/ops.sh:36-43`:

```bash
while [ $# -gt 0 ]; do
    case "$1" in
        --SplitHost)   SPLIT_HOST="${2:-}"; shift 2 ;;
        --SplitPort)   SPLIT_PORT="${2:-}"; shift 2 ;;
        --BackendUrl)  BACKEND_URL="${2:-}"; shift 2 ;;
        *) shift ;;  # ignore unknown
    esac
done
```

Add a `DEBUG_FLAG=""` initializer near the existing defaults (around line 32-34) and extend the parser:

```bash
SPLIT_HOST="0.0.0.0"
SPLIT_PORT="1422"
BACKEND_URL="http://127.0.0.1:1422"
DEBUG_FLAG=""
POSITIONALS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --SplitHost)   SPLIT_HOST="${2:-}"; shift 2 ;;
        --SplitPort)   SPLIT_PORT="${2:-}"; shift 2 ;;
        --BackendUrl)  BACKEND_URL="${2:-}"; shift 2 ;;
        --debug)       DEBUG_FLAG="--debug"; shift ;;
        --*)           shift ;;  # ignore other unknown flags
        *)             POSITIONALS+=("$1"); shift ;;
    esac
done
set -- "${POSITIONALS[@]}"
```

Now the dispatch sees positionals only via `"$@"`. The `prepare-binaries) prepare_binaries "${1:-both}" ;;` line from Task 1 still works because `set --` rebuilds `$@` from `POSITIONALS`.

- [ ] **Step 2: Forward `--debug` from `start_frontend` / `start_backend`**

Replace `start_frontend` (`scripts/ops.sh:166-171`):

```bash
start_frontend() {
    ensure_role_binaries
    export OMNILAUNCHER_BACKEND_URL="$BACKEND_URL"
    cd "$REPO_DIR"
    if [ -n "$DEBUG_FLAG" ]; then
        start_detached "$FRONTEND_EXE" "$FRONTEND_PID_FILE" "$DEBUG_FLAG"
    else
        start_detached "$FRONTEND_EXE" "$FRONTEND_PID_FILE"
    fi
}
```

Replace `start_backend` (`scripts/ops.sh:173-179`):

```bash
start_backend() {
    ensure_role_binaries
    export OMNILAUNCHER_SPLIT_HOST="$SPLIT_HOST"
    export OMNILAUNCHER_SPLIT_PORT="$SPLIT_PORT"
    cd "$REPO_DIR"
    if [ -n "$DEBUG_FLAG" ]; then
        start_detached "$BACKEND_EXE" "$BACKEND_PID_FILE" --split-backend "$DEBUG_FLAG"
    else
        start_detached "$BACKEND_EXE" "$BACKEND_PID_FILE" --split-backend
    fi
}
```

The `start_prod_debug_*` functions stay as today — they hard-code `--debug` so the flag is always passed regardless of `DEBUG_FLAG`.

- [ ] **Step 3: Verify the script is still valid bash**

Run: `bash -n scripts/ops.sh`
Expected: no output.

- [ ] **Step 4: Verify passthrough end-to-end (no execution)**

Run: `bash -x scripts/ops.sh start-backend --SplitHost 0.0.0.0 --SplitPort 1422 --debug 2>&1 | grep -E 'start_detached|--split-backend' | head -3`
Expected: trace shows `start_detached … --split-backend --debug` (the binary may then complain it's not built — that's fine; we're only verifying the args).

Optionally stop after the trace by killing it: `pkill -f 'omnilauncher-backend' || true`.

- [ ] **Step 5: Commit**

```bash
git add scripts/ops.sh
git commit -m "feat(ops): pass --debug through ops.sh to start_{frontend,backend}"
```

---

## Task 6: `--debug` passthrough in `scripts/ops.ps1`

**Files:**
- Modify: `scripts/ops.ps1:8-24` (param block — add `[switch]$Debug`)
- Modify: `scripts/ops.ps1:70-81` (`Start-Frontend`, `Start-Backend`)

- [ ] **Step 1: Add `-Debug` switch to the param block**

In the param block (already extended with `-Role` in Task 2 step 2), add `[switch]$DebugFlag` (using `$DebugFlag` because `$Debug` is reserved by PowerShell's common parameters):

```powershell
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet(
        'stop-frontend', 'stop-backend', 'stop-all',
        'start-frontend', 'start-backend',
        'start-wsl-backend', 'restart-wsl-backend',
        'prod-debug-backend', 'prod-debug-frontend', 'prod-debug',
        'test-backend', 'remove-binary', 'prepare-binaries',
        'clean-frontend', 'clean-backend', 'clean',
        'status'
    )]
    [string]$Action,

    [string]$SplitHost   = '0.0.0.0',
    [string]$SplitPort    = '1422',
    [string]$BackendUrl   = 'http://127.0.0.1:1422',
    [string]$Role         = 'both',
    [switch]$DebugFlag
)
```

**Important:** the Makefile passes `--debug` (POSIX style) but PowerShell wants `-DebugFlag`. The Makefile platform branch needs to provide the platform-correct token. Add a Make variable:

```make
ifeq ($(PLATFORM),windows)
  OPS_ROLE_FLAG = -Role
  OPS_DEBUG_FLAG_NAME = -DebugFlag
else
  OPS_ROLE_FLAG =
  OPS_DEBUG_FLAG_NAME =
endif
```

And rewrite `DEBUG_FLAG` (from Task 4 step 1):

```make
ifeq ($(DEBUG),1)
  DEBUG_FLAG := $(if $(OPS_DEBUG_FLAG_NAME),$(OPS_DEBUG_FLAG_NAME),--debug)
else ifeq ($(VERBOSE),1)
  DEBUG_FLAG := $(if $(OPS_DEBUG_FLAG_NAME),$(OPS_DEBUG_FLAG_NAME),--debug)
else
  DEBUG_FLAG :=
endif
```

On Linux `DEBUG_FLAG` is `--debug`; on Windows it's `-DebugFlag`. Apply this in the Makefile as part of this task.

- [ ] **Step 2: Update `Start-Frontend` to honor `-DebugFlag`**

Replace `Start-Frontend` (`scripts/ops.ps1:70-74`):

```powershell
function Start-Frontend {
    Ensure-RoleBinaries
    $env:OMNILAUNCHER_BACKEND_URL = $BackendUrl
    $argList = @()
    if ($DebugFlag) { $argList += '--debug' }
    if ($argList.Count -gt 0) {
        Start-Process -FilePath $frontendExe -ArgumentList $argList -WorkingDirectory (Get-Location)
    } else {
        Start-Process -FilePath $frontendExe -WorkingDirectory (Get-Location)
    }
}
```

- [ ] **Step 3: Update `Start-Backend` to honor `-DebugFlag`**

Replace `Start-Backend` (`scripts/ops.ps1:76-81`):

```powershell
function Start-Backend {
    Ensure-RoleBinaries
    $env:OMNILAUNCHER_SPLIT_HOST = $SplitHost
    $env:OMNILAUNCHER_SPLIT_PORT = $SplitPort
    $argList = @('--split-backend')
    if ($DebugFlag) { $argList += '--debug' }
    Start-Process -FilePath $backendExe -ArgumentList $argList -WorkingDirectory (Get-Location)
}
```

`Start-ProdDebugBackend` / `Start-ProdDebugFrontend` stay as-is.

- [ ] **Step 4: Commit**

```bash
git add scripts/ops.ps1 Makefile
git commit -m "feat(ops): pass --debug/-DebugFlag through ops.ps1 with platform-aware Make flag"
```

---

## Task 7: Backend startup banner

**Files:**
- Modify: `src-tauri/src/main.rs:1665-1728` (the `fn main()` body)

- [ ] **Step 1: Read current `main()`**

Run: `sed -n '1665,1730p' src-tauri/src/main.rs`
Expected: matches the body shown in the spec (CLI arg parsing, `init_debug_logging`, split-backend branch, then `run()`).

- [ ] **Step 2: Add a startup banner helper**

Just below `fn init_debug_logging` (around `src-tauri/src/main.rs:144`) add:

```rust
/// Emit a single info-level banner so debug logs and stderr always start
/// with a clear "what process, in what mode" line.
fn log_startup_banner(role: &str, debug_enabled: bool) {
    let version = env!("CARGO_PKG_VERSION");
    let log_target = if debug_enabled {
        debug_log_path().display().to_string()
    } else {
        "stderr".to_string()
    };
    log::info!(
        "OmniLauncher starting role={role} version={version} debug={debug_enabled} log={log_target}"
    );
}
```

- [ ] **Step 3: Call the banner from `main()`**

In `fn main()`, immediately after the existing `init_debug_logging(debug_enabled);` call and the `if debug_enabled { ... } else if TermLogger::init ... {}` block (i.e. after logging is wired up), and before the `if split_backend_only { ... }` block, add:

```rust
    let role = if split_backend_only { "split-backend" } else { "tauri-shell" };
    log_startup_banner(role, debug_enabled);
```

Concretely, replace the section from the `init_debug_logging(debug_enabled);` line (around `main.rs:1669`) through to the `if split_backend_only {` line (around `main.rs:1685`) with:

```rust
    init_debug_logging(debug_enabled);

    if debug_enabled {
        log::info!("Running with --debug");
        log::debug!("CLI args: {:?}", args);
    } else if TermLogger::init(
        LevelFilter::Info,
        ConfigBuilder::new().build(),
        TerminalMode::Stderr,
        ColorChoice::Never,
    )
    .is_ok()
    {
        log::info!("Running without debug file logging");
    }

    let role = if split_backend_only { "split-backend" } else { "tauri-shell" };
    log_startup_banner(role, debug_enabled);

    if split_backend_only {
```

- [ ] **Step 4: Build**

Run: `cd src-tauri && cargo build --release 2>&1 | tail -10`
Expected: clean build, no warnings about unused `log_startup_banner` (the call site uses it).

- [ ] **Step 5: Verify the banner fires**

Run: `cd src-tauri && cargo build 2>&1 | tail -3` (debug build is faster for the next sanity check).
Run: `OMNILAUNCHER_SPLIT_PORT=14222 ./src-tauri/target/debug/omnilauncher --split-backend --debug &
sleep 1
curl -s http://127.0.0.1:14222/health
pkill -f "omnilauncher --split-backend" || true
sleep 0.5
grep "OmniLauncher starting" ~/.omnilauncher/omnilauncher.log | tail -1`

Expected output of the grep:
```
… OmniLauncher starting role=split-backend version=2.0.0 debug=true log=…/omnilauncher.log
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(backend): emit startup banner with role/version/debug/log path"
```

---

## Task 8: Promote per-request logs in `split_server.rs` to info

**Files:**
- Modify: `src-tauri/src/split_server.rs:231-287` (request handler `tokio::spawn` body)

- [ ] **Step 1: Read the current spawn body**

Run: `sed -n '220,290p' src-tauri/src/split_server.rs`
Expected: matches the body shown in the spec.

- [ ] **Step 2: Add `Instant` import (if not already imported)**

Run: `grep -n "use std::time::Instant" src-tauri/src/split_server.rs`
- If empty, add this near the top of the file (after the existing `std` imports around `src-tauri/src/split_server.rs:2`):

```rust
use std::time::Instant;
```

- If `Instant` is already imported, skip.

- [ ] **Step 3: Capture start time and rewrite the request log line**

Inside the `tokio::spawn(async move { … })` block at `src-tauri/src/split_server.rs:231`, after the `let request = String::from_utf8_lossy(...)` line and before the existing request `log::debug!` block, add:

```rust
            let started_at = Instant::now();
```

Then replace the existing block:

```rust
            log::debug!(
                "split backend request from {}: method={} path={} query={} bytes={}",
                addr,
                method,
                path,
                query,
                read_len
            );
```

with:

```rust
            log::info!(
                "→ {} {} from={} query={} bytes={}",
                method, path, addr, query, read_len
            );
```

- [ ] **Step 4: Rewrite the response log line**

Replace the existing block at ~`src-tauri/src/split_server.rs:276`:

```rust
            log::debug!(
                "split backend response to {}: method={} path={} status={} body_bytes={}",
                addr,
                method,
                path,
                response.status,
                response.body.len()
            );
```

with:

```rust
            let elapsed_ms = started_at.elapsed().as_millis();
            log::info!(
                "← {} {} status={} body_bytes={} elapsed_ms={}",
                method, path, response.status, response.body.len(), elapsed_ms
            );
```

- [ ] **Step 5: Add a SSE exit log**

Find the SSE branch (around `src-tauri/src/split_server.rs:258-273`). Just before the final `let _ = stream.shutdown().await; return;` in that branch, insert:

```rust
                let elapsed_ms = started_at.elapsed().as_millis();
                log::info!(
                    "← {} {} status=200 sse_closed elapsed_ms={}",
                    method, path, elapsed_ms
                );
```

The SSE branch already has the existing `log::debug!` subscribe line — that stays at debug.

- [ ] **Step 6: Build and run the existing tests**

Run: `cd src-tauri && cargo build --release 2>&1 | tail -5`
Expected: no warnings about unused `Instant` import.

Run: `cd src-tauri && cargo test --lib split_server 2>&1 | tail -5`
Expected: existing tests pass (this change is logging-only).

- [ ] **Step 7: Smoke verify with curl**

Run:

```bash
cd src-tauri && cargo build 2>&1 | tail -1
OMNILAUNCHER_SPLIT_PORT=14222 ./target/debug/omnilauncher --split-backend --debug &
sleep 1
curl -s http://127.0.0.1:14222/health
pkill -f "omnilauncher --split-backend" || true
sleep 0.5
grep -E "→ GET /health|← GET /health" ~/.omnilauncher/omnilauncher.log | tail -2
```

Expected: two lines, one `→ GET /health from=…` and one `← GET /health status=200 body_bytes=… elapsed_ms=…`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/split_server.rs
git commit -m "feat(backend): promote per-request split-server logs to info with elapsed_ms"
```

---

## Task 9: Promote AI client request/response logs to info

**Files:**
- Modify: `src-tauri/src/ai/client.rs:244` (request log)
- Modify: `src-tauri/src/ai/client.rs:299` (response log)

- [ ] **Step 1: Read both locations**

Run: `sed -n '240,310p' src-tauri/src/ai/client.rs`
Expected: shows the two `log::debug!` blocks for `AI request →` and `AI response ←`.

- [ ] **Step 2: Promote both to `log::info!`**

In `src-tauri/src/ai/client.rs:244`, change:

```rust
        log::debug!(
            "AI request → endpoint={} model={} messages={} tools={} auth={}",
```

to:

```rust
        log::info!(
            "AI request → endpoint={} model={} messages={} tools={} auth={}",
```

In `src-tauri/src/ai/client.rs:299`, change:

```rust
        log::debug!(
            "AI response ← status={} in {} ms (model={})",
```

to:

```rust
        log::info!(
            "AI response ← status={} in {} ms (model={})",
```

Leave the streaming-related `debug!` blocks (e.g. around `src-tauri/src/ai/client.rs:156, 359`) untouched — they're chunk-level and would be noisy.

- [ ] **Step 3: Build**

Run: `cd src-tauri && cargo build --release 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 4: Run AI client tests**

Run: `cd src-tauri && cargo test --lib ai::client 2>&1 | tail -5`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/client.rs
git commit -m "feat(backend): promote AI request/response logs from debug to info"
```

---

## Task 10: Extract `frontendLog` to `src/lib/logger.ts` with a level gate

**Files:**
- Create: `src/lib/logger.ts`
- Modify: `src/lib/runtime.ts:1-40` (remove inline `frontendLog` + `summarizeArgs`; import them)

- [ ] **Step 1: Write the new `logger.ts`**

Create `src/lib/logger.ts` with:

```typescript
// Level-gated frontend logger. Mirrors backend log levels and forwards to the
// Tauri `frontend_log` command when running inside the desktop shell so the
// two logs interleave in ~/.omnilauncher/omnilauncher.log.
//
// Active level (highest priority first):
//   1. URL query param ?log=trace|debug|info|warn|error
//   2. localStorage.OMNI_LOG_LEVEL
//   3. import.meta.env.DEV  → "debug"
//   4. fallback              → "info"
//
// Below-threshold calls are no-ops: no console output, no Tauri invoke.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

const ORDER: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

const isTauriRuntime = () =>
  typeof window !== "undefined" && !!(window as any).__TAURI_INTERNALS__;

function readUrlLevel(): LogLevel | null {
  if (typeof window === "undefined") return null;
  try {
    const params = new URLSearchParams(window.location.search);
    const raw = params.get("log");
    if (raw && raw in ORDER) return raw as LogLevel;
  } catch {
    /* ignore */
  }
  return null;
}

function readStorageLevel(): LogLevel | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage?.getItem("OMNI_LOG_LEVEL");
    if (raw && raw in ORDER) return raw as LogLevel;
  } catch {
    /* ignore */
  }
  return null;
}

function defaultLevel(): LogLevel {
  // Vite injects import.meta.env.DEV at build time.
  try {
    if ((import.meta as any).env?.DEV) return "debug";
  } catch {
    /* ignore */
  }
  return "info";
}

let active: LogLevel =
  readUrlLevel() ?? readStorageLevel() ?? defaultLevel();

export function getLevel(): LogLevel {
  return active;
}

export function setLevel(level: LogLevel): void {
  active = level;
  try {
    window.localStorage?.setItem("OMNI_LOG_LEVEL", level);
  } catch {
    /* ignore */
  }
}

function enabled(level: LogLevel): boolean {
  return ORDER[level] >= ORDER[active];
}

function emit(level: LogLevel, message: string): void {
  if (!enabled(level)) return;
  const line = `[omni ${level}] ${message}`;
  const fn =
    level === "error" ? console.error :
    level === "warn"  ? console.warn  :
    console.log;
  fn(line);

  if (!isTauriRuntime()) return;
  tauriInvoke("frontend_log", { level, message: line }).catch(() => {
    // Logging must never break app behavior.
  });
}

export function summarizeArgs(args?: Record<string, unknown>): string {
  if (!args) return "none";
  try {
    return JSON.stringify(args, (key, value) => {
      const lower = key.toLowerCase();
      if (lower.includes("key") || lower.includes("token") || lower.includes("secret")) {
        return value ? "[redacted]" : value;
      }
      if (typeof value === "string" && value.length > 160) {
        return `${value.slice(0, 160)}...(${value.length} chars)`;
      }
      return value;
    });
  } catch {
    return "[unserializable]";
  }
}

export const logger = {
  trace: (msg: string) => emit("trace", msg),
  debug: (msg: string) => emit("debug", msg),
  info:  (msg: string) => emit("info",  msg),
  warn:  (msg: string) => emit("warn",  msg),
  error: (msg: string) => emit("error", msg),
  getLevel,
  setLevel,
};
```

- [ ] **Step 2: Update `runtime.ts` to import from `./logger`**

In `src/lib/runtime.ts`, replace lines 11-40 (the local `FrontendLogLevel`, `frontendLog`, and `summarizeArgs` definitions, plus any leftover blank lines before the next code) with:

```typescript
import { logger, summarizeArgs, type LogLevel } from "./logger";

type FrontendLogLevel = LogLevel;

function frontendLog(level: FrontendLogLevel, message: string) {
  logger[level](`[runtime] ${message}`);
}
```

This keeps the existing call-sites in `runtime.ts` (`frontendLog("debug", …)`) intact — they just route through `logger` now.

Confirm the existing `import { invoke as tauriInvoke } from "@tauri-apps/api/core";` line at the top of `runtime.ts` is still needed elsewhere in the file (it is — `runtime.ts` uses `tauriInvoke` for the Tauri path). Leave it.

- [ ] **Step 3: Frontend type-check**

Run: `npx tsc --noEmit 2>&1 | tail -10`
Expected: no new errors. If errors mention `summarizeArgs` not exported, double-check the `export` in `logger.ts`.

- [ ] **Step 4: Run frontend tests**

Run: `npm test -- --run 2>&1 | tail -10`
Expected: existing tests pass (logger extraction is no-op semantically).

- [ ] **Step 5: Commit**

```bash
git add src/lib/logger.ts src/lib/runtime.ts
git commit -m "feat(frontend): extract logger to src/lib/logger.ts with level gate"
```

---

## Task 11: Startup banner in `App.tsx`

**Files:**
- Modify: `src/App.tsx` (top-of-component effect)

- [ ] **Step 1: Locate the top-level component**

Run: `grep -n "function App\|export default\|useEffect" src/App.tsx | head -10`
Expected: identifies the `App` function component and an existing or near-top `useEffect`.

- [ ] **Step 2: Add the import**

At the top of `src/App.tsx`, alongside existing imports, add:

```typescript
import { logger } from "./lib/logger";
import { getBackendMode, getBackendUrl } from "./lib/runtime";
```

If `getBackendUrl` is not exported from `runtime.ts`, check first:

Run: `grep -n "export.*backendUrl\|export function backendUrl\|export function getBackendUrl" src/lib/runtime.ts`

- If `backendUrl` (camelCase function) is exported, import that instead:
  ```typescript
  import { getBackendMode, backendUrl } from "./lib/runtime";
  ```
- If neither is exported, add this export to `src/lib/runtime.ts` (next to `getBackendMode`):
  ```typescript
  export function getBackendUrl(): string {
    return backendUrl();
  }
  ```
  Then use the import above.

- [ ] **Step 3: Emit the banner on initial mount**

Inside the `App` component body, add (placing it before any other `useEffect`):

```tsx
  React.useEffect(() => {
    const mode = getBackendMode();
    const url = mode === "http" ? getBackendUrl() : "(tauri ipc)";
    const dev = !!(import.meta as any).env?.DEV;
    logger.info(`OmniLauncher UI mounted backend=${url} mode=${mode} dev=${dev}`);
    // Run once on mount; deps intentionally empty.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
```

If the file already imports `useEffect` directly (e.g. `import { useEffect } from "react"`), use `useEffect(...)` without the `React.` prefix.

- [ ] **Step 4: Type-check and lint**

Run: `npx tsc --noEmit 2>&1 | tail -10`
Expected: clean.

- [ ] **Step 5: Build the frontend**

Run: `npm run build 2>&1 | tail -10`
Expected: build succeeds.

- [ ] **Step 6: Commit**

```bash
git add src/App.tsx src/lib/runtime.ts
git commit -m "feat(frontend): emit startup banner on App mount via logger"
```

---

## Task 12: Final verification

**Files:** none modified.

- [ ] **Step 1: Run the full Rust test suite**

Run: `cd src-tauri && cargo test 2>&1 | tail -15`
Expected: all tests pass.

- [ ] **Step 2: Run the frontend test suite**

Run: `npm test -- --run 2>&1 | tail -15`
Expected: all tests pass.

- [ ] **Step 3: End-to-end: build, restart with DEBUG=1, check logs**

```bash
make build 2>&1 | tail -5
ls -1 src-tauri/target/release/omnilauncher*
```

Expected: lists `omnilauncher-frontend` and `omnilauncher-backend`. No bare `omnilauncher`.

```bash
SPLIT_PORT=14222 make restart-backend DEBUG=1 2>&1 | tail -5
sleep 2
curl -s http://127.0.0.1:14222/health
sleep 0.5
tail -20 ~/.omnilauncher/omnilauncher.log
make stop-backend
```

Expected: log file contains
- `OmniLauncher starting role=split-backend version=2.0.0 debug=true log=…`
- `split backend listening on http://0.0.0.0:14222`
- `→ GET /health from=… query= bytes=…`
- `← GET /health status=200 body_bytes=… elapsed_ms=…`

- [ ] **Step 4: Verify VERBOSE=1 alias works**

```bash
SPLIT_PORT=14222 make restart-backend VERBOSE=1 2>&1 | tail -3
sleep 2
ls -la ~/.omnilauncher/omnilauncher.log
make stop-backend
```

Expected: log file mtime updated within the last few seconds — meaning `--debug` was passed and trace logging was enabled.

- [ ] **Step 5: Verify no-flag case has no debug-file growth**

```bash
make stop-backend
LAST_SIZE=$(stat -c %s ~/.omnilauncher/omnilauncher.log 2>/dev/null || echo 0)
SPLIT_PORT=14222 make restart-backend 2>&1 | tail -3
sleep 2
curl -s http://127.0.0.1:14222/health >/dev/null
sleep 0.5
NEW_SIZE=$(stat -c %s ~/.omnilauncher/omnilauncher.log 2>/dev/null || echo 0)
make stop-backend
echo "before=$LAST_SIZE after=$NEW_SIZE"
```

Expected: `before == after` (no debug file logging when neither flag is set). Stderr should still have shown info-level startup banner + per-request lines during the run.

- [ ] **Step 6: Final commit (if any tweaks needed; otherwise skip)**

If verification surfaced any small fixes, commit them with a `chore: tweak …` message. Otherwise this task has nothing to commit.

---

## Self-Review Notes

Spec coverage (each spec section → tasks):

| Spec section | Covered by |
|---|---|
| §1 Role-named binaries | Tasks 1, 2, 3 |
| §2 DEBUG/VERBOSE flag | Tasks 4, 5, 6 |
| §3 Startup banner (backend) | Task 7 |
| §3 Per-request logs (split_server) | Task 8 |
| §3 AI client logs | Task 9 |
| §3 Live server | (no change needed — already at info) |
| §3 Frontend logger extract + level gate | Task 10 |
| §3 Frontend App.tsx banner | Task 11 |
| Testing checklist | Task 12 |

Placeholder check: none — every code change has the actual code; verification commands have expected outputs.

Type consistency: `prepare_binaries` takes a `role` arg in both `ops.sh` and `ops.ps1` (Task 1/2). `Prepare-Binaries` parameter is named `-Which` internally but exposed as the script-level `-Role` parameter (Task 2) — the Makefile uses `-Role`. The `frontendLog`/`logger`/`summarizeArgs` names are used consistently between Tasks 10 and 11.
