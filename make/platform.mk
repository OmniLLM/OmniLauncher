# -- Platform-specific configuration ------------------------------------------
#
# All lifecycle / ops now run through the single self-contained binary
# (serve / gui / start / stop / status / logs / doctor). There is no shell or
# PowerShell wrapper anymore — the binary owns process control, port probing,
# PID files, and health checks natively on every OS. Only the HTTP endpoint
# test drivers (smoke / e2e) remain as scripts.

ifeq ($(OS),Windows_NT)
  PLATFORM := windows
else
  PLATFORM := unix
endif

ifeq ($(PLATFORM),windows)
  # On Windows, GNU Make sets $(MAKE) to its own invocation path, e.g.
  # 'C:/Program Files (x86)/GnuWin32/bin/make'. Recursive recipe lines are
  # handed to Git Bash's sh, where the spaces and '(x86)' parens trigger
  # "syntax error near unexpected token `('"; under `make -n` the same path
  # breaks Make's internal CreateProcess (e=87). Quoting fixes sh but not the
  # -n path. Overriding to a bare, PATH-resolved `make` fixes both — and it's
  # how the user invokes make anyway.
  MAKE       := make
  BIN        := src-tauri/target/release/omnilauncher.exe
  SMOKE_CMD  = pwsh -NoProfile -File scripts/smoke-endpoints.ps1
  E2E_CMD    = pwsh -NoProfile -File scripts/test-e2e.ps1
else
  BIN        := src-tauri/target/release/omnilauncher
  SMOKE_CMD  = bash scripts/smoke-endpoints.sh
  E2E_CMD    = bash scripts/test-e2e.sh
endif

# Start binaries with file logging when DEBUG=1 or VERBOSE=1. The binary accepts
# --debug uniformly on every platform (no more per-wrapper flag-name shim).
ifeq ($(DEBUG),1)
  DEBUG_FLAG := --debug
else ifeq ($(VERBOSE),1)
  DEBUG_FLAG := --debug
else
  DEBUG_FLAG :=
endif
