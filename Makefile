.PHONY: help help-advanced \
        build build-frontend build-backend build-frontend-command build-backend-command \
        stop stop-frontend stop-backend stop-all \
        start start-frontend start-backend start-wsl-backend \
        restart restart-frontend restart-backend restart-wsl-backend \
        clean clean-frontend clean-backend \
        remove-binary prepare-binaries \
        logs status \
        prod-debug prod-debug-backend prod-debug-frontend \
        test test-frontend test-rust test-backend test-health test-smoke test-e2e test-unit test-all \
        maybe-rebuild-frontend maybe-rebuild-backend

include make/config.mk
include make/platform.mk
include make/help.mk
include make/commands.mk
include make/aliases.mk
