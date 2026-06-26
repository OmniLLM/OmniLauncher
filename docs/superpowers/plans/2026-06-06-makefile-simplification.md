# Makefile Simplification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the 300-line top-level Makefile with a compact command interface backed by focused included Makefiles, while preserving existing targets as compatibility aliases.

**Architecture:** The top-level `Makefile` becomes a thin include hub. `make/config.mk` owns defaults and debug flag normalization, `make/platform.mk` owns helper command selection, `make/help.mk` owns user-facing help, `make/commands.mk` owns canonical variable-driven workflows, and `make/aliases.mk` owns old target names.

**Tech Stack:** GNU Make, npm, npx, Cargo, existing bash/PowerShell helper scripts under `scripts/`.

---

## File Structure

- Modify: `Makefile` — reduce to `.PHONY` declarations and `include make/*.mk` lines.
- Create: `make/config.mk` — project command defaults and normalized role/test/debug variables.
- Create: `make/platform.mk` — Windows vs Unix script command variables and PowerShell/bash argument differences.
- Create: `make/help.mk` — compact `help` plus `help-advanced` for compatibility aliases.
- Create: `make/commands.mk` — canonical `build`, `start`, `stop`, `restart`, `test`, `clean`, `status`, `logs`, `remove-binary`, and `prepare-binaries` targets.
- Create: `make/aliases.mk` — old targets implemented via `$(MAKE)` delegation or direct helper calls when necessary.

## Task 1: Create focused Makefile includes

**Files:**
- Create: `make/config.mk`
- Create: `make/platform.mk`
- Create: `make/help.mk`
- Create: `make/commands.mk`
- Create: `make/aliases.mk`
- Modify: `Makefile`

- [ ] **Step 1: Create `make/config.mk`**

```make
# -- Configuration ------------------------------------------------------------

NPM          ?= npm
NPX          ?= npx
CARGO        ?= cargo
SPLIT_HOST   ?= 0.0.0.0
SPLIT_PORT   ?= 1422
BACKEND_URL  ?= http://127.0.0.1:$(SPLIT_PORT)

# Canonical command selectors.
ROLE         ?= both
KIND         ?= all

# Backend mode: local | wsl | remote
BACKEND_MODE ?= local

# Set REBUILD=1 to force a rebuild before starting.
REBUILD ?= 0

# Set DEBUG=1 or VERBOSE=1 to start binaries with --debug.
DEBUG   ?= 0
VERBOSE ?= 0
```

- [ ] **Step 2: Create `make/platform.mk`**

```make
# -- Platform-specific helper invocations -------------------------------------

ifeq ($(OS),Windows_NT)
  PLATFORM := windows
else
  PLATFORM := unix
endif

ifeq ($(PLATFORM),windows)
  OPS        = powershell -NoProfile -File scripts/ops.ps1
  LOGS_CMD   = pwsh -NoProfile -File scripts/logs.ps1
  SMOKE_CMD  = pwsh -NoProfile -File scripts/smoke-endpoints.ps1
  E2E_CMD    = pwsh -NoProfile -File scripts/test-e2e.ps1
  OPS_ROLE_FLAG       = -Role
  OPS_DEBUG_FLAG_NAME = -DebugFlag
else
  OPS        = bash scripts/ops.sh
  LOGS_CMD   = bash scripts/logs.sh
  SMOKE_CMD  = bash scripts/smoke-endpoints.sh
  E2E_CMD    = bash scripts/test-e2e.sh
  OPS_ROLE_FLAG       =
  OPS_DEBUG_FLAG_NAME =
endif

ifeq ($(DEBUG),1)
  DEBUG_FLAG := $(if $(OPS_DEBUG_FLAG_NAME),$(OPS_DEBUG_FLAG_NAME),--debug)
else ifeq ($(VERBOSE),1)
  DEBUG_FLAG := $(if $(OPS_DEBUG_FLAG_NAME),$(OPS_DEBUG_FLAG_NAME),--debug)
else
  DEBUG_FLAG :=
endif
```

- [ ] **Step 3: Create `make/help.mk`**

