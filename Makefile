.PHONY: help \
        build build-frontend build-backend \
        stop stop-frontend stop-backend stop-all \
        start start-frontend start-backend \
        restart restart-frontend restart-backend \
        clean clean-frontend clean-backend \
        remove-binary prepare-binaries \
        logs status \
        prod-debug prod-debug-backend prod-debug-frontend \
        test-frontend test-rust test-backend test-health test-smoke test-e2e test-unit test-all \
        start-wsl-backend restart-wsl-backend

# -- Configuration ------------------------------------------------------------

NPM          ?= npm
NPX          ?= npx
CARGO        ?= cargo
SPLIT_HOST   ?= 0.0.0.0
SPLIT_PORT   ?= 1422
BACKEND_URL  ?= http://127.0.0.1:$(SPLIT_PORT)

# Backend mode: local | wsl | remote
#   local  - build and run backend on this machine (default)
#   wsl    - build and run backend inside WSL
#   remote - connect to an already-running backend at $(BACKEND_URL)
BACKEND_MODE ?= local

# Set REBUILD=1 to force a rebuild before starting:
#   make start-backend              # start existing binary
#   make start-backend REBUILD=1    # rebuild, then start
REBUILD ?= 0

ifeq ($(OS),Windows_NT)
  PLATFORM := windows
else
  PLATFORM := unix
endif

OPS = powershell -NoProfile -File scripts/ops.ps1

# -- Help ---------------------------------------------------------------------

help:
	$(info OmniLauncher - Makefile targets)
	$(info )
	$(info [Build])
	$(info   build-frontend    Build frontend release binary and role copies)
	$(info   build-backend     Build backend release binary and role copies)
	$(info   build             Build both)
	$(info )
	$(info [Stop])
	$(info   stop-frontend     Stop frontend process)
	$(info   stop-backend      Stop backend process)
	$(info   stop-all          Stop both)
	$(info )
	$(info [Start] use REBUILD=1 to rebuild first)
	$(info   start-frontend    Start frontend release binary)
	$(info   start-backend     Start backend release binary)
	$(info   start             Start both)
	$(info )
	$(info [Restart] stop + remove binary + rebuild + start)
	$(info   restart-frontend  Rebuild and restart frontend)
	$(info   restart-backend   Rebuild and restart backend)
	$(info   restart           Rebuild and restart both)
	$(info )
	$(info [Backend modes])
	$(info   start-wsl-backend    Build and run backend inside WSL)
	$(info   restart-wsl-backend  Rebuild and restart backend inside WSL)
	$(info   prod-debug-backend   Start backend with --debug logging)
	$(info   prod-debug-frontend  Start frontend with --debug logging)
	$(info   prod-debug           Start both with --debug logging)
	$(info )
	$(info   Variables:)
	$(info     BACKEND_MODE=local|wsl|remote  default: local)
	$(info     BACKEND_URL=<url>              backend API URL)
	$(info     SPLIT_HOST=<host>              backend bind host)
	$(info     SPLIT_PORT=<port>              backend bind port)
	$(info     REBUILD=1                      rebuild before start)
	$(info )
	$(info [Test])
	$(info   test-frontend     Run frontend unit tests via vitest)
	$(info   test-rust         Run backend unit tests via cargo test)
	$(info   test-unit         Run both frontend + backend unit tests)
	$(info   test-backend      Check if backend is responding on $(BACKEND_URL))
	$(info   test-health       Alias for test-backend)
	$(info   test-smoke        Run expanded smoke tests against running backend)
	$(info   test-e2e          Run full E2E test mimicking frontend user flow)
	$(info   test-all          Run all tests: unit + smoke + e2e)
	$(info )
	$(info [Clean])
	$(info   clean             Remove all build artifacts)
	$(info   clean-frontend    Remove frontend build artifacts dist/)
	$(info   clean-backend     Remove backend build artifacts src-tauri/target/)
	$(info   remove-binary     Remove release and role binaries)
	$(info   prepare-binaries  Create frontend/backend role binary names)
	$(info )
	$(info [Logs])
	$(info   logs              Tail the debug log file)
	$(info )
	$(info [Status])
	$(info   status            Show process, port, and health status)
	@:

# -- Build --------------------------------------------------------------------

build-frontend:
	$(NPM) run build
	$(NPX) tauri build --no-bundle
	$(OPS) prepare-binaries

build-backend:
	cd src-tauri && $(CARGO) build --release
	$(OPS) prepare-binaries

build: build-frontend build-backend

# -- Stop ---------------------------------------------------------------------

stop-frontend:
	$(OPS) stop-frontend

stop-backend:
	$(OPS) stop-backend

stop-all: stop-frontend stop-backend

# -- Remove binary ------------------------------------------------------------

remove-binary:
	$(OPS) remove-binary

prepare-binaries:
	$(OPS) prepare-binaries

# -- Start --------------------------------------------------------------------

# Conditional rebuild helper: when REBUILD=1, remove binary and rebuild
# before the actual start step. Default REBUILD=0 starts the existing binary.
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

# -- Frontend start -----------------------------------------------------------

start-frontend: stop-frontend maybe-rebuild-frontend
	$(OPS) start-frontend --BackendUrl "$(BACKEND_URL)"

# -- Backend start local ------------------------------------------------------

start-backend: stop-backend maybe-rebuild-backend
	$(OPS) start-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"

start: start-backend start-frontend

# -- WSL backend --------------------------------------------------------------
# Builds and runs the backend inside WSL. The frontend on Windows connects
# via BACKEND_URL. The default http://127.0.0.1:1422 works with WSL2 forwarding.

start-wsl-backend:
	$(OPS) start-wsl-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)" --BackendUrl "$(BACKEND_URL)"

restart-wsl-backend:
	$(OPS) restart-wsl-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"

# -- Restart ------------------------------------------------------------------

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

# -- Clean --------------------------------------------------------------------

clean-frontend:
	$(OPS) clean-frontend

clean-backend:
	$(OPS) clean-backend

clean: clean-frontend clean-backend

# -- Logs ---------------------------------------------------------------------

logs:
	pwsh -NoProfile -File scripts/logs.ps1

# -- Status -------------------------------------------------------------------

status:
	$(OPS) status --BackendUrl "$(BACKEND_URL)" --SplitPort "$(SPLIT_PORT)"

# -- Debug release binaries with --debug --------------------------------------

prod-debug-backend: stop-backend maybe-rebuild-backend
	$(OPS) prod-debug-backend --SplitHost "$(SPLIT_HOST)" --SplitPort "$(SPLIT_PORT)"

prod-debug-frontend: stop-frontend maybe-rebuild-frontend
	$(OPS) prod-debug-frontend --BackendUrl "$(BACKEND_URL)"

prod-debug: prod-debug-backend prod-debug-frontend

# -- Test ---------------------------------------------------------------------

test-frontend:
	$(NPM) test

test-rust:
	cd src-tauri && $(CARGO) test

test-backend:
	$(OPS) test-backend --BackendUrl "$(BACKEND_URL)"

test-health: test-backend

test-smoke:
	pwsh -NoProfile -File scripts/smoke-endpoints.ps1 -BaseUrl "$(BACKEND_URL)"

test-e2e:
	pwsh -NoProfile -File scripts/test-e2e.ps1 -BaseUrl "$(BACKEND_URL)"

test-unit: test-frontend test-rust

test-all: test-unit test-smoke test-e2e
