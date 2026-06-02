#!/usr/bin/env bash
# Cold-start benchmark: spawn → first X window for OmniLauncher.
# Uses one shared Xvfb; fresh OMNILAUNCHER_CONFIG_DIR per run for true cold start.
set -uo pipefail

REPO=/data/tools/OmniLauncher
BIN_MAIN="$REPO/target-bench/main/omnilauncher"
BIN_BRANCH="$REPO/target-bench/branch/omnilauncher"
RUNS=${RUNS:-5}
DISP=":95"

# Ensure Xvfb is up
if ! DISPLAY=$DISP xdpyinfo >/dev/null 2>&1; then
    Xvfb $DISP -screen 0 1280x800x24 -nolisten tcp >/dev/null 2>&1 &
    sleep 0.4
fi

bench_one() {
    local label="$1"; local bin="$2"
    local samples=()
    for i in $(seq 1 "$RUNS"); do
        local dd; dd=$(mktemp -d)
        # Best-effort cold cache (won't work without root; falls back fine)
        sync
        local t0; t0=$(date +%s.%N)
        OMNILAUNCHER_CONFIG_DIR="$dd" DISPLAY=$DISP "$bin" >/dev/null 2>&1 &
        local app=$!
        local elapsed=""
        for _ in $(seq 1 200); do
            local w
            w=$(DISPLAY=$DISP xdotool search --pid "$app" 2>/dev/null | head -1)
            if [ -n "$w" ]; then
                local t1; t1=$(date +%s.%N)
                elapsed=$(awk "BEGIN{printf \"%.3f\", $t1 - $t0}")
                break
            fi
            sleep 0.02
        done
        kill -INT "$app" 2>/dev/null
        sleep 0.15
        kill -KILL "$app" 2>/dev/null
        wait "$app" 2>/dev/null
        rm -rf "$dd"
        if [ -z "$elapsed" ]; then
            echo "  $label run $i: TIMEOUT"
            samples+=("999")
        else
            echo "  $label run $i: ${elapsed}s"
            samples+=("$elapsed")
        fi
        sleep 0.3
    done
    printf '%s\n' "${samples[@]}" | sort -n | awk -v lab="$label" '
        { v[NR]=$1+0 }
        END {
            n=NR; mid = (n%2)?v[(n+1)/2]:(v[n/2]+v[n/2+1])/2
            sum=0; for(i=1;i<=n;i++) sum+=v[i]
            printf "  %s -> min=%.3fs  median=%.3fs  mean=%.3fs  max=%.3fs (n=%d)\n", lab, v[1], mid, sum/n, v[n], n
        }'
}

echo "Benchmark: $RUNS cold starts each. Display=$DISP"
echo
bench_one MAIN   "$BIN_MAIN"
echo
bench_one BRANCH "$BIN_BRANCH"