```make
# -- Help ---------------------------------------------------------------------

help:
	$(info OmniLauncher - common Make targets)
	$(info )
	$(info   make build   [ROLE=frontend|backend|both])
	$(info   make start   [ROLE=frontend|backend|both] [BACKEND_MODE=local|wsl|remote] [DEBUG=1])
	$(info   make stop    [ROLE=frontend|backend|both])
	$(info   make restart [ROLE=frontend|backend|both] [BACKEND_MODE=local|wsl|remote] [DEBUG=1])
	$(info   make test    [KIND=frontend|rust|unit|backend|health|smoke|e2e|all])
	$(info   make clean   [ROLE=frontend|backend|both])
	$(info   make status)
	$(info   make logs)
	$(info )
	$(info Defaults: ROLE=both KIND=all BACKEND_MODE=local SPLIT_PORT=$(SPLIT_PORT))
	$(info Run 'make help-advanced' for compatibility aliases and variables.)
	@:

help-advanced:
	$(info OmniLauncher - advanced Make targets)
	$(info )
	$(info Compatibility aliases:)
	$(info   build-frontend build-backend)
	$(info   start-frontend start-backend start-wsl-backend)
	$(info   stop-frontend stop-backend stop-all)
	$(info   restart-frontend restart-backend restart-wsl-backend)
	$(info   prod-debug prod-debug-backend prod-debug-frontend)
	$(info   test-frontend test-rust test-unit test-backend test-health test-smoke test-e2e test-all)
	$(info   clean-frontend clean-backend remove-binary prepare-binaries)
	$(info )
	$(info Variables:)
	$(info   ROLE=frontend|backend|both       default: both)
	$(info   KIND=frontend|rust|unit|backend|health|smoke|e2e|all  default: all)
	$(info   BACKEND_MODE=local|wsl|remote    default: local)
	$(info   BACKEND_URL=<url>                default: $(BACKEND_URL))
	$(info   SPLIT_HOST=<host>                default: $(SPLIT_HOST))
	$(info   SPLIT_PORT=<port>                default: $(SPLIT_PORT))
	$(info   REBUILD=1                        rebuild before start)
	$(info   DEBUG=1 or VERBOSE=1             start binary with --debug)
	@:
```

- [ ] **Step 4: Create `make/commands.mk`**

