.PHONY: help dev dev-debug prod prod-debug restart restart-rebuild build build-frontend install install-deps clean lint format check test release bundle stop stop-running stop-dev-server status logs backend-dev frontend-dev split-check backend-prod frontend-prod serve-frontend-prod stop-split-backend

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
	@printf '%s\n' 'OmniLauncher - Makefile targets:'
	@printf '%s\n' ''
	@printf '%s\n' '  dev              Start integrated dev server with hot reload (Vite + Tauri)'
	@printf '%s\n' '  dev-debug        Start integrated dev server with verbose file logging (--debug)'
	@printf '%s\n' '  backend-dev      Run split backend API server only (for WSL/Linux)'
	@printf '%s\n' '  backend-prod     Run split backend API server in release mode'
	@printf '%s\n' '  frontend-dev     Run frontend only, targeting split backend via HTTP'
	@printf '%s\n' '  frontend-prod    Build frontend for split production deployment'
	@printf '%s\n' '  serve-frontend-prod Serve built split frontend locally'
	@printf '%s\n' '  split-check      Verify frontend build + Rust checks for split workflow'
	@printf '%s\n' '  prod             Build and start app in production mode (release)'
	@printf '%s\n' '  prod-debug       Build and start app in production mode with --debug logging'
	@printf '%s\n' '  restart          Restart production app (use REBUILD=1 to rebuild)'
	@printf '%s\n' '  restart-rebuild  Rebuild release binary and restart production app'
	@printf '%s\n' '  build            Build frontend + Tauri (debug)'
	@printf '%s\n' '  build-frontend   Build frontend only (Vite)'
	@printf '%s\n' '  release          Build release binary with optimizations'
	@printf '%s\n' '  bundle           Create installer packages (MSI, NSIS, etc.)'
	@printf '%s\n' '  install          Install frontend + Rust dependencies'
	@printf '%s\n' '  install-deps     Alias for install'
	@printf '%s\n' '  clean            Remove build artifacts'
	@printf '%s\n' '  lint             Run Clippy (Rust linter)'
	@printf '%s\n' '  format           Format Rust + frontend code'
	@printf '%s\n' '  check            Run TypeScript + Rust type checks'
	@printf '%s\n' '  test             Run all tests'
	@printf '%s\n' '  status           Show app status (running, dev server, build)'
	@printf '%s\n' '  logs             Tail the debug log file live'

dev: stop-running stop-dev-server
	$(TAURI_DEV)

dev-debug: stop-running stop-dev-server
	TAURI_DEBUG=1 $(TAURI_DEV) -- -- --debug

backend-dev:
	OMNILAUNCHER_SPLIT_HOST=$(SPLIT_HOST) OMNILAUNCHER_SPLIT_PORT=$(SPLIT_PORT) $(CARGO) run --manifest-path src-tauri/Cargo.toml -- --split-backend

backend-prod:
	OMNILAUNCHER_SPLIT_HOST=$(SPLIT_HOST) OMNILAUNCHER_SPLIT_PORT=$(SPLIT_PORT) $(CARGO) run --release --manifest-path src-tauri/Cargo.toml -- --split-backend

frontend-dev:
	VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(NPM) run frontend:split

frontend-prod:
	VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(NPM) run build

serve-frontend-prod: frontend-prod
	$(NPX) vite preview --host 0.0.0.0 --port $(FRONTEND_SERVE_PORT)

split-check: build-frontend
	cd src-tauri && $(CARGO) check

prod: stop-running release
ifeq ($(PLATFORM),windows)
	@if [ -f src-tauri/target/release/omnilauncher.exe ]; then powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe'"; else echo 'Release binary not found. Run make release first.'; exit 1; fi
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then nohup src-tauri/target/release/omnilauncher >/dev/null 2>&1 &; else echo 'Release binary not found. Run make release first.'; exit 1; fi
endif

prod-debug: stop-running release
ifeq ($(PLATFORM),windows)
	@if [ -f src-tauri/target/release/omnilauncher.exe ]; then powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' -ArgumentList '--debug'"; else echo 'Release binary not found. Run make release first.'; exit 1; fi
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then nohup src-tauri/target/release/omnilauncher --debug >/dev/null 2>&1 &; else echo 'Release binary not found. Run make release first.'; exit 1; fi
endif

restart: stop-running
ifeq ($(REBUILD),1)
	$(MAKE) prod
else
ifeq ($(PLATFORM),windows)
	@if [ -f src-tauri/target/release/omnilauncher.exe ]; then powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe'"; else echo 'Release binary not found. Run make prod or make restart REBUILD=1.'; exit 1; fi
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then nohup src-tauri/target/release/omnilauncher >/dev/null 2>&1 &; else echo 'Release binary not found. Run make prod or make restart REBUILD=1.'; exit 1; fi
endif
endif

restart-rebuild:
	$(MAKE) restart REBUILD=1

build: stop-running build-frontend
	cd src-tauri && $(CARGO) build

build-frontend:
	$(NPM) run build

release: stop-running build-frontend
	$(TAURI_BUILD) --no-bundle

bundle: stop-running build-frontend
	$(TAURI_BUILD)

install: install-deps

install-deps:
	$(NPM) install
	cd src-tauri && $(CARGO) fetch

clean: stop-running
	rm -rf dist node_modules src-tauri/target

lint:
	cd src-tauri && $(CARGO) clippy -- -D warnings

format:
	$(NPX) prettier --write "src/**/*.{ts,tsx,css,json}"
	cd src-tauri && $(CARGO) fmt

check:
	$(NPX) tsc --noEmit
	cd src-tauri && $(CARGO) check

test: stop-running
	cd src-tauri && $(CARGO) test -- --test-threads=1

stop: stop-running stop-split-backend

stop-split-backend:
ifeq ($(PLATFORM),windows)
	-powershell -NoProfile -Command "Get-NetTCPConnection -LocalPort $(SPLIT_PORT) -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { Stop-Process -Id ($$_.OwningProcess) -Force -ErrorAction SilentlyContinue }"
else
	-lsof -ti:$(SPLIT_PORT) | xargs -r kill -9 >/dev/null 2>&1 || true
endif

stop-running:
ifeq ($(PLATFORM),windows)
	-powershell -NoProfile -Command "Get-Process omnilauncher -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue"
else
	-pkill -x omnilauncher >/dev/null 2>&1 || true
endif

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
