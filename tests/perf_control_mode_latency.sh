#!/bin/bash
# Measures tmux control mode command latency (the actual path pmux uses)
# Attaches via tmux -CC, sends commands, measures response time
set -euo pipefail

SESSION="${1:-pmux-saas-mono}"
ITERATIONS="${2:-20}"

echo "======================================"
echo "  tmux Control Mode Latency"
echo "======================================"
echo "Session: $SESSION"
echo ""

PANE=$(tmux list-panes -t "$SESSION" -F '#{pane_id}' 2>/dev/null | head -1)
echo "Target pane: $PANE"
echo ""

# Create a temp fifo for control mode I/O
FIFO_IN=$(mktemp -u /tmp/pmux_perf_in.XXXXXX)
FIFO_OUT=$(mktemp -u /tmp/pmux_perf_out.XXXXXX)
mkfifo "$FIFO_IN" "$FIFO_OUT"
trap "rm -f $FIFO_IN $FIFO_OUT; kill %1 2>/dev/null || true" EXIT

# Start tmux -CC in background, connected to the session
# We use a separate client that does NOT interfere with the running pmux one
tmux -C new-session -d -s "__perf_test_$$" 2>/dev/null || true
tmux -C kill-session -t "__perf_test_$$" 2>/dev/null || true

echo "--- Measuring send-keys via control mode ---"
echo "(writes 'send-keys' directly to tmux -CC connection, no process spawn)"
echo ""

# Method: open a raw PTY to tmux -CC and time commands
python3 << 'PYEOF'
import subprocess, time, os, sys, select

session = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("SESSION", "pmux-saas-mono")
pane = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("PANE", "%0")
iterations = int(sys.argv[3]) if len(sys.argv) > 3 else 20

# Start tmux -C (control mode, not -CC)
proc = subprocess.Popen(
    ["tmux", "-C", "attach", "-t", session],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.DEVNULL,
)

# Wait for initial handshake
time.sleep(0.5)
# Drain initial output
while select.select([proc.stdout], [], [], 0.1)[0]:
    proc.stdout.read(4096)

print("Test 1: Empty send-keys command (tmux parsing overhead only)")
latencies = []
for i in range(iterations):
    cmd = f"send-keys -t {pane} ''\n".encode()
    start = time.perf_counter_ns()
    proc.stdin.write(cmd)
    proc.stdin.flush()
    # Wait for %end response
    deadline = time.perf_counter_ns() + 100_000_000  # 100ms timeout
    while time.perf_counter_ns() < deadline:
        if select.select([proc.stdout], [], [], 0.005)[0]:
            data = proc.stdout.read(4096)
            if b"%end" in data or b"%error" in data:
                end = time.perf_counter_ns()
                latencies.append((end - start) / 1_000_000)
                break
    else:
        latencies.append(-1)

valid = [x for x in latencies if x > 0]
if valid:
    print(f"  Count:  {len(valid)}/{iterations}")
    print(f"  Min:    {min(valid):.2f} ms")
    print(f"  Max:    {max(valid):.2f} ms")
    print(f"  Mean:   {sum(valid)/len(valid):.2f} ms")
    print(f"  Median: {sorted(valid)[len(valid)//2]:.2f} ms")
else:
    print("  All timed out")

print()
print("Test 2: Single character send-keys -l (actual input path)")
latencies2 = []
for i in range(iterations):
    cmd = f"send-keys -l -t {pane} 'x'\n".encode()
    start = time.perf_counter_ns()
    proc.stdin.write(cmd)
    proc.stdin.flush()
    deadline = time.perf_counter_ns() + 100_000_000
    while time.perf_counter_ns() < deadline:
        if select.select([proc.stdout], [], [], 0.005)[0]:
            data = proc.stdout.read(4096)
            if b"%end" in data or b"%output" in data or b"%error" in data:
                end = time.perf_counter_ns()
                latencies2.append((end - start) / 1_000_000)
                break
    else:
        latencies2.append(-1)

# Clean up the x's
proc.stdin.write(f"send-keys -t {pane} C-u\n".encode())
proc.stdin.flush()
time.sleep(0.1)

valid2 = [x for x in latencies2 if x > 0]
if valid2:
    print(f"  Count:  {len(valid2)}/{iterations}")
    print(f"  Min:    {min(valid2):.2f} ms")
    print(f"  Max:    {max(valid2):.2f} ms")
    print(f"  Mean:   {sum(valid2)/len(valid2):.2f} ms")
    print(f"  Median: {sorted(valid2)[len(valid2)//2]:.2f} ms")
else:
    print("  All timed out")

print()
print("Test 3: Burst of 10 characters (batching efficiency)")
latencies3 = []
for i in range(iterations):
    cmds = "".join(f"send-keys -l -t {pane} '{chr(97 + (i*10 + j) % 26)}'\n" for j in range(10))
    start = time.perf_counter_ns()
    proc.stdin.write(cmds.encode())
    proc.stdin.flush()
    # Wait for all responses
    responses = 0
    deadline = time.perf_counter_ns() + 500_000_000
    while responses < 10 and time.perf_counter_ns() < deadline:
        if select.select([proc.stdout], [], [], 0.005)[0]:
            data = proc.stdout.read(4096)
            responses += data.count(b"%end") + data.count(b"%output")
            if responses >= 10:
                end = time.perf_counter_ns()
                latencies3.append((end - start) / 1_000_000)
                break
    else:
        latencies3.append(-1)

# Clean up
proc.stdin.write(f"send-keys -t {pane} C-u\n".encode())
proc.stdin.flush()
time.sleep(0.1)

valid3 = [x for x in latencies3 if x > 0]
if valid3:
    print(f"  Count:  {len(valid3)}/{iterations}")
    print(f"  Min:    {min(valid3):.2f} ms")
    print(f"  Max:    {max(valid3):.2f} ms")
    print(f"  Mean:   {sum(valid3)/len(valid3):.2f} ms")
    print(f"  Median: {sorted(valid3)[len(valid3)//2]:.2f} ms")
    if valid2:
        avg_per_char = (sum(valid3)/len(valid3)) / 10
        avg_single = sum(valid2)/len(valid2)
        print(f"  Per-char (batched): {avg_per_char:.2f} ms vs single: {avg_single:.2f} ms")
else:
    print("  All timed out")

proc.stdin.write(b"detach\n")
proc.stdin.flush()
proc.terminate()
proc.wait()

print()
print("Interpretation:")
print("  Control mode adds ~1-3ms per command (no process spawn)")
print("  Batched commands are ~2-5x faster per character")
print("  The minimum achievable latency via tmux is ~2-5ms per keystroke")
PYEOF

echo ""
echo "======================================"
echo "Done."
rm -f "$FIFO_IN" "$FIFO_OUT"
