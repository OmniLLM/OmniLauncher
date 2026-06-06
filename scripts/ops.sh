#!/usr/bin/env bash
# Helper for Makefile targets - start/stop/test the split backend and frontend.
# Linux/macOS counterpart of scripts/ops.ps1.
#
# Called from the Makefile as:
#   scripts/ops.sh <action> [--SplitHost host] [--SplitPort port] [--BackendUrl url]
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

SPLIT_HOST="0.0.0.0"
SPLIT_PORT="1422"
BACKEND_URL="http://127.0.0.1:1422"
DEBUG_FLAG=""
POSITIONALS=()

while [ $# -gt 0 ]; do
    case "$1" in
        --SplitHost)   SPLIT_HOST="${2:-}"; shift 2 ;;
        --SplitPort)   SPLIT_PORT="${2:-}"; shift 2 ;;
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
BASE_EXE="$BIN_DIR/omnilauncher"
FRONTEND_EXE="$BIN_DIR/omnilauncher-frontend"
BACKEND_EXE="$BIN_DIR/omnilauncher-backend"
RUN_DIR="$REPO_DIR/.run"
mkdir -p "$RUN_DIR" 2>/dev/null || true
FRONTEND_PID_FILE="$RUN_DIR/omnilauncher-frontend.pid"
BACKEND_PID_FILE="$RUN_DIR/omnilauncher-backend.pid"

# -- Helpers ------------------------------------------------------------------
# Copy the freshly-built omnilauncher binary into role-named file(s) and
# remove the generic source so we don't ship three identical files.
#   prepare_binaries frontend  -> only omnilauncher-frontend remains
#   prepare_binaries backend   -> only omnilauncher-backend remains
#   prepare_binaries both      -> both role files exist; bare omnilauncher gone
# Idempotent: if the bare omnilauncher is missing but the requested role
# file already exists, succeed silently.
prepare_binaries() {
    local role="${1:-both}"
    case "$role" in
        frontend|backend|both) ;;
        *)
            err "prepare_binaries: unknown role '$role' (expected frontend|backend|both)"
            return 2
            ;;
    esac

    if [ ! -f "$BASE_EXE" ]; then
        local have_fe=0 have_be=0
        [ -f "$FRONTEND_EXE" ] && have_fe=1
        [ -f "$BACKEND_EXE" ] && have_be=1
        case "$role" in
            frontend) [ "$have_fe" = "1" ] && return 0 ;;
            backend)  [ "$have_be" = "1" ] && return 0 ;;
            both)     [ "$have_fe" = "1" ] && [ "$have_be" = "1" ] && return 0 ;;
        esac
        err "Release binary not found at $BASE_EXE"
        err "Run: make build-frontend or make build-backend"
        exit 1
    fi

    case "$role" in
        frontend|both) cp -f "$BASE_EXE" "$FRONTEND_EXE" ;;
    esac
    case "$role" in
        backend|both)  cp -f "$BASE_EXE" "$BACKEND_EXE" ;;
    esac
    rm -f "$BASE_EXE"

    ok "Prepared role binaries (role=$role):"
    [ -f "$FRONTEND_EXE" ] && echo "  frontend: $FRONTEND_EXE"
    [ -f "$BACKEND_EXE" ]  && echo "  backend:  $BACKEND_EXE"
}

# Make sure the role-named binaries needed by the caller exist. Role-aware
# so `start_backend` doesn't complain when the frontend hasn't been built
# yet (and vice versa). Falls through to `prepare_binaries` to copy/rename
# from the bare omnilauncher if it's still around.
ensure_role_binaries() {
    local role="${1:-both}"
    case "$role" in
        frontend) [ -f "$FRONTEND_EXE" ] && return 0 ;;
        backend)  [ -f "$BACKEND_EXE" ] && return 0 ;;
        both)     [ -f "$FRONTEND_EXE" ] && [ -f "$BACKEND_EXE" ] && return 0 ;;
    esac
    prepare_binaries "$role"
}

remove_binaries() {
    rm -f "$BASE_EXE" "$FRONTEND_EXE" "$BACKEND_EXE" 2>/dev/null || true
}

# Stop processes by name (basename of binary). Falls back to pid file.
stop_by_name() {
    local name="$1"
    local pid_file="$2"
    local stopped=0

    if command -v pkill >/dev/null 2>&1; then
        if pkill -x -f "$name" 2>/dev/null; then
            stopped=1
        fi
    fi

    # pid-file fallback
    if [ -f "$pid_file" ]; then
        local pid
        pid="$(cat "$pid_file" 2>/dev/null || true)"
        if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            sleep 0.2
            kill -9 "$pid" 2>/dev/null || true
            stopped=1
        fi
        rm -f "$pid_file" 2>/dev/null || true
    fi

    if [ "$stopped" = "1" ]; then
        ok "Stopped $name"
    fi
}