```make
# -- Internal command fragments ------------------------------------------------

build-frontend-command:
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) frontend

build-backend-command:
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) backend

maybe-rebuild-frontend:
ifeq ($(REBUILD),1)
	$(OPS) remove-binary
	$(MAKE) build-frontend-command
endif

maybe-rebuild-backend:
ifeq ($(REBUILD),1)
	$(OPS) remove-binary
	$(MAKE) build-backend-command
endif

# -- Canonical build -----------------------------------------------------------

build:
ifeq ($(ROLE),frontend)
	$(MAKE) build-frontend-command
else ifeq ($(ROLE),backend)
	$(MAKE) build-backend-command
else ifeq ($(ROLE),both)
	$(MAKE) build-frontend-command
	$(MAKE) build-backend-command
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical stop ------------------------------------------------------------

stop:
ifeq ($(ROLE),frontend)
	$(OPS) stop-frontend
else ifeq ($(ROLE),backend)
	$(OPS) stop-backend
else ifeq ($(ROLE),both)
	$(OPS) stop-all
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical start -----------------------------------------------------------

start:
ifeq ($(ROLE),frontend)
	$(MAKE) maybe-rebuild-frontend
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)
else ifeq ($(ROLE),backend)
ifeq ($(BACKEND_MODE),wsl)
	$(OPS) start-wsl-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" --BackendUrl "$(BACKEND_URL)"
else ifeq ($(BACKEND_MODE),remote)
	$(info BACKEND_MODE=remote: not starting backend; using $(BACKEND_URL))
else ifeq ($(BACKEND_MODE),local)
	$(MAKE) maybe-rebuild-backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" $(DEBUG_FLAG)
else
	$(error BACKEND_MODE must be local, wsl, or remote)
endif
else ifeq ($(ROLE),both)
ifeq ($(BACKEND_MODE),wsl)
	$(OPS) start-wsl-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" --BackendUrl "$(BACKEND_URL)"
else ifeq ($(BACKEND_MODE),remote)
	$(info BACKEND_MODE=remote: not starting backend; using $(BACKEND_URL))
else ifeq ($(BACKEND_MODE),local)
	$(MAKE) maybe-rebuild-backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" $(DEBUG_FLAG)
else
	$(error BACKEND_MODE must be local, wsl, or remote)
endif
	$(MAKE) maybe-rebuild-frontend
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical restart ---------------------------------------------------------

restart:
	$(MAKE) stop ROLE=$(ROLE)
ifeq ($(BACKEND_MODE),wsl)
ifeq ($(ROLE),backend)
	$(OPS) restart-wsl-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"
else ifeq ($(ROLE),both)
	$(OPS) restart-wsl-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"
	$(OPS) remove-binary
	$(MAKE) build-frontend-command
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)
else
	$(OPS) remove-binary
	$(MAKE) build-frontend-command
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)" $(DEBUG_FLAG)
endif
else
	$(OPS) remove-binary
	$(MAKE) build ROLE=$(ROLE)
	$(MAKE) start ROLE=$(ROLE) BACKEND_MODE=$(BACKEND_MODE) REBUILD=0 DEBUG=$(DEBUG) VERBOSE=$(VERBOSE)
endif

# -- Canonical clean -----------------------------------------------------------

clean:
ifeq ($(ROLE),frontend)
	$(OPS) clean-frontend
else ifeq ($(ROLE),backend)
	$(OPS) clean-backend
else ifeq ($(ROLE),both)
	$(OPS) clean
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical test ------------------------------------------------------------

test:
ifeq ($(KIND),frontend)
	$(NPM) test
else ifeq ($(KIND),rust)
	cd src-tauri && $(CARGO) test
else ifeq ($(KIND),unit)
	$(MAKE) test KIND=frontend
	$(MAKE) test KIND=rust
else ifeq ($(KIND),backend)
	$(OPS) test-backend --BackendUrl "$(BACKEND_URL)"
else ifeq ($(KIND),health)
	$(OPS) test-backend --BackendUrl "$(BACKEND_URL)"
else ifeq ($(KIND),smoke)
	$(SMOKE_CMD) -BaseUrl "$(BACKEND_URL)"
else ifeq ($(KIND),e2e)
	$(E2E_CMD) -BaseUrl "$(BACKEND_URL)"
else ifeq ($(KIND),all)
	$(MAKE) test KIND=unit
	$(MAKE) test KIND=smoke
	$(MAKE) test KIND=e2e
else
	$(error KIND must be frontend, rust, unit, backend, health, smoke, e2e, or all)
endif

# -- Other common commands -----------------------------------------------------

logs:
	$(LOGS_CMD)

status:
	$(OPS) status --BackendUrl "$(BACKEND_URL)" --SplitPort "$(SPLIT_PORT)"

remove-binary:
	$(OPS) remove-binary

prepare-binaries:
	$(OPS) prepare-binaries $(OPS_ROLE_FLAG) $(ROLE)
```

- [ ] **Step 5: Create `make/aliases.mk`**

