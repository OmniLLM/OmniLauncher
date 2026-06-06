#!/usr/bin/env bash
# Linux/macOS counterpart of scripts/test-e2e.ps1.
# Mimics the frontend user flow: health, settings, AI upstream probe,
# search, AI query + SSE wait, favorites, skills, plugins, slash, CORS, 404.
#
# Usage:  scripts/test-e2e.sh [-BaseUrl URL] [-AiTimeoutSeconds N] [--token VALUE] [--token-file PATH]
#
# Auth token resolution order: --token > $OMNILAUNCHER_SERVER_TOKEN > --token-file
# (default token file: ~/.config/omnilauncher/server-token). When a token is
# resolved it is sent as the X-OmniLauncher-Token header on every request,
# including the SSE listener connections.

set -uo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:1422}"
AI_TIMEOUT="${AI_TIMEOUT:-30}"
TOKEN="${OMNILAUNCHER_SERVER_TOKEN:-}"
TOKEN_FILE="${HOME}/.config/omnilauncher/server-token"
TOKEN_FROM_PARAM=""

while [ $# -gt 0 ]; do
    case "$1" in
        -BaseUrl|--BaseUrl)               BASE_URL="${2:-}"; shift 2 ;;
        -AiTimeoutSeconds|--ai-timeout)   AI_TIMEOUT="${2:-30}"; shift 2 ;;
        -Token|--token)                   TOKEN_FROM_PARAM="${2:-}"; shift 2 ;;
        -TokenFile|--token-file)          TOKEN_FILE="${2:-}"; shift 2 ;;
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
    RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; CYAN='\033[0;36m'; GRAY='\033[0;90m'; NC='\033[0m'
else
    RED=''; GREEN=''; YELLOW=''; CYAN=''; GRAY=''; NC=''
fi

if ! command -v curl >/dev/null 2>&1; then
    printf "${RED}curl is required for E2E tests${NC}\n"; exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
    printf "${RED}python3 is required for E2E tests (used to probe JSON)${NC}\n"; exit 1
fi

passed=0
failed=0
warnings=0

pass()  { printf "  ${GREEN}PASS${NC}  %s\n" "$*"; passed=$((passed+1)); }
fail()  { printf "  ${RED}FAIL${NC}  %s\n" "$*"; failed=$((failed+1)); }
warn()  { printf "  ${YELLOW}WARN${NC}  %s\n" "$*"; warnings=$((warnings+1)); }

# Returns "<status>\n<body>" on stdout. Never exits non-zero.
api() {
    local method="$1" path="$2" body="${3:-}"
    local args=(-sS -o /tmp/oml-e2e-body.$$ -w "%{http_code}" --max-time 10 -X "$method")
    args+=("${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}")
    if [ -n "$body" ]; then
        args+=(-H "Content-Type: application/json; charset=utf-8" -d "$body")
    fi
    local code
    code="$(curl "${args[@]}" "$BASE_URL$path" 2>/dev/null)" || true
    [ -z "$code" ] && code="000"
    echo "$code"
    cat /tmp/oml-e2e-body.$$ 2>/dev/null || true
    rm -f /tmp/oml-e2e-body.$$ 2>/dev/null || true
}

# json_field <body> <field>  - prints field value (or empty)
json_field() {
    printf '%s' "$1" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
field = '$2'
if isinstance(data, dict) and field in data:
    val = data[field]
    if val is None:
        sys.exit(0)
    print(val)
" 2>/dev/null || true
}

# json_count <body>  - prints len() if JSON list, else empty
json_count() {
    printf '%s' "$1" | python3 -c "
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
if isinstance(data, list):
    print(len(data))
" 2>/dev/null || true
}

echo
printf "${CYAN}========================================${NC}\n"
printf "${CYAN}  OmniLauncher E2E API Test${NC}\n"
printf "${CYAN}  Backend: %s${NC}\n" "$BASE_URL"
printf "${CYAN}========================================${NC}\n"
echo

# 1. Health
printf "%b\n" "${YELLOW}--- 1. Health Check ---${NC}"
out="$(api GET /health)"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ] && [ "$(json_field "$body" ok)" = "True" -o "$(json_field "$body" ok)" = "true" ]; then
    pass "GET /health returns ok=true"
else
    fail "GET /health -> status=$status body=$body"
fi

# 2. Settings
printf "%b\n" "${YELLOW}--- 2. Settings ---${NC}"
settings_body=""
out="$(api GET /api/settings)"
status="$(printf '%s\n' "$out" | head -n 1)"
settings_body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ] && [ -n "$(json_field "$settings_body" ai_model)" ] && [ -n "$(json_field "$settings_body" theme)" ]; then
    pass "GET /api/settings has ai_model and theme"
    ai_base_url="$(json_field "$settings_body" ai_base_url)"
    ai_api_key="$(json_field "$settings_body" ai_api_key)"
    if [ -z "$ai_base_url" ]; then
        warn "AI config -> ai_base_url is empty -- AI queries will fail"
    elif [ -z "$ai_api_key" ]; then
        warn "AI config -> ai_api_key is empty -- AI queries may fail"
    else
        pass "AI config has base_url=$ai_base_url model=$(json_field "$settings_body" ai_model)"
    fi
