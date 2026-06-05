.PHONY: help dev dev-debug prod prod-debug restart restart-rebuild build build-frontend install install-deps clean lint format check test release bundle status logs \
        backend-dev backend-prod backend-build stop-backend \
        frontend-dev frontend-dev-debug frontend-prod frontend-prod-debug frontend-build stop-frontend \
        browser-dev frontend-prod-web serve-frontend-prod split-check stop stop-running stop-dev-server stop-split-backend

SHELL := /usr/bin/env bash

ifeq ($(OS),Windows_NT)
  PLATFORM := windows
else
  PLATFORM := unix
endif

NPM ?= npm
NPX ?= npx
CARGO ?= cargo
TAURI_DEV ?= $(NPX) tauri dev
TAURI_BUILD ?= $(NPX) tauri build
SPLIT_HOST ?= 0.0.0.0
SPLIT_PORT ?= 1422
FRONTEND_BACKEND_URL ?= http://127.0.0.1:$(SPLIT_PORT)
FRONTEND_SERVE_PORT ?= 4173


help:
	$(info OmniLauncher - Makefile targets:)
	$(info )
	$(info   backend-dev       Start backend API server in dev/debug mode)
	$(info   backend-prod      Start backend API server in release mode)
	$(info   backend-build     Build backend release binary)
	$(info   stop-backend      Stop process listening on backend port $(SPLIT_PORT))
	$(info )
	$(info   frontend-dev      Start desktop frontend app connected to $(FRONTEND_BACKEND_URL))
	$(info   frontend-dev-debug Start desktop frontend app with verbose file logging (--debug))
	$(info   frontend-prod     Build and start release desktop frontend app connected to $(FRONTEND_BACKEND_URL))
	$(info   frontend-prod-debug Build and start release desktop frontend app with --debug logging)
	$(info   frontend-build    Build desktop frontend app release binary)
	$(info   stop-frontend     Stop desktop frontend app process)
	$(info )
	$(info   browser-dev       Run browser-only frontend against the backend (NOT the desktop app))
	$(info   frontend-prod-web Build web frontend for split production deployment)
	$(info   serve-frontend-prod Serve built split web frontend locally)
	$(info   split-check       Verify frontend build + Rust checks for split workflow)
	$(info )
	$(info   dev              Alias for frontend-dev)
	$(info   dev-debug        Alias for frontend-dev-debug)
	$(info   prod             Alias for frontend-prod)
	$(info   prod-debug       Alias for frontend-prod-debug)
	$(info   build            Alias for frontend-build)
	$(info   release          Alias for frontend-build)
	$(info   bundle           Create installer packages (MSI, NSIS, etc.))
	$(info   restart          Restart production desktop frontend app (use REBUILD=1 to rebuild))
	$(info   restart-rebuild  Rebuild release binary and restart production desktop frontend app)
	$(info   stop             Stop frontend app and backend)
	$(info )
	$(info   install          Install frontend + Rust dependencies)
	$(info   install-deps     Alias for install)
	$(info   clean            Remove build artifacts)
	$(info   lint             Run Clippy (Rust linter))
	$(info   format           Format Rust + frontend code)
	$(info   check            Run TypeScript + Rust type checks)
	$(info   test             Run all tests)
	$(info   status           Show app status (running, dev server, build))
	$(info   logs             Tail the debug log file live)
	@:

# ── Desktop frontend app ─────────────────────────────────────────────────────

frontend-dev: stop-frontend stop-dev-server
	@echo "Desktop frontend app - connects to backend at $(FRONTEND_BACKEND_URL)."
ifeq ($(PLATFORM),windows)
	set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(TAURI_DEV)
else
	OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(TAURI_DEV)
endif

frontend-dev-debug: stop-frontend stop-dev-server
	@echo "Desktop frontend app (debug) - connects to backend at $(FRONTEND_BACKEND_URL)."
ifeq ($(PLATFORM),windows)
	set "TAURI_DEBUG=1" && set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(TAURI_DEV) -- -- --debug
else
	TAURI_DEBUG=1 OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(TAURI_DEV) -- -- --debug
endif

frontend-build: stop-frontend build-frontend
	$(TAURI_BUILD) --no-bundle

frontend-prod: stop-frontend frontend-build
ifeq ($(PLATFORM),windows)
	@if exist src-tauri\target\release\omnilauncher.exe (set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' -WorkingDirectory (Get-Location)") else (echo Release binary not found. Run make frontend-build first. && exit /b 1)
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) nohup src-tauri/target/release/omnilauncher >/dev/null 2>&1 & else echo 'Release binary not found. Run make frontend-build first.'; exit 1; fi
endif

frontend-prod-debug: stop-frontend frontend-build
ifeq ($(PLATFORM),windows)
	@if exist src-tauri\target\release\omnilauncher.exe (set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' -ArgumentList '--debug' -WorkingDirectory (Get-Location)") else (echo Release binary not found. Run make frontend-build first. && exit /b 1)
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) nohup src-tauri/target/release/omnilauncher --debug >/dev/null 2>&1 & else echo 'Release binary not found. Run make frontend-build first.'; exit 1; fi
endif

stop-frontend:
ifeq ($(PLATFORM),windows)
	-powershell -NoProfile -Command "Get-Process omnilauncher -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue"
