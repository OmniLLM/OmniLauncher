#!/usr/bin/env bash
# Linux/macOS counterpart of scripts/smoke-endpoints.ps1.
# Hits the server HTTP endpoints with curl and asserts status / JSON fields.
#
# Usage:  scripts/smoke-endpoints.sh [-BaseUrl URL] [--token VALUE] [--token-file PATH]
#         BASE_URL=http://127.0.0.1:1422 scripts/smoke-endpoints.sh
#
# Auth token resolution order: --token > $OMNILAUNCHER_SERVER_TOKEN > --token-file
# (default token file: ~/.config/omnilauncher/server-token). When a token is
# resolved it is sent as the X-OmniLauncher-Token header on every request.

set -uo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:1422}"
TOKEN="${OMNILAUNCHER_SERVER_TOKEN:-}"
TOKEN_FILE="${HOME}/.config/omnilauncher/server-token"
TOKEN_FROM_PARAM=""
while [ $# -gt 0 ]; do
    case "$1" in
        -BaseUrl|--BaseUrl|--base-url) BASE_URL="${2:-}"; shift 2 ;;
        -Token|--token)                TOKEN_FROM_PARAM="${2:-}"; shift 2 ;;
        -TokenFile|--token-file)       TOKEN_FILE="${2:-}"; shift 2 ;;
        *) shift ;;
    esac
done

# Resolve token: param > env > file.
if [ -n "$TOKEN_FROM_PARAM" ]; then
    TOKEN="$TOKEN_FROM_PARAM"
elif [ -z "$TOKEN" ] && [ -f "$TOKEN_FILE" ]; then
    TOKEN="$(tr -d '\r\n' < "$TOKEN_FILE" 2>/dev/null || true)"
fi

# Build curl header args (empty when no token resolved). Expanded into each
# curl invocation with the `+...` guard so it is safe under `set -u`.
TOKEN_HEADER_ARGS=()
if [ -n "$TOKEN" ]; then
    TOKEN_HEADER_ARGS=(-H "X-OmniLauncher-Token: $TOKEN")
fi

if [ -t 1 ]; then
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; CYAN=''; NC=''
fi

if ! command -v curl >/dev/null 2>&1; then
    printf "${RED}curl is required for smoke tests${NC}\n"; exit 1
fi

passed=0
failed=0

# request method path body  -> echoes "<status>\n<body>"
request() {
    local method="$1" path="$2" body="${3:-}"
    local status
    if [ -n "$body" ]; then
        status="$(curl -sS -o /tmp/oml-smoke-body.$$ -w "%{http_code}" \
             -X "$method" -H "Content-Type: application/json" \
             "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" \
             -d "$body" --max-time 10 "$BASE_URL$path" 2>/dev/null)" || true
    else
        status="$(curl -sS -o /tmp/oml-smoke-body.$$ -w "%{http_code}" \
             -X "$method" \
             "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" \
             --max-time 10 "$BASE_URL$path" 2>/dev/null)" || true
    fi
    [ -z "$status" ] && status="000"
    echo "$status"
    cat /tmp/oml-smoke-body.$$ 2>/dev/null || true
    rm -f /tmp/oml-smoke-body.$$ 2>/dev/null || true
}

check() {
    local method="$1" path="$2" body="${3:-}" expected="${4:-200}"
    local out status
    out="$(request "$method" "$path" "$body")"
    status="$(printf '%s\n' "$out" | head -n 1)"
    if [ "$status" = "$expected" ]; then
        printf "${GREEN}OK   %s %s (status=%s)${NC}\n" "$method" "$path" "$status"
        passed=$((passed+1))
    else
        printf "${RED}FAIL %s %s -> expected %s got %s${NC}\n" "$method" "$path" "$expected" "$status"
        failed=$((failed+1))
    fi
}

