.PHONY: help help-advanced \
        build build-frontend build-backend build-frontend-command build-backend-command \
        stop stop-frontend stop-backend stop-all \
        start start-frontend start-backend start-wsl-backend \
        start-frontend-command start-backend-command \
        restart restart-frontend restart-backend restart-wsl-backend \
        start-wsl-backend-command restart-wsl-backend-command \
        clean clean-frontend clean-backend \
        remove-binary install-cli uninstall-cli \
        logs status \
        prod-debug prod-debug-backend prod-debug-frontend \
        test test-frontend test-rust test-backend test-health test-smoke test-e2e test-unit test-all \
        maybe-rebuild-frontend maybe-rebuild-backend

include make/config.mk
include make/platform.mk
include make/help.mk
include make/commands.mk
include make/aliases.mk