```make
# -- Compatibility aliases -----------------------------------------------------

build-frontend:
	$(MAKE) build ROLE=frontend

build-backend:
	$(MAKE) build ROLE=backend

stop-frontend:
	$(MAKE) stop ROLE=frontend

stop-backend:
	$(MAKE) stop ROLE=backend

stop-all:
	$(MAKE) stop ROLE=both

start-frontend:
	$(MAKE) start ROLE=frontend

start-backend:
	$(MAKE) start ROLE=backend

start-wsl-backend:
	$(MAKE) start ROLE=backend BACKEND_MODE=wsl

restart-frontend:
	$(MAKE) restart ROLE=frontend

restart-backend:
	$(MAKE) restart ROLE=backend

restart-wsl-backend:
	$(MAKE) restart ROLE=backend BACKEND_MODE=wsl

prod-debug-backend:
	$(MAKE) start ROLE=backend DEBUG=1

prod-debug-frontend:
	$(MAKE) start ROLE=frontend DEBUG=1

prod-debug:
	$(MAKE) start ROLE=both DEBUG=1

clean-frontend:
	$(MAKE) clean ROLE=frontend

clean-backend:
	$(MAKE) clean ROLE=backend

test-frontend:
	$(MAKE) test KIND=frontend

test-rust:
	$(MAKE) test KIND=rust

test-unit:
	$(MAKE) test KIND=unit

test-backend:
	$(MAKE) test KIND=backend

test-health:
	$(MAKE) test KIND=health

test-smoke:
	$(MAKE) test KIND=smoke

test-e2e:
	$(MAKE) test KIND=e2e

test-all:
	$(MAKE) test KIND=all
```

- [ ] **Step 6: Replace the top-level `Makefile`**

```make
.PHONY: help help-advanced \
        build build-frontend build-backend build-frontend-command build-backend-command \
        stop stop-frontend stop-backend stop-all \
        start start-frontend start-backend start-wsl-backend \
        restart restart-frontend restart-backend restart-wsl-backend \
        clean clean-frontend clean-backend \
        remove-binary prepare-binaries \
        logs status \
        prod-debug prod-debug-backend prod-debug-frontend \
        test test-frontend test-rust test-backend test-health test-smoke test-e2e test-unit test-all \
        maybe-rebuild-frontend maybe-rebuild-backend

include make/config.mk
include make/platform.mk
include make/help.mk
include make/commands.mk
include make/aliases.mk
```

- [ ] **Step 7: Verify Makefile parsing and help**

Run: `make help`
Expected: exits 0 and shows the compact common target list.

Run: `make help-advanced`
Expected: exits 0 and shows compatibility aliases and variables.

- [ ] **Step 8: Verify representative dry-runs**

Run: `make -n start-backend`
Expected: exits 0 and shows delegation to `make start ROLE=backend`, then `scripts/ops.sh start-backend` or PowerShell equivalent.

Run: `make -n restart-frontend`
Expected: exits 0 and shows stop/remove/build/start frontend workflow.

Run: `make -n test-smoke`
Expected: exits 0 and shows delegation to `make test KIND=smoke` and the smoke endpoint script.

## Task 2: Run tests and fix any Makefile issues

**Files:**
- Modify only files created or changed in Task 1 if test failures reveal Make syntax or behavior bugs.

- [ ] **Step 1: Run frontend tests through canonical target**

Run: `make test KIND=frontend`
Expected: npm test suite passes.

- [ ] **Step 2: Run Rust tests through canonical target**

Run: `make test KIND=rust`
Expected: cargo test suite passes.

- [ ] **Step 3: Run unit alias target**

Run: `make test KIND=unit`
Expected: frontend and Rust tests both pass through the variable-driven `test` target.

- [ ] **Step 4: Run compatibility alias dry-run checks**

Run: `make -n test-unit`
Expected: delegates to `make test KIND=unit`.

Run: `make -n prod-debug`
Expected: delegates to `make start ROLE=both DEBUG=1` and uses `--debug` or `-DebugFlag`.

- [ ] **Step 5: Report verification honestly**

If any test fails, include the failing command and output. If all commands pass, state that the Makefile refactor is complete and verified.

## Self-Review

Spec coverage:
- Compact user interface: Task 1, Steps 3 and 6.
- Compatibility aliases: Task 1, Step 5.
- Split file structure: Task 1, Steps 1-6.
- WSL and remote behavior: Task 1, Step 4.
- Debug behavior: Task 1, Steps 2, 4, and 5.
- Testing and verification: Task 1 Steps 7-8 and Task 2.

Placeholder scan: no TBD/TODO/implement-later placeholders are present.

Type consistency: selectors are consistently `ROLE`, `KIND`, `BACKEND_MODE`, `DEBUG`, and `VERBOSE` across all tasks.
