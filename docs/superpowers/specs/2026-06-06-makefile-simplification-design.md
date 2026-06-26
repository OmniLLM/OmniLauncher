# Makefile Simplification Design

## Goal

Simplify the Makefile so the common interface is easy to discover and maintain while preserving existing workflows. The top-level Makefile should read like a small command interface, not like a long operations script.

## Current Pain

The current Makefile mixes several concerns in one file:

- user-facing help text
- default configuration variables
- Windows vs Unix helper command selection
- build/start/stop/restart/test orchestration
- WSL backend support
- debug aliases
- backward-compatible role-specific targets

This makes the file hard to scan and increases duplication, especially around rebuild, prepare-binaries, and role-specific start/restart flows.

## User-Facing Interface

The main help output should emphasize a compact set of commands:

```sh
make help
make build ROLE=frontend|backend|both
make start ROLE=frontend|backend|both BACKEND_MODE=local|wsl|remote DEBUG=1
make stop ROLE=frontend|backend|both
make restart ROLE=frontend|backend|both DEBUG=1
make test KIND=unit|frontend|rust|backend|smoke|e2e|all
make clean ROLE=frontend|backend|both
make status
make logs
```

Defaults:

- `ROLE=both` for build/start/stop/restart/clean unless a command has a safer existing default.
- `KIND=all` for `make test`.
- `BACKEND_MODE=local`.
- `DEBUG=0` and `VERBOSE=0`, with both still accepted as aliases for the binary `--debug` flag.

## Compatibility

Existing common targets must continue to work as aliases so scripts and muscle memory do not break:

```sh
make build-frontend
make build-backend
make start-frontend
make start-backend
make stop-frontend
make stop-backend
make stop-all
make restart-frontend
make restart-backend
make test-frontend
make test-rust
make test-backend
make test-health
make test-smoke
make test-e2e
make test-unit
make test-all
make clean-frontend
make clean-backend
make remove-binary
make prepare-binaries
make start-wsl-backend
make restart-wsl-backend
make prod-debug
make prod-debug-backend
make prod-debug-frontend
```

These aliases should move out of the primary help text. A separate `make help-advanced` target should list compatibility aliases and variables.

## File Structure

Split the Make logic into focused include files:

```text
Makefile
make/
  config.mk      # variables, defaults, normalized DEBUG flag
  platform.mk    # Windows vs Unix helper command selection
  help.mk        # help and help-advanced output
  commands.mk    # canonical variable-driven commands
  aliases.mk     # compatibility aliases for old targets
```

The top-level `Makefile` should mainly define `.PHONY` and include these files.

## Responsibility Boundaries

- Makefiles orchestrate high-level workflows.
- `scripts/ops.sh` and `scripts/ops.ps1` remain responsible for process management and platform-specific operational details.
- Existing script flags and environment variables remain compatible.
- New Makefile logic should avoid duplicating platform-specific shell behavior.

## Workflow Design

Canonical commands dispatch by variables:

- `make build ROLE=frontend|backend|both`
- `make start ROLE=frontend|backend|both`
- `make stop ROLE=frontend|backend|both`
- `make restart ROLE=frontend|backend|both`
- `make clean ROLE=frontend|backend|both`
- `make test KIND=frontend|rust|unit|backend|health|smoke|e2e|all`

Aliases should call these canonical commands through `$(MAKE)` where practical. This keeps old target names stable while reducing duplicated command bodies.

## WSL and Remote Backend Behavior

WSL backend support remains available through both:

```sh
make start BACKEND_MODE=wsl
make restart BACKEND_MODE=wsl
```

and compatibility aliases:

```sh
make start-wsl-backend
make restart-wsl-backend
```

`BACKEND_MODE=remote` should not start a backend. It should allow frontend-only workflows to connect to `BACKEND_URL`. If a full `ROLE=both` start is requested with `BACKEND_MODE=remote`, the backend start step should be skipped and the frontend should start against `BACKEND_URL`.

## Debug Behavior

Existing `DEBUG=1` and `VERBOSE=1` behavior stays intact. Both produce the platform-appropriate debug flag for `ops` helpers.

Compatibility debug targets remain:

```sh
make prod-debug
make prod-debug-backend
make prod-debug-frontend
```

They should delegate to the canonical start flow with `DEBUG=1` where that preserves current behavior.

## Testing and Verification

Verification should include:

1. `make help`
2. `make help-advanced`
3. `make test KIND=frontend`
4. `make test KIND=rust`
5. `make test KIND=unit`
6. Compatibility checks for representative aliases with dry-run output where running would start long-lived processes:
   - `make -n start-backend`
   - `make -n restart-frontend`
   - `make -n test-smoke`

If project tests are too slow or require unavailable services, report exactly what was run and why anything was skipped. Do not claim full verification unless the relevant commands passed.