stop_frontend() {
    stop_by_name "omnilauncher-frontend" "$FRONTEND_PID_FILE"
    # Older single-binary variant
    if command -v pkill >/dev/null 2>&1; then
        pkill -x "omnilauncher" 2>/dev/null || true
    fi
}

stop_backend() {
    stop_by_name "omnilauncher-backend" "$BACKEND_PID_FILE"

    # Free the port if anything else is camped on it.
    local pid_on_port=""
    if command -v lsof >/dev/null 2>&1; then
        pid_on_port="$(lsof -ti tcp:"$SPLIT_PORT" -sTCP:LISTEN 2>/dev/null | head -n 1 || true)"
    fi
    if [ -z "$pid_on_port" ] && command -v ss >/dev/null 2>&1; then
        pid_on_port="$(ss -tlnpH 2>/dev/null | awk -v p=":$SPLIT_PORT" '$4 ~ p {print $0}' \
            | grep -oE 'pid=[0-9]+' | head -n 1 | cut -d= -f2 || true)"
    fi
    if [ -n "${pid_on_port:-}" ]; then
        kill "$pid_on_port" 2>/dev/null || true
        sleep 0.2
        kill -9 "$pid_on_port" 2>/dev/null || true
    fi
}

# Start a binary detached, write its pid to a file.
start_detached() {
    local exe="$1"; shift
    local pid_file="$1"; shift
    # Remaining args ($@) are passed to the binary.

    if [ ! -x "$exe" ]; then
        err "Not executable: $exe"
        exit 1
    fi

    # nohup + setsid so it survives this shell.
    if command -v setsid >/dev/null 2>&1; then
        setsid nohup "$exe" "$@" >/dev/null 2>&1 < /dev/null &
    else
        nohup "$exe" "$@" >/dev/null 2>&1 < /dev/null &
    fi
    local pid=$!
    echo "$pid" > "$pid_file"
    ok "Started $(basename "$exe") (pid=$pid)"
}

start_frontend() {
    ensure_role_binaries frontend
    export OMNILAUNCHER_BACKEND_URL="$BACKEND_URL"
    cd "$REPO_DIR"
    if [ -n "$DEBUG_FLAG" ]; then
        start_detached "$FRONTEND_EXE" "$FRONTEND_PID_FILE" "$DEBUG_FLAG"
    else
        start_detached "$FRONTEND_EXE" "$FRONTEND_PID_FILE"
    fi
}

start_backend() {
    ensure_role_binaries backend
    export OMNILAUNCHER_SPLIT_HOST="$SPLIT_HOST"
    export OMNILAUNCHER_SPLIT_PORT="$SPLIT_PORT"
    cd "$REPO_DIR"
    if [ -n "$DEBUG_FLAG" ]; then
        start_detached "$BACKEND_EXE" "$BACKEND_PID_FILE" --split-backend "$DEBUG_FLAG"
    else
        start_detached "$BACKEND_EXE" "$BACKEND_PID_FILE" --split-backend
    fi
}

start_prod_debug_backend() {
    ensure_role_binaries backend
    export OMNILAUNCHER_SPLIT_HOST="$SPLIT_HOST"
    export OMNILAUNCHER_SPLIT_PORT="$SPLIT_PORT"
    cd "$REPO_DIR"
    start_detached "$BACKEND_EXE" "$BACKEND_PID_FILE" --split-backend --debug
}