else
    fail "GET /api/settings -> status=$status"
    ai_base_url=""
fi

# 3. AI upstream probe
printf "%b\n" "${YELLOW}--- 3. AI Upstream Reachability ---${NC}"
if [ -n "${ai_base_url:-}" ]; then
    probe_url="${ai_base_url%/}/v1/models"
    probe_status="$(curl -sS -o /dev/null -w "%{http_code}" --max-time 5 "$probe_url" 2>/dev/null)" || true
    [ -z "$probe_status" ] && probe_status="000"
    case "$probe_status" in
        200) pass "AI upstream $probe_url is reachable (status=200)" ;;
        401|403) warn "AI upstream $probe_url returned auth error -- check API key" ;;
        000) fail "AI upstream $probe_url -- TIMEOUT or unreachable. AI queries will hang." ;;
        *) warn "AI upstream $probe_url returned status $probe_status" ;;
    esac
else
    warn "No ai_base_url configured, skipping"
fi

# 4. Launcher Config
printf "%b\n" "${YELLOW}--- 4. Launcher Config ---${NC}"
out="$(api GET /api/launcher-config)"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "GET /api/launcher-config" || fail "GET /api/launcher-config -> status=$status"

# 5. Sessions
printf "%b\n" "${YELLOW}--- 5. Sessions ---${NC}"
out="$(api GET /api/sessions)"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ]; then
    n="$(json_count "$body")"
    pass "GET /api/sessions returned ${n:-?} session(s)"
else
    fail "GET /api/sessions -> status=$status"
fi
out="$(api GET /api/sessions/current)"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
[ "$status" = "200" ] && pass "GET /api/sessions/current = $body" || fail "GET /api/sessions/current -> status=$status"
out="$(api POST /api/sessions/clear)"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "POST /api/sessions/clear (new session)" || fail "POST /api/sessions/clear -> status=$status"

# 6. Search
printf "%b\n" "${YELLOW}--- 6. Search ---${NC}"
out="$(api POST /api/search '{"query":"calc"}')"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ]; then
    n="$(json_count "$body")"
    if [ -n "$n" ] && [ "$n" -gt 0 ]; then pass "POST /api/search 'calc' returned $n result(s)"
    else warn "Search -> no results for 'calc'"; fi
else
    fail "POST /api/search -> status=$status"
fi
out="$(api POST /api/search '{"query":""}')"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "POST /api/search empty query (no crash)" || fail "POST /api/search empty -> status=$status"

# 7. AI query + SSE
printf "%b\n" "${YELLOW}--- 7. AI Query + SSE Event Flow ---${NC}"
api POST /api/ai/cancel >/dev/null 2>&1 || true

DONE_FILE="$(mktemp -t oml-e2e-done.XXXXXX)"
ERR_FILE="$(mktemp -t oml-e2e-err.XXXXXX)"
done_url="$BASE_URL/api/events/omnilauncher%3A%2F%2Fai-done"
err_url="$BASE_URL/api/events/omnilauncher%3A%2F%2Fai-error"
curl -sS --max-time "$AI_TIMEOUT" "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" "$done_url" >"$DONE_FILE" 2>/dev/null &
DONE_PID=$!
curl -sS --max-time "$AI_TIMEOUT" "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" "$err_url" >"$ERR_FILE" 2>/dev/null &
ERR_PID=$!
sleep 0.5  # let SSE connect

out="$(api POST /api/ai/query '{"query":"say hello in one word"}')"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ] && [ "$body" = "true" ]; then
    pass "POST /api/ai/query accepted (returned true)"
else
    fail "POST /api/ai/query -> status=$status body=$body"
fi

printf "  ${GRAY}...   waiting up to %ss for SSE response...${NC}\n" "$AI_TIMEOUT"
elapsed=0
while [ "$elapsed" -lt "$AI_TIMEOUT" ]; do
    if grep -q '^data: ' "$DONE_FILE" 2>/dev/null || grep -q '^data: ' "$ERR_FILE" 2>/dev/null; then break; fi
    sleep 1
    elapsed=$((elapsed+1))
done
kill "$DONE_PID" "$ERR_PID" 2>/dev/null || true
wait 2>/dev/null || true

if grep -q '^data: ' "$DONE_FILE" 2>/dev/null; then
    payload="$(grep -m1 '^data: ' "$DONE_FILE" | sed 's/^data: //')"
    content="$(printf '%s' "$payload" | python3 -c "
import json, sys
try: print(json.load(sys.stdin).get('content', '')[:80])
except Exception: pass" 2>/dev/null || true)"
    if [ -n "$content" ]; then
        pass "SSE ai-done received: content='$content...'"
    else
        warn "SSE ai-done received but content is empty"
    fi
