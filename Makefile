.PHONY: help dev build build-frontend install install-deps clean lint format check test release bundle

SHELL := pwsh

help:
	@echo OmniLauncher - Makefile targets:
	@powershell -Command "Write-Host ''"
	@echo   dev              Start dev server with hot reload (Vite + Tauri)
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

dev:
	npm run tauri dev

build: build-frontend
	cd src-tauri && cargo build

build-frontend:
	npm run build

release: build-frontend
	cd src-tauri && cargo build --release

bundle: build-frontend
	npx tauri build

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
	npx prettier --write "src/**/*.{ts,tsx,css,json}" 2>$null
	cd src-tauri && cargo fmt

check:
	npx tsc --noEmit
	cd src-tauri && cargo check

test:
	cd src-tauri && cargo test
