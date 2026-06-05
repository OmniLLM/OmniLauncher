.PHONY: help dev dev-debug prod prod-debug restart restart-rebuild build build-frontend install install-deps clean lint format check test release bundle stop stop-running stop-dev-server status logs backend-dev browser-dev split-check backend-prod frontend-prod serve-frontend-prod stop-split-backend

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
	$(info   dev              Start the DESKTOP UI shell (Tauri); needs a backend (run 'make backend-dev'))
	$(info   dev-debug        Start the desktop UI shell with verbose file logging (--debug))
	$(info   backend-dev      Run the separated backend API server (hosts all logic/plugins/skills))
	$(info   backend-prod     Run the separated backend API server in release mode)
	$(info   browser-dev      Run the browser-only frontend against the backend (NOT the desktop app))
	$(info   frontend-prod    Build frontend for split production deployment)
	$(info   serve-frontend-prod Serve built split frontend locally)
	$(info   split-check      Verify frontend build + Rust checks for split workflow)
	$(info   prod             Build and start app in production mode (release))
	$(info   prod-debug       Build and start app in production mode with --debug logging)
	$(info   restart          Restart production app (use REBUILD=1 to rebuild))
	$(info   restart-rebuild  Rebuild release binary and restart production app)
	$(info   build            Build frontend + Tauri (debug))
	$(info   build-frontend   Build frontend only (Vite))
	$(info   release          Build release binary with optimizations)
	$(info   bundle           Create installer packages (MSI, NSIS, etc.))
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

dev: stop-running stop-dev-server
	@echo "Desktop UI shell - expects a backend at $(FRONTEND_BACKEND_URL) (start it with 'make backend-dev')."
ifeq ($(PLATFORM),windows)
	set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(TAURI_DEV)
else
	OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(TAURI_DEV)
endif

dev-debug: stop-running stop-dev-server
ifeq ($(PLATFORM),windows)
	set "TAURI_DEBUG=1" && set "OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(TAURI_DEV) -- -- --debug
else
	TAURI_DEBUG=1 OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(TAURI_DEV) -- -- --debug
endif

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

browser-dev: stop-dev-server
ifeq ($(PLATFORM),windows)
	set "VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(NPM) run frontend:split
else
	VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(NPM) run frontend:split
endif

frontend-prod:
ifeq ($(PLATFORM),windows)
	set "VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL)" && $(NPM) run build
else
	VITE_OMNILAUNCHER_BACKEND_URL=$(FRONTEND_BACKEND_URL) $(NPM) run build
endif

serve-frontend-prod: frontend-prod
	$(NPX) vite preview --host 0.0.0.0 --port $(FRONTEND_SERVE_PORT)

split-check: build-frontend
	cd src-tauri && $(CARGO) check

prod: stop-running release
ifeq ($(PLATFORM),windows)
	@if exist src-tauri\target\release\omnilauncher.exe (powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe'") else (echo Release binary not found. Run make release first. && exit /b 1)
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then nohup src-tauri/target/release/omnilauncher >/dev/null 2>&1 &; else echo 'Release binary not found. Run make release first.'; exit 1; fi
endif

prod-debug: stop-running release
ifeq ($(PLATFORM),windows)
	@if exist src-tauri\target\release\omnilauncher.exe (powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' -ArgumentList '--debug'") else (echo Release binary not found. Run make release first. && exit /b 1)
else
	@if [ -f src-tauri/target/release/omnilauncher ]; then nohup src-tauri/target/release/omnilauncher --debug >/dev/null 2>&1 &; else echo 'Release binary not found. Run make release first.'; exit 1; fi
endif

restart: stop-running
ifeq ($(REBUILD),1)
	$(MAKE) prod
else
ifeq ($(PLATFORM),windows)
	@if exist src-tauri\target\release\omnilauncher.exe (powershell -NoProfile -Command "Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe'") else (echo Release binary not found. Run make prod or make restart REBUILD=1. && exit /b 1)
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
