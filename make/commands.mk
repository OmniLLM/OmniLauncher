# -- Internal command fragments ------------------------------------------------
#
# Single self-dispatching binary: `cargo build --release` produces one
# `omnilauncher` artifact that owns every runtime mode (GUI, serve, and the `ol`
# CLI). There is no longer a frontend/backend role copy step — the historical
# `prepare-binaries` (which duplicated the binary into role-named files) is gone.

build-frontend-command:
	$(NPM) run build
	$(NPX) tauri build --no-bundle

build-backend-command:
	cd src-tauri && $(CARGO) build --release

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
	$(OPS) start-wsl-backend --ServerHost "$(SERVER_HOST)" --ServerPort "$(SERVER_PORT)" --BackendUrl "$(BACKEND_URL)"
else ifeq ($(BACKEND_MODE),remote)
	$(info BACKEND_MODE=remote: not starting backend; using $(BACKEND_URL))
else ifeq ($(BACKEND_MODE),local)
	$(MAKE) maybe-rebuild-backend
	$(OPS) start-backend --ServerHost "$(SERVER_HOST)" --ServerPort "$(SERVER_PORT)" $(DEBUG_FLAG)
else
	$(error BACKEND_MODE must be local, wsl, or remote)
endif
else ifeq ($(ROLE),both)
ifeq ($(BACKEND_MODE),wsl)
	$(OPS) start-wsl-backend --ServerHost "$(SERVER_HOST)" --ServerPort "$(SERVER_PORT)" --BackendUrl "$(BACKEND_URL)"
else ifeq ($(BACKEND_MODE),remote)
	$(info BACKEND_MODE=remote: not starting backend; using $(BACKEND_URL))
else ifeq ($(BACKEND_MODE),local)
	$(MAKE) maybe-rebuild-backend
	$(OPS) start-backend --ServerHost "$(SERVER_HOST)" --ServerPort "$(SERVER_PORT)" $(DEBUG_FLAG)
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
	$(OPS) restart-wsl-backend --ServerHost "$(SERVER_HOST)" --ServerPort "$(SERVER_PORT)"
else ifeq ($(ROLE),both)
	$(OPS) restart-wsl-backend --ServerHost "$(SERVER_HOST)" --ServerPort "$(SERVER_PORT)"
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
	cd src-tauri && $(CARGO) test $(CARGO_TEST_FLAGS)
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
	$(OPS) status --BackendUrl "$(BACKEND_URL)" --ServerPort "$(SERVER_PORT)"

remove-binary:
	$(OPS) remove-binary

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
