#!/bin/bash
# Input latency measurement for pmux
# Measures round-trip: send character via tmux → see echo in captured output
# Usage: bash tests/perf_input_latency.sh [session_name] [iterations]

set -euo pipefail

SESSION="${1:-pmux-saas-mono}"
ITERATIONS="${2:-20}"
WINDOW="main"

echo "======================================"
echo "  pmux Input Latency Measurement"
echo "======================================"
echo "Session: $SESSION"
echo "Iterations: $ITERATIONS"
echo ""

# Detect target pane
PANE=$(tmux list-panes -t "$SESSION:$WINDOW" -F '#{pane_id}' 2>/dev/null | head -1)
if [ -z "$PANE" ]; then
    echo "ERROR: No pane found for $SESSION:$WINDOW"
    echo "Available sessions:"
    tmux list-sessions 2>/dev/null || echo "  (no tmux sessions)"
    exit 1
fi
echo "Target pane: $PANE"
echo ""

# Method 1: tmux send-keys round-trip (measures tmux overhead)
echo "--- Method 1: tmux send-keys round-trip ---"
echo "(send unique marker via send-keys, capture-pane to check arrival)"
echo ""

LATENCIES=()
for i in $(seq 1 "$ITERATIONS"); do
    MARKER="__LAT${RANDOM}${i}__"
    
    START_NS=$(python3 -c "import time; print(int(time.time_ns()))")
    
    # Send marker via tmux send-keys (same path as pmux input)
    tmux send-keys -t "$PANE" "echo $MARKER" Enter
    
    # Poll capture-pane until marker appears
    FOUND=0
    for attempt in $(seq 1 200); do
        CONTENT=$(tmux capture-pane -t "$PANE" -p 2>/dev/null || true)
        if echo "$CONTENT" | grep -q "$MARKER"; then
            END_NS=$(python3 -c "import time; print(int(time.time_ns()))")
            ELAPSED_MS=$(python3 -c "print(f'{($END_NS - $START_NS) / 1_000_000:.1f}')")
            LATENCIES+=("$ELAPSED_MS")
            FOUND=1
            break
        fi
        sleep 0.005  # 5ms poll interval
    done
    
    if [ "$FOUND" -eq 0 ]; then
        echo "  [$i] TIMEOUT"
        LATENCIES+=("timeout")
    fi
done

echo "Results (ms):"
VALID=()
for i in "${!LATENCIES[@]}"; do
    lat="${LATENCIES[$i]}"
    idx=$((i + 1))
    if [ "$lat" != "timeout" ]; then
        printf "  [%2d] %s ms\n" "$idx" "$lat"
        VALID+=("$lat")
    else
        printf "  [%2d] TIMEOUT\n" "$idx"
    fi
done

if [ ${#VALID[@]} -gt 0 ]; then
    echo ""
    # Calculate stats with python
    python3 -c "
import statistics
vals = [float(x) for x in '${VALID[*]}'.split()]
print(f'  Count:  {len(vals)}')
print(f'  Min:    {min(vals):.1f} ms')
print(f'  Max:    {max(vals):.1f} ms')
print(f'  Mean:   {statistics.mean(vals):.1f} ms')
print(f'  Median: {statistics.median(vals):.1f} ms')
print(f'  Stdev:  {statistics.stdev(vals):.1f} ms' if len(vals) > 1 else '')
print(f'  P95:    {sorted(vals)[int(len(vals)*0.95)]:.1f} ms' if len(vals) >= 5 else '')
"
fi

echo ""
echo "--- Method 2: Pure tmux command latency ---"
echo "(measures tmux server processing time only)"
echo ""

CMD_LATENCIES=()
for i in $(seq 1 "$ITERATIONS"); do
    START_NS=$(python3 -c "import time; print(int(time.time_ns()))")
    tmux send-keys -t "$PANE" "" 2>/dev/null  # empty send-keys (no-op)
    END_NS=$(python3 -c "import time; print(int(time.time_ns()))")
    ELAPSED_MS=$(python3 -c "print(f'{($END_NS - $START_NS) / 1_000_000:.1f}')")
    CMD_LATENCIES+=("$ELAPSED_MS")
done

python3 -c "
import statistics
vals = [float(x) for x in '${CMD_LATENCIES[*]}'.split()]
print(f'  Count:  {len(vals)}')
print(f'  Min:    {min(vals):.1f} ms')
print(f'  Max:    {max(vals):.1f} ms')
print(f'  Mean:   {statistics.mean(vals):.1f} ms')
print(f'  Median: {statistics.median(vals):.1f} ms')
"

echo ""
echo "--- Method 3: Single character send-keys latency ---"
echo "(measures per-keystroke overhead via tmux control mode)"
echo ""

CHAR_LATENCIES=()
for i in $(seq 1 "$ITERATIONS"); do
    START_NS=$(python3 -c "import time; print(int(time.time_ns()))")
    tmux send-keys -l -t "$PANE" "x"
    END_NS=$(python3 -c "import time; print(int(time.time_ns()))")
    ELAPSED_MS=$(python3 -c "print(f'{($END_NS - $START_NS) / 1_000_000:.1f}')")
    CHAR_LATENCIES+=("$ELAPSED_MS")
done

# Clean up the x's
tmux send-keys -t "$PANE" C-u

python3 -c "
import statistics
vals = [float(x) for x in '${CHAR_LATENCIES[*]}'.split()]
print(f'  Count:  {len(vals)}')
print(f'  Min:    {min(vals):.1f} ms')
print(f'  Max:    {max(vals):.1f} ms')
print(f'  Mean:   {statistics.mean(vals):.1f} ms')
print(f'  Median: {statistics.median(vals):.1f} ms')
print()
print('Interpretation:')
print('  < 5ms  per char: tmux overhead is minimal')
print('  5-15ms per char: tmux overhead is significant')
print('  > 15ms per char: tmux overhead is the bottleneck')
"

echo ""
echo "======================================"
echo "Done."