check_json_field() {
    local method="$1" path="$2" body="${3:-}" field="$4"
    local out status payload
    out="$(request "$method" "$path" "$body")"
    status="$(printf '%s\n' "$out" | head -n 1)"
    payload="$(printf '%s\n' "$out" | tail -n +2)"
    if [ "$status" != "200" ]; then
        printf "${RED}FAIL %s %s -> status %s${NC}\n" "$method" "$path" "$status"
        failed=$((failed+1))
        return
    fi
    # Use python for robust JSON field probing (no jq dependency).
    if printf '%s' "$payload" | python3 -c "
import json, sys
data = json.load(sys.stdin)
field = '$field'
if isinstance(data, dict) and field in data:
    sys.exit(0)
sys.exit(1)
" 2>/dev/null; then
        printf "${GREEN}OK   %s %s (field '%s' present)${NC}\n" "$method" "$path" "$field"
        passed=$((passed+1))
    else
        printf "${RED}FAIL %s %s -> missing field '%s'${NC}\n" "$method" "$path" "$field"
        failed=$((failed+1))
    fi
}

echo
printf "${CYAN}=== OmniLauncher Backend Smoke Tests ===${NC}\n"
echo "Backend: $BASE_URL"
echo

printf "%b\n" "${YELLOW}--- Health ---${NC}"
check          GET  "/health"
check_json_field GET "/health" "" "ok"

printf "%b\n" "${YELLOW}--- Settings ---${NC}"
check            GET "/api/settings"
check_json_field GET "/api/settings" "" "ai_model"
check_json_field GET "/api/settings" "" "theme"

printf "%b\n" "${YELLOW}--- Launcher Config ---${NC}"
check GET "/api/launcher-config"

printf "%b\n" "${YELLOW}--- Skills ---${NC}"
check GET "/api/skills"
check GET "/api/skills/usage"

printf "%b\n" "${YELLOW}--- Plugins ---${NC}"
check GET "/api/plugins/collections"
check GET "/api/plugins/runtime-deps"

printf "%b\n" "${YELLOW}--- Search ---${NC}"
check POST "/api/search" '{"query":"calc"}'
check POST "/api/search" '{"query":""}'

printf "%b\n" "${YELLOW}--- Slash Commands ---${NC}"
check POST "/api/slash/preview" '{"query":"/calc 2+2"}'

printf "%b\n" "${YELLOW}--- Sessions ---${NC}"
check GET  "/api/sessions"
check GET  "/api/sessions/current"
check POST "/api/sessions/clear"

printf "%b\n" "${YELLOW}--- Favorites ---${NC}"
check GET "/api/favorites"

printf "%b\n" "${YELLOW}--- AI Cancel ---${NC}"
check POST "/api/ai/cancel"

printf "%b\n" "${YELLOW}--- CORS ---${NC}"
cors_header="$(curl -sS -X OPTIONS -D - -o /dev/null --max-time 5 \
    "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" "$BASE_URL/api/settings" 2>/dev/null \
    | tr -d '\r' | awk -F': ' 'tolower($1)=="access-control-allow-origin"{print $2}')"
if [ "$cors_header" = "*" ]; then
    printf "${GREEN}OK   OPTIONS /api/settings (CORS headers present)${NC}\n"
    passed=$((passed+1))
else
    printf "${RED}FAIL OPTIONS /api/settings -> missing CORS header${NC}\n"
    failed=$((failed+1))
fi

printf "%b\n" "${YELLOW}--- Error Handling ---${NC}"
check GET "/api/nonexistent" "" 404

echo
printf "${CYAN}=== Results ===${NC}\n"
printf "${GREEN}Passed: %d${NC}\n" "$passed"
if [ "$failed" -gt 0 ]; then
    printf "${RED}Failed: %d${NC}\n" "$failed"
    echo
    printf "${RED}SMOKE TESTS FAILED${NC}\n"
    exit 1
fi
printf "${GREEN}Failed: 0${NC}\n"
echo
printf "${GREEN}All smoke checks passed.${NC}\n"