start_prod_debug_frontend() {
    ensure_role_binaries frontend
    export OMNILAUNCHER_BACKEND_URL="$BACKEND_URL"
    cd "$REPO_DIR"
    start_detached "$FRONTEND_EXE" "$FRONTEND_PID_FILE" --debug
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

find_process_pids() {
    local name="$1"

    # Linux task names are limited to 15 bytes, so `pgrep -x` cannot find long
    # role names like omnilauncher-backend. Match the executable basename from
    # the full argv instead.
    ps -eo pid=,args= 2>/dev/null | awk -v name="$name" '
        {
            pid = $1
            $1 = ""
            sub(/^[[:space:]]+/, "")
            split($0, argv, /[[:space:]]+/)
            exe = argv[1]
            sub(/^.*\//, "", exe)
            if (exe == name) print pid
        }
    '
}

find_split_backend_pids() {
    {
        find_process_pids omnilauncher-backend
        ps -eo pid=,args= 2>/dev/null | awk '
            /[[:space:]]--split-backend([[:space:]]|$)/ {
                pid = $1
                $1 = ""
                sub(/^[[:space:]]+/, "")
                split($0, argv, /[[:space:]]+/)
                exe = argv[1]
                sub(/^.*\//, "", exe)
                if (exe == "omnilauncher") print pid
            }
        '
    } | awk '!seen[$0]++'
}

find_listener_pid() {
    local port="$1"
    local pid=""

    if command -v lsof >/dev/null 2>&1; then
        pid="$(lsof -nP -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null | head -n 1 || true)"
    fi
    if [ -z "$pid" ] && command -v ss >/dev/null 2>&1; then
        pid="$(ss -tlnpH 2>/dev/null | awk -v p=":$port" '
            $4 ~ p && match($0, /pid=[0-9]+/) {
                print substr($0, RSTART + 4, RLENGTH - 4)
                exit
            }
        ')"
    fi

    printf "%s\n" "$pid"
}

show_status() {
    echo
    info "=== OmniLauncher Status ==="
    echo

    # --- Binaries ---
    printf "%b\n" "${YELLOW}--- Binaries ---${NC}"
    if [ -f "$FRONTEND_EXE" ]; then
        local sz
        sz="$(du -m "$FRONTEND_EXE" 2>/dev/null | awk '{print $1}')"
        ok "  frontend exe: OK  (${sz:-?} MB)"
    else
        err "  frontend exe: MISSING"
    fi
    if [ -f "$BACKEND_EXE" ]; then
        local sz
        sz="$(du -m "$BACKEND_EXE" 2>/dev/null | awk '{print $1}')"
        ok "  backend  exe: OK  (${sz:-?} MB)"
    else
        err "  backend  exe: MISSING"
    fi

    # --- Processes ---
    printf "%b\n" "${YELLOW}--- Processes ---${NC}"
    local fe_pids be_pids
    fe_pids="$(find_process_pids omnilauncher-frontend)"
    if [ -n "$fe_pids" ]; then
        for pid in $fe_pids; do
            local mem_kb mem_mb
            mem_kb="$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ' || true)"
            if [ -n "${mem_kb:-}" ]; then
                mem_mb="$(awk "BEGIN{printf \"%.1f\", $mem_kb/1024}")"
            else
                mem_mb="?"
            fi
            ok "  frontend: RUNNING  PID=$pid  MEM=${mem_mb}MB"
        done
    else
        err "  frontend: STOPPED"
    fi

    be_pids="$(find_split_backend_pids)"
    if [ -z "$be_pids" ]; then
        be_pids="$(find_listener_pid "$SPLIT_PORT")"
    fi
    if [ -n "$be_pids" ]; then
        for pid in $be_pids; do
            local mem_kb mem_mb
            mem_kb="$(ps -o rss= -p "$pid" 2>/dev/null | tr -d ' ' || true)"
            if [ -n "${mem_kb:-}" ]; then
                mem_mb="$(awk "BEGIN{printf \"%.1f\", $mem_kb/1024}")"
            else
                mem_mb="?"
            fi
            ok "  backend:  RUNNING  PID=$pid  MEM=${mem_mb}MB"
        done
    else
        err "  backend:  STOPPED"
    fi

    # --- Port ---
    printf "%b\n" "${YELLOW}--- Network ---${NC}"
    local listener=""
    if command -v ss >/dev/null 2>&1; then
        listener="$(ss -tlnH 2>/dev/null | awk -v p=":$SPLIT_PORT" '$4 ~ p {print; exit}')"
    elif command -v lsof >/dev/null 2>&1; then
        listener="$(lsof -iTCP:"$SPLIT_PORT" -sTCP:LISTEN 2>/dev/null | sed -n '2p')"
    fi
    if [ -n "$listener" ]; then
        ok "  port $SPLIT_PORT: LISTENING"
    else
        err "  port $SPLIT_PORT: NOT LISTENING"
    fi

    # --- Health ---
    printf "%b\n" "${YELLOW}--- Health ---${NC}"
    if command -v curl >/dev/null 2>&1; then
        local body
        if body="$(curl -fsS --max-time 3 "$BACKEND_URL/health" 2>/dev/null)"; then
            ok "  $BACKEND_URL/health: OK"
            echo "    $body"
        else
            err "  $BACKEND_URL/health: UNREACHABLE"
        fi
    else
        warn "  curl not found, skipping health probe"
    fi
    echo
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
    prepare-binaries)     prepare_binaries "${1:-both}" ;;
    remove-binary)        remove_binaries ;;
    clean-frontend)       clean_frontend ;;
    clean-backend)        clean_backend ;;
    clean)                clean_all ;;
    *)
        err "ops.sh: unknown action '$ACTION'"
        echo "valid actions: stop-frontend stop-backend stop-all start-frontend start-backend"
        echo "               prod-debug-backend prod-debug-frontend prod-debug"
        echo "               test-backend status prepare-binaries remove-binary"
        echo "               clean-frontend clean-backend clean"
        exit 2
        ;;
esac
