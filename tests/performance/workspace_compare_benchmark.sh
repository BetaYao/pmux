#!/usr/bin/env bash
# Benchmark: compare terminal input latency between two workspaces under tmux-cc conditions.
# Usage: ./workspace_compare_benchmark.sh /path/to/workspace1 /path/to/workspace2
set -euo pipefail

WS1="${1:-/Users/matt.chow/workspace/okena}"
WS2="${2:-/Users/matt.chow/workspace/saas-mono}"
ITERATIONS=50
RESULTS_DIR="$(dirname "$0")/../regression/results"
mkdir -p "$RESULTS_DIR"
REPORT="$RESULTS_DIR/workspace_compare_$(date +%Y%m%d_%H%M%S).md"

ws_basename() { basename "$1"; }
SESS1="bench-$(ws_basename "$WS1")"
SESS2="bench-$(ws_basename "$WS2")"

cleanup() {
    tmux kill-session -t "$SESS1" 2>/dev/null || true
    tmux kill-session -t "$SESS2" 2>/dev/null || true
}
trap cleanup EXIT

echo "=== Workspace Input Latency Benchmark ==="
echo "WS1: $WS1 (session: $SESS1)"
echo "WS2: $WS2 (session: $SESS2)"
echo "Iterations: $ITERATIONS"
echo ""

# Create test sessions
tmux new-session -d -s "$SESS1" -c "$WS1" 2>/dev/null || true
tmux new-session -d -s "$SESS2" -c "$WS2" 2>/dev/null || true
sleep 2

# Get pane info
PANE1=$(tmux list-panes -t "$SESS1" -F '#{pane_id}' | head -1)
PANE2=$(tmux list-panes -t "$SESS2" -F '#{pane_id}' | head -1)
TTY1=$(tmux list-panes -t "$SESS1" -F '#{pane_tty}' | head -1)
TTY2=$(tmux list-panes -t "$SESS2" -F '#{pane_tty}' | head -1)
SIZE1=$(tmux list-panes -t "$SESS1" -F '#{pane_width}x#{pane_height}' | head -1)
SIZE2=$(tmux list-panes -t "$SESS2" -F '#{pane_width}x#{pane_height}' | head -1)

echo "Pane1: $PANE1 ($SIZE1) TTY=$TTY1"
echo "Pane2: $PANE2 ($SIZE2) TTY=$TTY2"
echo ""

# Wait for shell prompt
sleep 1
tmux send-keys -t "$SESS1" "export PS1='BENCH> '" Enter
tmux send-keys -t "$SESS2" "export PS1='BENCH> '" Enter
sleep 1

# --- Test 1: send-keys -H latency (pmux's actual input path) ---
echo "### Test 1: send-keys -H round-trip latency"

measure_sendkeys_latency() {
    local sess="$1"
    local iters="$2"
    local latencies=()

    # Clear pane
    tmux send-keys -t "$sess" "clear" Enter
    sleep 0.5

    for i in $(seq 1 "$iters"); do
        local marker="M${i}X"
        local hex=$(printf '%s\n' "echo $marker" | xxd -p | tr -d '\n')
        hex="${hex}0d"  # append \r

        local start_ns=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
        tmux send-keys -H -t "$sess" $hex
        
        # Poll for marker in capture-pane
        local found=0
        for attempt in $(seq 1 100); do
            if tmux capture-pane -t "$sess" -p 2>/dev/null | grep -q "$marker"; then
                found=1
                break
            fi
            sleep 0.01
        done
        local end_ns=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')

        if [ "$found" = "1" ]; then
            local diff_ms=$(( (end_ns - start_ns) / 1000000 ))
            latencies+=("$diff_ms")
        fi
        
        # Clear for next iteration
        tmux send-keys -t "$sess" "clear" Enter
        sleep 0.05
    done

    # Output latencies
    printf '%s\n' "${latencies[@]}"
}

echo "  Testing $WS1..."
LAT1_RAW=$(measure_sendkeys_latency "$SESS1" "$ITERATIONS")
echo "  Testing $WS2..."
LAT2_RAW=$(measure_sendkeys_latency "$SESS2" "$ITERATIONS")

# --- Test 2: Direct TTY write latency ---
echo ""
echo "### Test 2: Direct TTY write latency"

measure_tty_write_latency() {
    local sess="$1"
    local tty="$2"
    local iters="$3"
    local latencies=()

    tmux send-keys -t "$sess" "clear" Enter
    sleep 0.5

    for i in $(seq 1 "$iters"); do
        local marker="D${i}Y"

        local start_ns=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
        printf "echo %s\r" "$marker" > "$tty" 2>/dev/null || true

        local found=0
        for attempt in $(seq 1 100); do
            if tmux capture-pane -t "$sess" -p 2>/dev/null | grep -q "$marker"; then
                found=1
                break
            fi
            sleep 0.01
        done
        local end_ns=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')

        if [ "$found" = "1" ]; then
            local diff_ms=$(( (end_ns - start_ns) / 1000000 ))
            latencies+=("$diff_ms")
        fi

        tmux send-keys -t "$sess" "clear" Enter
        sleep 0.05
    done

    printf '%s\n' "${latencies[@]}"
}