else
	-pkill -x omnilauncher >/dev/null 2>&1 || true
endif

# Backwards-compatible desktop frontend aliases.
dev: frontend-dev

dev-debug: frontend-dev-debug

prod: frontend-prod

prod-debug: frontend-prod-debug

build: frontend-build

release: frontend-build

restart: stop-frontend
ifeq ($(REBUILD),1)
	$(MAKE) frontend-prod
else
ifeq ($(PLATFORM),windows)
	@if exist src-tauri\target\release\omnilauncher.exe (set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' -WorkingDirectory (Get-Location)") else (echo Release binary not found. Run make frontend-prod or make restart REBUILD=1. && exit /b 1)
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) nohup src-tauri/target/release/omnilauncher >/dev/null 2>&1 & else echo 'Release binary not found. Run make frontend-prod or make restart REBUILD=1.'; exit 1; fi
endif
endif

restart-rebuild:
	$(MAKE) restart REBUILD=1

# ── Backend API server ────────────────────────────────────────────────────────

backend-dev:
ifeq ($(PLATFORM),windows)
	set "OMNILAUNCHER_SPLIT_HOST=$(SPLIT_HOST)" && set "OMNILAUNCHER_SPLIT_PORT=$(SPLIT_PORT)" && $(CARGO) run --manifest-path src-tauri/Cargo.toml -- --split-backend
else
	OMNILAUNCHER_SPLIT_HOST=$(SPLIT_HOST) OMNILAUNCHER_SPLIT_PORT=$(SPLIT_PORT) $(CARGO) run --manifest-path src-tauri/Cargo.toml -- --split-backend
endif

backend-prod:
ifeq ($(PLATFORM),windows)
	set "OMNILAUNCHER_SPLIT_HOST=$(SPLIT_HOST)" && set "OMNILAUNCHER_SPLIT_PORT=$(SPLIT_PORT)" && $(CARGO) run --release --manifest-path src-tauri/Cargo.toml -- --split-backend
else
	OMNILAUNCHER_SPLIT_HOST=$(SPLIT_HOST) OMNILAUNCHER_SPLIT_PORT=$(SPLIT_PORT) $(CARGO) run --release --manifest-path src-tauri/Cargo.toml -- --split-backend
endif

backend-build:
	cd src-tauri && $(CARGO) build --release

stop-backend:
ifeq ($(PLATFORM),windows)
	-powershell -NoProfile -Command "Get-NetTCPConnection -LocalPort $(SPLIT_PORT) -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { Stop-Process -Id ($$_.OwningProcess) -Force -ErrorAction SilentlyContinue }"
else
	-lsof -ti:$(SPLIT_PORT) | xargs -r kill -9 >/dev/null 2>&1 || true
endif

# Backwards-compatible backend stop alias.
stop-split-backend: stop-backend

# ── Browser/web frontend ──────────────────────────────────────────────────────

browser-dev: stop-dev-server
ifeq ($(PLATFORM),windows)
	set "VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(NPM) run frontend:split
else
	VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(NPM) run frontend:split
endif

frontend-prod-web:
ifeq ($(PLATFORM),windows)
	set "VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(NPM) run build
else
	VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(NPM) run build
endif

serve-frontend-prod: frontend-prod-web
	$(NPX) vite preview --host 0.0.0.0 --port $(FRONTEND_SERVE_PORT)

split-check: build-frontend
	cd src-tauri && $(CARGO) check

# ── Shared maintenance ────────────────────────────────────────────────────────

build-frontend:
	$(NPM) run build

bundle: stop-frontend build-frontend
	$(TAURI_BUILD)

install: install-deps

install-deps:
	$(NPM) install
	cd src-tauri && $(CARGO) fetch

clean: stop-frontend
ifeq ($(PLATFORM),windows)
	-powershell -NoProfile -Command "Remove-Item -Recurse -Force dist,node_modules,src-tauri/target -ErrorAction SilentlyContinue"
else
	rm -rf dist node_modules src-tauri/target
endif

lint:
	cd src-tauri && $(CARGO) clippy -- -D warnings

format:
	$(NPX) prettier --write "src/**/*.{ts,tsx,css,json}"
	cd src-tauri && $(CARGO) fmt

check:
	$(NPX) tsc --noEmit
	cd src-tauri && $(CARGO) check

test: stop-frontend
	cd src-tauri && $(CARGO) test -- --test-threads=1

stop: stop-frontend stop-backend

# Backwards-compatible frontend stop alias.
stop-running: stop-frontend

stop-dev-server:
ifeq ($(PLATFORM),windows)
	-powershell -NoProfile -Command "Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { Stop-Process -Id ($$_.OwningProcess) -Force -ErrorAction SilentlyContinue }"
else
	-lsof -ti:1420 | xargs -r kill -9 >/dev/null 2>&1 || true
endif

status:
ifeq ($(PLATFORM),windows)
	@pwsh -NoProfile -File scripts/status.ps1
else
	@bash scripts/status.sh
endif

logs:
ifeq ($(PLATFORM),windows)
	@pwsh -NoProfile -File scripts/logs.ps1
else
	@bash scripts/logs.sh
endif