elif grep -q '^data: ' "$ERR_FILE" 2>/dev/null; then
    payload="$(grep -m1 '^data: ' "$ERR_FILE" | sed 's/^data: //')"
    fail "SSE ai-error received: $payload"
else
    fail "SSE timeout: No ai-done or ai-error event within ${AI_TIMEOUT}s"
fi
rm -f "$DONE_FILE" "$ERR_FILE" 2>/dev/null || true

# 8. AI cancel
printf "%b\n" "${YELLOW}--- 8. AI Cancel ---${NC}"
out="$(api POST /api/ai/cancel)"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
[ "$status" = "200" ] && pass "POST /api/ai/cancel returned $body" || fail "POST /api/ai/cancel -> status=$status"

# 9. Favorites
printf "%b\n" "${YELLOW}--- 9. Favorites ---${NC}"
out="$(api GET /api/favorites)"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "GET /api/favorites" || fail "GET /api/favorites -> status=$status"

# 10. Skills
printf "%b\n" "${YELLOW}--- 10. Skills ---${NC}"
out="$(api GET /api/skills)"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ]; then
    n="$(json_count "$body")"
    pass "GET /api/skills returned ${n:-?} skill(s)"
else
    fail "GET /api/skills -> status=$status"
fi
out="$(api GET /api/skills/usage)"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "GET /api/skills/usage" || fail "GET /api/skills/usage -> status=$status"

# 11. Plugins
printf "%b\n" "${YELLOW}--- 11. Plugins ---${NC}"
out="$(api GET /api/plugins/collections)"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "GET /api/plugins/collections" || fail "GET /api/plugins/collections -> status=$status"
out="$(api GET /api/plugins/runtime-deps)"
status="$(printf '%s\n' "$out" | head -n 1)"
[ "$status" = "200" ] && pass "GET /api/plugins/runtime-deps" || fail "GET /api/plugins/runtime-deps -> status=$status"

# 12. Slash
printf "%b\n" "${YELLOW}--- 12. Slash Commands ---${NC}"
out="$(api POST /api/slash/preview '{"query":"/calc 2+2"}')"
status="$(printf '%s\n' "$out" | head -n 1)"
body="$(printf '%s\n' "$out" | tail -n +2)"
if [ "$status" = "200" ]; then
    n="$(json_count "$body")"
    if [ -n "$n" ] && [ "$n" -gt 0 ]; then
        title="$(printf '%s' "$body" | python3 -c "
import json, sys
try: print(json.load(sys.stdin)[0].get('title',''))
except Exception: pass" 2>/dev/null || true)"
        pass "POST /api/slash/preview '/calc 2+2' -> $title"
    else
        pass "POST /api/slash/preview returned empty (no matching command)"
    fi
else
    fail "POST /api/slash/preview -> status=$status"
fi

# 13. CORS
printf "%b\n" "${YELLOW}--- 13. CORS ---${NC}"
cors="$(curl -sS -X OPTIONS -D - -o /dev/null --max-time 5 \
    "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" "$BASE_URL/api/settings" 2>/dev/null \
    | tr -d '\r' | awk -F': ' 'tolower($1)=="access-control-allow-origin"{print $2}')"
[ "$cors" = "*" ] && pass "OPTIONS CORS headers present" || fail "CORS -> got '$cors'"

# 14. 404
printf "%b\n" "${YELLOW}--- 14. Error Handling ---${NC}"
status="$(curl -sS -o /dev/null -w "%{http_code}" --max-time 5 "${TOKEN_HEADER_ARGS[@]+"${TOKEN_HEADER_ARGS[@]}"}" "$BASE_URL/api/nonexistent" 2>/dev/null)" || true
[ -z "$status" ] && status="000"
[ "$status" = "404" ] && pass "GET /api/nonexistent returns 404" || fail "GET /api/nonexistent -> status=$status"

# Summary
echo
printf "${CYAN}========================================${NC}\n"
printf "${CYAN}  Results${NC}\n"
printf "${CYAN}========================================${NC}\n"
printf "  ${GREEN}Passed:   %d${NC}\n" "$passed"
if [ "$failed" -gt 0 ]; then
    printf "  ${RED}Failed:   %d${NC}\n" "$failed"
else
    printf "  ${GREEN}Failed:   0${NC}\n"
fi
if [ "$warnings" -gt 0 ]; then
    printf "  ${YELLOW}Warnings: %d${NC}\n" "$warnings"
else
    printf "  ${GREEN}Warnings: 0${NC}\n"
fi
echo

if [ "$failed" -gt 0 ]; then
    printf "  ${RED}E2E TESTS FAILED${NC}\n\n"; exit 1
fi
if [ "$warnings" -gt 0 ]; then
    printf "  ${YELLOW}E2E TESTS PASSED WITH WARNINGS${NC}\n\n"
else
    printf "  ${GREEN}ALL E2E TESTS PASSED${NC}\n\n"
fi
