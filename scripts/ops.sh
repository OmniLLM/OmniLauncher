#!/usr/bin/env bash
# Helper for Makefile targets - start/stop/test the server and frontend.
# Linux/macOS counterpart of scripts/ops.ps1.
#
# Called from the Makefile as:
#   scripts/ops.sh <action> [--ServerHost host] [--ServerPort port] [--BackendUrl url]
#
# We accept the same flag names as ops.ps1 so the Makefile can stay symmetric.

set -uo pipefail

# -- Colors (only when stdout is a tty) ---------------------------------------
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    CYAN='\033[0;36m'
    NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; CYAN=''; NC=''
fi

info()  { printf "${CYAN}%b${NC}\n" "$*"; }
ok()    { printf "${GREEN}%b${NC}\n" "$*"; }
warn()  { printf "%b\n" "${YELLOW}$*${NC}"; }
err()   { printf "${RED}%b${NC}\n" "$*"; }

# -- Arg parsing --------------------------------------------------------------
ACTION="${1:-}"
shift || true

SERVER_HOST="0.0.0.0"
SERVER_PORT="1422"
BACKEND_URL="http://127.0.0.1:1422"
DEBUG_FLAG=""
POSITIONALS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --ServerHost)   SERVER_HOST="${2:-}"; shift 2 ;;
        --ServerPort)   SERVER_PORT="${2:-}"; shift 2 ;;
        --BackendUrl)  BACKEND_URL="${2:-}"; shift 2 ;;
        --debug)       DEBUG_FLAG="--debug"; shift ;;
        --*)           shift ;;  # ignore other unknown flags
        *)             POSITIONALS+=("$1"); shift ;;
    esac
done
# Rebuild $@ from collected positionals so the dispatch can see role args.
# The `+...` guards against bash <4.4 treating an empty array as unbound under set -u.
set -- "${POSITIONALS[@]+"${POSITIONALS[@]}"}"

if [ -z "$ACTION" ]; then
    err "ops.sh: missing action"
    exit 2
fi

# -- Paths --------------------------------------------------------------------
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd -- "$SCRIPT_DIR/.." && pwd)"
BIN_DIR="$REPO_DIR/src-tauri/target/release"
# Single self-dispatching binary. The historical role copies
# (omnilauncher-frontend / omnilauncher-backend) are gone — the binary now owns
# all lifecycle commands and decides its mode from the subcommand (serve / gui /
# start / stop / ...). PID files live under ~/.omnilauncher/run/ (owned by the
# binary), NOT the repo-local .run/ anymore.
BASE_EXE="$BIN_DIR/omnilauncher"
GUI_PID_FILE="$HOME/.omnilauncher/run/omnilauncher-gui.pid"

# -- Helpers ------------------------------------------------------------------
# Ensure the single release binary exists before delegating a lifecycle command
# to it.
ensure_binary() {
    if [ ! -x "$BASE_EXE" ]; then
        err "Release binary not found at $BASE_EXE"
        err "Run: make build"
        exit 1
    fi
}

# Backend lifecycle now delegates entirely to the self-contained binary, which
# spawns a detached `serve`, tracks its PID under ~/.omnilauncher/run/, and waits
# for /health. Host/port come from the environment the binary inherits.
start_backend() {
    ensure_binary
    export OMNILAUNCHER_SERVER_HOST="$SERVER_HOST"
    export OMNILAUNCHER_SERVER_PORT="$SERVER_PORT"
    "$BASE_EXE" start $DEBUG_FLAG
}

stop_backend() {
    ensure_binary
    "$BASE_EXE" stop || true
}

# GUI (desktop shell) lifecycle. `gui --detached` backgrounds the shell and
# writes ~/.omnilauncher/run/omnilauncher-gui.pid; we stop it via that file.
start_frontend() {
    ensure_binary
    export OMNILAUNCHER_BACKEND_URL="$BACKEND_URL"
    "$BASE_EXE" gui --detached $DEBUG_FLAG
}

stop_frontend() {
    if [ -f "$GUI_PID_FILE" ]; then
        local pid
        pid="$(cat "$GUI_PID_FILE" 2>/dev/null || true)"
        if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            sleep 0.2
            kill -9 "$pid" 2>/dev/null || true
            ok "Stopped desktop shell (pid=$pid)"
        fi
        rm -f "$GUI_PID_FILE" 2>/dev/null || true
    fi
}

start_prod_debug_backend() {
    ensure_binary
    export OMNILAUNCHER_SERVER_HOST="$SERVER_HOST"
    export OMNILAUNCHER_SERVER_PORT="$SERVER_PORT"
    "$BASE_EXE" start --debug
}

start_prod_debug_frontend() {
    ensure_binary
    export OMNILAUNCHER_BACKEND_URL="$BACKEND_URL"
    "$BASE_EXE" gui --detached --debug
}

start_wsl_backend_unsupported() {
    err "start-wsl-backend / restart-wsl-backend is a Windows-only target"
    err "(it launches the backend from Windows into WSL via the wsl.exe shim)."
    err "On Linux, just run: make start-backend"
    exit 1
}

test_backend() {
    info "Checking backend health at $BACKEND_URL/health ..."
    if ! command -v curl >/dev/null 2>&1; then
        err "curl is required for test-backend on Linux/macOS"
        exit 1
    fi
    local body
    if body="$(curl -fsS --max-time 5 "$BACKEND_URL/health")"; then
        ok "Backend is running:"
        echo "$body"
    else
        err "Backend is NOT responding at $BACKEND_URL"
        exit 1
    fi
}

# `make status` / `make logs` delegate to the binary's own rich implementations.
# `status` is informational here, so we don't propagate its exit code (the
# binary exits non-zero when no managed backend is running, which shouldn't fail
# `make status`).
show_status() {
    ensure_binary
    "$BASE_EXE" status || true
}

# Remove the built binary (used by REBUILD=1 flows before a fresh build).
remove_binaries() {
    rm -f "$BASE_EXE" 2>/dev/null || true
    ok "Removed $BASE_EXE"
}

clean_frontend() {
    rm -rf "$REPO_DIR/dist" 2>/dev/null || true
    ok "Removed $REPO_DIR/dist"
}

clean_backend() {
    rm -rf "$REPO_DIR/src-tauri/target" 2>/dev/null || true
    ok "Removed $REPO_DIR/src-tauri/target"
}

clean_all() {
    clean_frontend
    clean_backend
}

# -- Dispatch -----------------------------------------------------------------
case "$ACTION" in
    stop-frontend)        stop_frontend ;;
    stop-backend)         stop_backend ;;
    stop-all)             stop_frontend; stop_backend ;;
    start-frontend)       start_frontend ;;
    start-backend)        start_backend ;;
    prod-debug-backend)   start_prod_debug_backend ;;
    prod-debug-frontend)  start_prod_debug_frontend ;;
    prod-debug)           start_prod_debug_backend; start_prod_debug_frontend ;;
    start-wsl-backend|restart-wsl-backend) start_wsl_backend_unsupported ;;
    test-backend)         test_backend ;;
    status)               show_status ;;
    remove-binary)        remove_binaries ;;
    clean-frontend)       clean_frontend ;;
    clean-backend)        clean_backend ;;
    clean)                clean_all ;;
    *)
        err "ops.sh: unknown action '$ACTION'"
        echo "valid actions: stop-frontend stop-backend stop-all start-frontend start-backend"
        echo "               prod-debug-backend prod-debug-frontend prod-debug"
        echo "               test-backend status remove-binary"
        echo "               clean-frontend clean-backend clean"
        exit 2
        ;;
esac
