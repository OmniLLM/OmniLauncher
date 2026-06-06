#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

app_running=false
dev_running=false
rel_exists=false
dbg_exists=false

if pgrep -x omnilauncher >/dev/null 2>&1; then
    app_running=true
fi

if lsof -ti:1420 >/dev/null 2>&1 || ss -tlnp 2>/dev/null | grep -q ':1420 '; then
    dev_running=true
fi

if [ -f "src-tauri/target/release/omnilauncher" ]; then
    rel_exists=true
fi

if [ -f "src-tauri/target/dev/omnilauncher" ]; then
    dbg_exists=true
fi

echo ""
echo -e "${CYAN}OmniLauncher Status:${NC}"
echo "===================="
echo ""

if $app_running; then
    echo -e "  App Process:    ${GREEN}Running${NC}"
else
    echo -e "  App Process:    ${YELLOW}Not running${NC}"
fi

if $dev_running; then
    echo -e "  Dev Server:     ${GREEN}Running (port 1420)${NC}"
else
    echo -e "  Dev Server:     ${YELLOW}Not running${NC}"
fi

if $rel_exists; then
    echo -e "  Release Binary: ${GREEN}Exists${NC}"
else
    echo -e "  Release Binary: ${RED}Not built${NC}"
fi

if $dbg_exists; then
    echo -e "  Debug Binary:   ${GREEN}Exists${NC}"
else
    echo -e "  Debug Binary:   ${RED}Not built${NC}"
fi

echo ""
