# -- Internal command fragments ------------------------------------------------
#
# Single self-dispatching binary: `cargo build --release` produces one
# `omnilauncher` artifact that owns every runtime mode (GUI, serve, and the `ol`
# CLI) AND every lifecycle op (start/stop/restart/status/health/logs). There is
# no shell/PowerShell ops wrapper anymore — recipes invoke `$(BIN)` subcommands
# directly, so behavior is identical on Linux, macOS, and Windows.
#
# Host/port/backend-url are passed to the binary via the environment variables
# it already reads (OMNILAUNCHER_SERVER_HOST / _SERVER_PORT / _BACKEND_URL),
# inlined on the same recipe line so they apply to that single invocation.

build-frontend-command:
	$(NPM) run build
	$(NPX) tauri build --no-bundle

build-backend-command:
	cd src-tauri && $(CARGO) build --release

maybe-rebuild-frontend:
ifeq ($(REBUILD),1)
	rm -f $(BIN)
	$(MAKE) build-frontend-command
endif

maybe-rebuild-backend:
ifeq ($(REBUILD),1)
	rm -f $(BIN)
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
#
# frontend → detached GUI shell; backend → managed serve; both → everything.

stop:
ifeq ($(ROLE),frontend)
	$(BIN) stop --gui
else ifeq ($(ROLE),backend)
	$(BIN) stop
else ifeq ($(ROLE),both)
	$(BIN) stop --all
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical start -----------------------------------------------------------

start:
ifeq ($(ROLE),frontend)
	$(MAKE) maybe-rebuild-frontend
	OMNILAUNCHER_BACKEND_URL="$(BACKEND_URL)" $(BIN) gui --detached $(DEBUG_FLAG)
else ifeq ($(ROLE),backend)
ifeq ($(BACKEND_MODE),wsl)
	$(MAKE) start-wsl-backend-command
else ifeq ($(BACKEND_MODE),remote)
	$(info BACKEND_MODE=remote: not starting backend; using $(BACKEND_URL))
else ifeq ($(BACKEND_MODE),local)
	$(MAKE) maybe-rebuild-backend
	OMNILAUNCHER_SERVER_HOST="$(SERVER_HOST)" OMNILAUNCHER_SERVER_PORT="$(SERVER_PORT)" $(BIN) start $(DEBUG_FLAG)
else
	$(error BACKEND_MODE must be local, wsl, or remote)
endif
else ifeq ($(ROLE),both)
ifeq ($(BACKEND_MODE),wsl)
	$(MAKE) start-wsl-backend-command
else ifeq ($(BACKEND_MODE),remote)
	$(info BACKEND_MODE=remote: not starting backend; using $(BACKEND_URL))
else ifeq ($(BACKEND_MODE),local)
	$(MAKE) maybe-rebuild-backend
	OMNILAUNCHER_SERVER_HOST="$(SERVER_HOST)" OMNILAUNCHER_SERVER_PORT="$(SERVER_PORT)" $(BIN) start $(DEBUG_FLAG)
else
	$(error BACKEND_MODE must be local, wsl, or remote)
endif
	$(MAKE) maybe-rebuild-frontend
	OMNILAUNCHER_BACKEND_URL="$(BACKEND_URL)" $(BIN) gui --detached $(DEBUG_FLAG)
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical restart ---------------------------------------------------------

restart:
	$(MAKE) stop ROLE=$(ROLE)
ifeq ($(BACKEND_MODE),wsl)
ifeq ($(ROLE),backend)
	$(MAKE) restart-wsl-backend-command
else ifeq ($(ROLE),both)
	$(MAKE) restart-wsl-backend-command
	rm -f $(BIN)
	$(MAKE) build-frontend-command
	OMNILAUNCHER_BACKEND_URL="$(BACKEND_URL)" $(BIN) gui --detached $(DEBUG_FLAG)
else
	rm -f $(BIN)
	$(MAKE) build-frontend-command
	OMNILAUNCHER_BACKEND_URL="$(BACKEND_URL)" $(BIN) gui --detached $(DEBUG_FLAG)
endif
else
	rm -f $(BIN)
	$(MAKE) build ROLE=$(ROLE)
	$(MAKE) start ROLE=$(ROLE) BACKEND_MODE=$(BACKEND_MODE) REBUILD=0 DEBUG=$(DEBUG) VERBOSE=$(VERBOSE)
endif

# -- Canonical clean -----------------------------------------------------------

clean:
ifeq ($(ROLE),frontend)
	rm -rf dist
else ifeq ($(ROLE),backend)
	rm -rf src-tauri/target
else ifeq ($(ROLE),both)
	rm -rf dist src-tauri/target
else
	$(error ROLE must be frontend, backend, or both)
endif

# -- Canonical test ------------------------------------------------------------

test:
ifeq ($(KIND),frontend)
	$(NPM) test
else ifeq ($(KIND),rust)
	cd src-tauri && $(CARGO) test $(CARGO_TEST_FLAGS)
else ifeq ($(KIND),unit)
	$(MAKE) test KIND=frontend
	$(MAKE) test KIND=rust
else ifeq ($(KIND),backend)
	$(BIN) health
else ifeq ($(KIND),health)
	$(BIN) health
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
	$(BIN) logs -f

status:
	$(BIN) status

remove-binary:
	rm -f $(BIN)

# Symlink the single binary onto PATH as `ol` so the CLI/REPL is reachable from
# any directory. Idempotent; warns if ~/.local/bin isn't on PATH.
install-cli:
	@mkdir -p "$(HOME)/.local/bin"
	@ln -sf "$(CURDIR)/src-tauri/target/release/omnilauncher" "$(HOME)/.local/bin/ol"
	@echo "linked $(HOME)/.local/bin/ol -> src-tauri/target/release/omnilauncher"
	@case ":$$PATH:" in *":$(HOME)/.local/bin:"*) ;; *) echo "warning: $(HOME)/.local/bin is not on your PATH — add it to use \`ol\`";; esac

uninstall-cli:
	@rm -f "$(HOME)/.local/bin/ol"
	@echo "removed $(HOME)/.local/bin/ol"

# -- WSL split-machine backend (Windows-only) ---------------------------------
#
# Build + run the single binary inside WSL in `serve` mode, with the Windows
# desktop shell connecting over BACKEND_URL. Windows-only because it drives
# wsl.exe; on Linux/macOS just use `make start-backend`. Previously lived in
# scripts/ops.ps1.

WSL_REPO ?= /mnt/c/Users/jzhu/repos/OmniLauncher
WSL_BIN  := $(WSL_REPO)/src-tauri/target/release/omnilauncher

start-wsl-backend-command:
ifeq ($(PLATFORM),windows)
	@echo "Building backend inside WSL..."
	wsl -e bash -c 'cd $(WSL_REPO)/src-tauri && cargo build --release'
	@echo "Starting backend inside WSL on $(SERVER_HOST):$(SERVER_PORT)..."
	wsl -e bash -c "OMNILAUNCHER_SERVER_HOST=$(SERVER_HOST) OMNILAUNCHER_SERVER_PORT=$(SERVER_PORT) nohup $(WSL_BIN) serve >/dev/null 2>&1 &"
	@echo "WSL backend started. Frontend connects via BACKEND_URL=$(BACKEND_URL)"
else
	$(error BACKEND_MODE=wsl is only supported on Windows)
endif

restart-wsl-backend-command:
ifeq ($(PLATFORM),windows)
	$(BIN) stop
	wsl -e bash -c 'rm -f $(WSL_BIN)'
	$(MAKE) start-wsl-backend-command
else
	$(error BACKEND_MODE=wsl is only supported on Windows)
endif