echo "  Testing $WS1 (direct TTY)..."
LAT1_TTY=$(measure_tty_write_latency "$SESS1" "$TTY1" "$ITERATIONS")
echo "  Testing $WS2 (direct TTY)..."
LAT2_TTY=$(measure_tty_write_latency "$SESS2" "$TTY2" "$ITERATIONS")

# --- Test 3: Shell startup / prompt speed ---
echo ""
echo "### Test 3: Shell .zshrc / prompt complexity"

measure_prompt_time() {
    local dir="$1"
    local start_ns=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
    # Measure time for a new zsh to reach interactive prompt
    echo "exit" | zsh -i -c "exit" 2>/dev/null
    local end_ns=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
    echo $(( (end_ns - start_ns) / 1000000 ))
}

echo "  Measuring shell startup in $WS1..."
SHELL_TIME1=$(cd "$WS1" && measure_prompt_time "$WS1")
echo "  Measuring shell startup in $WS2..."
SHELL_TIME2=$(cd "$WS2" && measure_prompt_time "$WS2")

# --- Test 4: Repo git status overhead ---
echo ""
echo "### Test 4: git status overhead"
echo "  $WS1..."
GIT_TIME1=$( { time git -C "$WS1" status --porcelain >/dev/null 2>&1; } 2>&1 | grep real | awk '{print $2}')
echo "  $WS2..."
GIT_TIME2=$( { time git -C "$WS2" status --porcelain >/dev/null 2>&1; } 2>&1 | grep real | awk '{print $2}')

# --- Stats helper ---
calc_stats() {
    local data="$1"
    python3 -c "
import sys
vals = [int(x) for x in '''$data'''.strip().split('\n') if x.strip()]
if not vals:
    print('N=0 min=0 max=0 avg=0 p50=0 p95=0 p99=0')
    sys.exit()
vals.sort()
n = len(vals)
avg = sum(vals) / n
p50 = vals[n//2]
p95 = vals[int(n*0.95)]
p99 = vals[int(n*0.99)]
print(f'N={n} min={vals[0]} max={vals[-1]} avg={avg:.1f} p50={p50} p95={p95} p99={p99}')
"
}

# --- Generate report ---
STATS1=$(calc_stats "$LAT1_RAW")
STATS2=$(calc_stats "$LAT2_RAW")
STATS1_TTY=$(calc_stats "$LAT1_TTY")
STATS2_TTY=$(calc_stats "$LAT2_TTY")

cat > "$REPORT" <<EOF
# Workspace Input Latency Comparison

**Date**: $(date '+%Y-%m-%d %H:%M:%S')
**tmux version**: $(tmux -V)
**Iterations**: $ITERATIONS

## Workspaces

| | WS1 | WS2 |
|---|---|---|
| Path | \`$(ws_basename "$WS1")\` | \`$(ws_basename "$WS2")\` |
| Pane size | $SIZE1 | $SIZE2 |
| Pane TTY | $TTY1 | $TTY2 |

## Test 1: send-keys -H round-trip (ms)

This is pmux's actual input path. Measures: send hex → tmux processes → shell executes → capture-pane sees output.

| Metric | $(ws_basename "$WS1") | $(ws_basename "$WS2") |
|---|---|---|
| Stats | $STATS1 | $STATS2 |

## Test 2: Direct TTY write round-trip (ms)

Bypasses tmux command parsing. Writes directly to pane TTY.

| Metric | $(ws_basename "$WS1") | $(ws_basename "$WS2") |
|---|---|---|
| Stats | $STATS1_TTY | $STATS2_TTY |

## Test 3: Shell startup time

Time for \`zsh -i -c exit\` in each directory (ms).

| $(ws_basename "$WS1") | $(ws_basename "$WS2") |
|---|---|
| ${SHELL_TIME1}ms | ${SHELL_TIME2}ms |

## Test 4: git status overhead

| $(ws_basename "$WS1") | $(ws_basename "$WS2") |
|---|---|
| $GIT_TIME1 | $GIT_TIME2 |

## Raw Data

### send-keys -H latencies (ms)

#### $(ws_basename "$WS1")
\`\`\`
$(echo "$LAT1_RAW" | tr '\n' ' ')
\`\`\`

#### $(ws_basename "$WS2")
\`\`\`
$(echo "$LAT2_RAW" | tr '\n' ' ')
\`\`\`

### Direct TTY write latencies (ms)

#### $(ws_basename "$WS1")
\`\`\`
$(echo "$LAT1_TTY" | tr '\n' ' ')
\`\`\`

#### $(ws_basename "$WS2")
\`\`\`
$(echo "$LAT2_TTY" | tr '\n' ' ')
\`\`\`
EOF

echo ""
echo "=== Report written to: $REPORT ==="
echo ""
echo "=== Summary ==="
echo "send-keys -H:  $(ws_basename "$WS1"): $STATS1"
echo "send-keys -H:  $(ws_basename "$WS2"): $STATS2"
echo "Direct TTY:    $(ws_basename "$WS1"): $STATS1_TTY"
echo "Direct TTY:    $(ws_basename "$WS2"): $STATS2_TTY"
echo "Shell startup: $(ws_basename "$WS1"): ${SHELL_TIME1}ms  $(ws_basename "$WS2"): ${SHELL_TIME2}ms"
echo "git status:    $(ws_basename "$WS1"): $GIT_TIME1  $(ws_basename "$WS2"): $GIT_TIME2"
