# -- Platform-specific helper invocations -------------------------------------

ifeq ($(OS),Windows_NT)
  PLATFORM := windows
else
  PLATFORM := unix
endif

ifeq ($(PLATFORM),windows)
  OPS        = powershell -NoProfile -File scripts/ops.ps1
  LOGS_CMD   = pwsh -NoProfile -File scripts/logs.ps1
  SMOKE_CMD  = pwsh -NoProfile -File scripts/smoke-endpoints.ps1
  E2E_CMD    = pwsh -NoProfile -File scripts/test-e2e.ps1
  OPS_ROLE_FLAG       = -Role
  OPS_DEBUG_FLAG_NAME = -DebugFlag
else
  OPS        = bash scripts/ops.sh
  LOGS_CMD   = bash scripts/logs.sh
  SMOKE_CMD  = bash scripts/smoke-endpoints.sh
  E2E_CMD    = bash scripts/test-e2e.sh
  OPS_ROLE_FLAG       =
  OPS_DEBUG_FLAG_NAME =
endif

ifeq ($(DEBUG),1)
  DEBUG_FLAG := $(if $(OPS_DEBUG_FLAG_NAME),$(OPS_DEBUG_FLAG_NAME),--debug)
else ifeq ($(VERBOSE),1)
  DEBUG_FLAG := $(if $(OPS_DEBUG_FLAG_NAME),$(OPS_DEBUG_FLAG_NAME),--debug)
else
  DEBUG_FLAG :=
endif
