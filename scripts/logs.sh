#!/usr/bin/env bash
set -euo pipefail

YELLOW='\033[1;33m'
CYAN='\033[0;36m'
DARK_GRAY='\033[0;90m'
NC='\033[0m'

log_path="$HOME/.omnilauncher/omnilauncher.log"

if [ ! -f "$log_path" ]; then
    echo ""
    echo -e "${YELLOW}No log file found at:${NC}"
    echo "  $log_path"
    echo ""
    echo -e "${CYAN}Run with --debug to enable logging:${NC}"
    echo "  make prod-debug"
    echo ""
    exit 0
fi

echo ""
echo -e "${CYAN}Tailing log file:${NC}"
echo "  $log_path"
echo -e "${DARK_GRAY}(Press Ctrl+C to stop)${NC}"
echo ""

tail -n 50 -f "$log_path"
