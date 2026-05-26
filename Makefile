.PHONY: help dev dev-debug prod prod-debug restart restart-rebuild build build-frontend install install-deps clean lint format check test release bundle stop stop-running stop-dev-server

SHELL := pwsh

help:
	@echo OmniLauncher - Makefile targets:
	@powershell -Command "Write-Host ''"
	@echo   dev              Start dev server with hot reload (Vite + Tauri)
	@echo   dev-debug        Start dev server with verbose file logging (--debug)
	@echo   prod             Build and start app in production mode (release)
	@echo   prod-debug       Build and start app in production mode with --debug logging
	@echo   restart          Restart production app (use REBUILD=1 to rebuild)
	@echo   restart-rebuild  Rebuild release binary and restart production app
	@echo   build            Build frontend + Tauri (debug)
	@echo   build-frontend   Build frontend only (Vite)
	@echo   release          Build release binary with optimizations
	@echo   bundle           Create installer packages (MSI, NSIS, etc.)
	@echo   install          Install frontend + Rust dependencies
	@echo   install-deps     Alias for install
	@echo   clean            Remove build artifacts
	@echo   lint             Run Clippy (Rust linter)
	@echo   format           Format Rust + frontend code
	@echo   check            Run TypeScript + Rust type checks
	@echo   test             Run all tests

dev: stop-running stop-dev-server
	@cmd /c "set CARGO_TARGET_DIR=target\dev&& npm run tauri dev"

dev-debug: stop-running stop-dev-server
	@cmd /c "set CARGO_TARGET_DIR=target\dev&& npm run tauri dev -- --debug"

prod: stop-running release
	@pwsh -NoProfile -Command "if (Test-Path src-tauri/target/release/omnilauncher.exe) { Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' } else { Write-Error 'Release binary not found. Run make release first.'; exit 1 }"

prod-debug: stop-running release
	@pwsh -NoProfile -Command "if (Test-Path src-tauri/target/release/omnilauncher.exe) { Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' -ArgumentList '--debug' } else { Write-Error 'Release binary not found. Run make release first.'; exit 1 }"

restart: stop-running
ifeq ($(REBUILD),1)
	$(MAKE) prod
else
	@pwsh -NoProfile -Command "if (Test-Path src-tauri/target/release/omnilauncher.exe) { Start-Process -FilePath 'src-tauri/target/release/omnilauncher.exe' } else { Write-Error 'Release binary not found. Run make prod or make restart REBUILD=1.'; exit 1 }"
endif

restart-rebuild:
	$(MAKE) restart REBUILD=1

build: stop-running build-frontend
	cd src-tauri && cargo build

build-frontend:
	npm run build

release: stop-running build-frontend
	npx tauri build --no-bundle

bundle: stop-running build-frontend
	@cmd /c "set CARGO_TARGET_DIR=target\bundle&& npx tauri build"

install: install-deps

install-deps:
	npm install
	cd src-tauri && cargo fetch

clean:
	if (Test-Path dist) { Remove-Item -Recurse -Force dist }
	if (Test-Path node_modules) { Remove-Item -Recurse -Force node_modules }
	if (Test-Path src-tauri/target) { Remove-Item -Recurse -Force src-tauri/target }

lint:
	cd src-tauri && cargo clippy -- -D warnings

format:
	npx prettier --write "src/**/*.{ts,tsx,css,json}"
	cd src-tauri && cargo fmt

check:
	npx tsc --noEmit
	cd src-tauri && cargo check

test: stop-running
	@cmd /c "cd /d src-tauri && set CARGO_TARGET_DIR=target\test&& cargo test -- --test-threads=1"

stop: stop-running

stop-running:
	@cmd /c "taskkill /IM omnilauncher.exe /F /T >NUL 2>&1 & exit /b 0"

stop-dev-server:
	@powershell -NoProfile -Command 'Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object { Stop-Process -Id ($$_.OwningProcess) -Force -ErrorAction SilentlyContinue }; exit 0'
