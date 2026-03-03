#!/usr/bin/env python3
"""Measure tmux send-keys latency via external command (baseline).

This measures the overhead of the `tmux send-keys` external command,
which is the UPPER BOUND of latency. pmux's control mode path is faster
because it avoids process spawn and socket connection.
"""
import subprocess, time, sys, statistics

session = sys.argv[1] if len(sys.argv) > 1 else "pmux-saas-mono"
iterations = int(sys.argv[2]) if len(sys.argv) > 2 else 20

pane_result = subprocess.run(
    ["tmux", "list-panes", "-t", session, "-F", "#{pane_id}"],
    capture_output=True, text=True
)
pane = pane_result.stdout.strip().split('\n')[0]
print(f"Session: {session}, Pane: {pane}")
print()

# Test 1: External tmux send-keys command latency
print("Test 1: External `tmux send-keys` command (upper bound)")
lats = []
for i in range(iterations):
    start = time.perf_counter()
    subprocess.run(["tmux", "send-keys", "-l", "-t", pane, "x"], capture_output=True)
    end = time.perf_counter()
    lats.append((end - start) * 1000)

subprocess.run(["tmux", "send-keys", "-t", pane, "C-u"], capture_output=True)

print(f"  Min:    {min(lats):.1f} ms")
print(f"  Max:    {max(lats):.1f} ms")
print(f"  Mean:   {statistics.mean(lats):.1f} ms")
print(f"  Median: {statistics.median(lats):.1f} ms")
print(f"  P95:    {sorted(lats)[int(len(lats)*0.95)]:.1f} ms")
print()

# Test 2: Batch 10 chars in one send-keys
print("Test 2: Batch 10 chars in one `tmux send-keys -l` command")
lats2 = []
for i in range(iterations):
    start = time.perf_counter()
    subprocess.run(["tmux", "send-keys", "-l", "-t", pane, "abcdefghij"], capture_output=True)
    end = time.perf_counter()
    lats2.append((end - start) * 1000)

subprocess.run(["tmux", "send-keys", "-t", pane, "C-u"], capture_output=True)

print(f"  Min:    {min(lats2):.1f} ms")
print(f"  Max:    {max(lats2):.1f} ms")
print(f"  Mean:   {statistics.mean(lats2):.1f} ms")
print(f"  Per-char: {statistics.mean(lats2)/10:.1f} ms")
print()

# Test 3: Round-trip (send + capture-pane check)
print("Test 3: Round-trip (send-keys + poll capture-pane for echo)")
rts = []
for i in range(iterations):
    marker = f"__RT{i}__"
    start = time.perf_counter()
    subprocess.run(["tmux", "send-keys", "-t", pane, f"echo {marker}", "Enter"], capture_output=True)
    for _ in range(100):
        result = subprocess.run(["tmux", "capture-pane", "-t", pane, "-p"], capture_output=True, text=True)
        if marker in result.stdout:
            end = time.perf_counter()
            rts.append((end - start) * 1000)
            break
        time.sleep(0.002)
    else:
        rts.append(-1)

valid = [x for x in rts if x > 0]
if valid:
    print(f"  Valid:  {len(valid)}/{iterations}")
    print(f"  Min:    {min(valid):.1f} ms")
    print(f"  Max:    {max(valid):.1f} ms")
    print(f"  Mean:   {statistics.mean(valid):.1f} ms")
    print(f"  Median: {statistics.median(valid):.1f} ms")
else:
    print("  All timed out")

print()
print("Summary:")
print(f"  External tmux command overhead: ~{statistics.median(lats):.0f} ms per keystroke")
print(f"  Control mode (pmux) estimate:   ~{statistics.median(lats)/5:.0f}-{statistics.median(lats)/3:.0f} ms per keystroke")
print(f"  Direct PTY write (Zed-like):    ~0.1-0.5 ms per keystroke")
print()
print("The tmux layer adds unavoidable latency. Options to reduce it:")
print("  1. Batch multiple keystrokes into single send-keys (reduces overhead/char)")
print("  2. Use local PTY backend when session persistence not needed")
print("  3. Optimize the control mode write path (pre-allocate, zero-copy)")
