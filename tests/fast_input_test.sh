#!/bin/bash
# Fast input frame-drop test for pmux
#
# Detects rendering frame drops by analyzing tmux %output event timing.
# Uses tmux control mode for both sending (sub-ms injection) and monitoring
# (real-time %output stream) — the SAME path pmux uses.
#
# Tests:
#   1. Single-char echo latency via control mode
#   2. Rapid burst: %output event timing analysis (frame drop detection)
#   3. Rapid capture-pane polling: intermediate state completeness
#   4. Full string verification: final correctness check
#
# Usage: bash tests/fast_input_test.sh

set -euo pipefail

SESSION="__fast_input_test_$$"
cleanup() { tmux kill-session -t "$SESSION" 2>/dev/null || true; }
trap cleanup EXIT

echo "========================================"
echo "  Fast Input Frame-Drop Test"
echo "========================================"
echo ""

# Create isolated session with `cat` for clean echo (no prompt decorations)
tmux new-session -d -s "$SESSION" -x 120 -y 40
sleep 0.3
PANE=$(tmux list-panes -t "$SESSION" -F '#{pane_id}' | head -1)
tmux send-keys -t "$PANE" "cat" Enter
sleep 0.3

echo "Session: $SESSION"
echo "Pane:    $PANE (running 'cat' for clean echo)"
echo ""

# ─── All tests in a single Python process ──────────────────────────────
python3 - "$SESSION" "$PANE" <<'PYEOF'
import os, sys, time, select, subprocess, statistics, collections

session = sys.argv[1]
pane_id = sys.argv[2]

COALESCE_MS = 4        # pmux normal-shell coalescing window
FRAME_BUDGET_MS = 16.7 # 60fps
STUTTER_THRESHOLD_MS = 50  # visible stutter
N_SINGLE = 100         # iterations for single-char test
N_BURST = 200          # chars in burst test
WARMUP = 20
TIMEOUT_S = 0.5

# ─── tmux control mode helper ─────────────────────────────────────────
class TmuxControl:
    def __init__(self, sess):
        self.proc = subprocess.Popen(
            ["tmux", "-C", "attach", "-t", sess],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
        self.fd = self.proc.stdout.fileno()
        self.buf = b""
        time.sleep(0.2)
        self._drain(0.3)

    def close(self):
        try:
            self._send("detach")
        except Exception:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=2)
        except Exception:
            pass

    def _send(self, cmd):
        self.proc.stdin.write((cmd + "\n").encode())
        self.proc.stdin.flush()

    def _drain(self, budget):
        deadline = time.perf_counter() + budget
        while time.perf_counter() < deadline:
            r, _, _ = select.select([self.fd], [], [], 0.01)
            if r:
                chunk = os.read(self.fd, 65536)
                if not chunk:
                    break

    def send_char(self, ch):
        """Send a single literal character via send-keys -l."""
        self._send(f"send-keys -l -t {pane_id} '{ch}'")

    def send_char_hex(self, code):
        """Send a single char by hex code via send-keys -H."""
        self._send(f"send-keys -H -t {pane_id} {code:02x}")

    def send_ctrl(self, key):
        self._send(f"send-keys -t {pane_id} C-{key}")

    def wait_output(self, timeout):
        """Wait for a %output event for our pane. Returns (True, data) or (False, None)."""
        deadline = time.perf_counter() + timeout
        while time.perf_counter() < deadline:
            r, _, _ = select.select([self.fd], [], [], 0.002)
            if not r:
                continue
            chunk = os.read(self.fd, 65536)
            if not chunk:
                return False, None
            self.buf += chunk
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                line = line.rstrip(b"\r")
                if line.startswith(b"%output "):
                    rest = line[len(b"%output "):]
                    parts = rest.split(b" ", 1)
                    if len(parts) >= 1:
                        out_pane = parts[0].decode("utf-8", "ignore")
                        data = parts[1] if len(parts) > 1 else b""
                        if out_pane == pane_id:
                            return True, data
        return False, None

    def collect_outputs(self, timeout, expected_chars=0):
        """Collect all %output events until timeout or expected chars received.
        Returns list of (timestamp_ns, decoded_data_str)."""
        events = []
        total_chars = 0
        deadline = time.perf_counter() + timeout
        while time.perf_counter() < deadline:
            r, _, _ = select.select([self.fd], [], [], 0.001)
            if not r:
                continue
            chunk = os.read(self.fd, 65536)
            if not chunk:
                break
            self.buf += chunk
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                line = line.rstrip(b"\r")
                if line.startswith(b"%output "):
                    ts = time.perf_counter_ns()
                    rest = line[len(b"%output "):]
                    parts = rest.split(b" ", 1)
                    if len(parts) >= 2:
                        out_pane = parts[0].decode("utf-8", "ignore")
                        if out_pane == pane_id:
                            # Unescape tmux control mode encoding
                            raw = parts[1]
                            try:
                                decoded = raw.decode("unicode_escape")
                            except Exception:
                                decoded = raw.decode("utf-8", "replace")
                            events.append((ts, decoded))
                            # Count printable ASCII chars
                            total_chars += sum(1 for c in decoded if 32 <= ord(c) < 127)
                            if expected_chars > 0 and total_chars >= expected_chars:
                                return events
        return events


def pct(vals, p):
    if not vals:
        return 0.0
    s = sorted(vals)
    return s[min(int(len(s) * p), len(s) - 1)]


def capture_pane_content():
    """Get current pane content via capture-pane (external call)."""
    r = subprocess.run(
        ["tmux", "capture-pane", "-t", pane_id, "-p"],
        capture_output=True, text=True, timeout=2,
    )
    return r.stdout


# ═══════════════════════════════════════════════════════════════════════
# Test 1: Single-char echo latency via control mode
# ═══════════════════════════════════════════════════════════════════════
print("--- Test 1: Single-char echo latency (control mode) ---")
print(f"(Send char via CC send-keys → wait for %output echo, {N_SINGLE} iters)")
print()

ctl = TmuxControl(session)

# Warmup
for i in range(WARMUP):
    ctl._drain(0.01)
    ctl.send_char_hex(97 + (i % 26))
    ctl.wait_output(TIMEOUT_S)
ctl.send_ctrl("u")
ctl._drain(0.2)

latencies = []
timeouts = 0
for i in range(N_SINGLE):
    ctl._drain(0.005)
    ch_code = 97 + (i % 26)
    start = time.perf_counter_ns()
    ctl.send_char_hex(ch_code)
    ok, _ = ctl.wait_output(TIMEOUT_S)
    if ok:
        latencies.append((time.perf_counter_ns() - start) / 1_000_000)
    else:
        timeouts += 1

ctl.send_ctrl("u")
ctl._drain(0.1)

if latencies:
    print(f"  Count:    {len(latencies)}/{N_SINGLE}  (timeouts: {timeouts})")
    print(f"  Min:      {min(latencies):.3f} ms")
    print(f"  p50:      {pct(latencies, 0.50):.3f} ms")
    print(f"  p95:      {pct(latencies, 0.95):.3f} ms")
    print(f"  p99:      {pct(latencies, 0.99):.3f} ms")
    print(f"  Max:      {max(latencies):.3f} ms")
    print(f"  Mean:     {statistics.mean(latencies):.3f} ms")
else:
    print("  All timed out!")
print()


# ═══════════════════════════════════════════════════════════════════════
# Test 2: Rapid burst with %output event timing analysis
# ═══════════════════════════════════════════════════════════════════════
print(f"--- Test 2: Rapid burst %output analysis ({N_BURST} chars) ---")
print("(Send all chars at max speed, analyze output event delivery pattern)")
print(f"(Simulates pmux coalescing={COALESCE_MS}ms to predict frame drops)")
print()

ctl.send_ctrl("u")
ctl._drain(0.3)

# Build the test string: alternating different chars (reproduces user's bug pattern)
test_chars = [chr(97 + (i % 26)) for i in range(N_BURST)]

# Send ALL chars as fast as possible
send_times = []
burst_start = time.perf_counter_ns()
for ch in test_chars:
    ctl.send_char(ch)
    send_times.append(time.perf_counter_ns())
burst_send_done = time.perf_counter_ns()

# Collect all %output events
events = ctl.collect_outputs(timeout=3.0, expected_chars=N_BURST)
burst_all_received = time.perf_counter_ns()

send_duration_ms = (burst_send_done - burst_start) / 1_000_000

if not events:
    print("  ERROR: No output events received!")
    print()
else:
    first_event_ts = events[0][0]
    last_event_ts = events[-1][0]
    delivery_duration_ms = (last_event_ts - first_event_ts) / 1_000_000
    total_duration_ms = (burst_all_received - burst_start) / 1_000_000

    # Count total printable chars received
    total_received = 0
    for _, data in events:
        total_received += sum(1 for c in data if 32 <= ord(c) < 127)

    print(f"  Chars sent:           {N_BURST}")
    print(f"  Chars received:       {total_received}")
    print(f"  Output events:        {len(events)}")
    print(f"  Send duration:        {send_duration_ms:.1f} ms ({send_duration_ms/N_BURST:.3f} ms/char)")
    print(f"  Delivery duration:    {delivery_duration_ms:.1f} ms")
    print(f"  Total duration:       {total_duration_ms:.1f} ms")
    print(f"  Avg chars/event:      {total_received/len(events):.1f}")
    print()

    # ── Inter-event gap analysis ──
    gaps = []
    for i in range(1, len(events)):
        gap_ms = (events[i][0] - events[i-1][0]) / 1_000_000
        gaps.append(gap_ms)

    if gaps:
        print("  Inter-event gaps:")
        print(f"    Min:    {min(gaps):.3f} ms")
        print(f"    p50:    {pct(gaps, 0.50):.3f} ms")
        print(f"    p95:    {pct(gaps, 0.95):.3f} ms")
        print(f"    Max:    {max(gaps):.3f} ms")
        print(f"    Mean:   {statistics.mean(gaps):.3f} ms")

        missed_frames = sum(1 for g in gaps if g > FRAME_BUDGET_MS)
        stutter_events = sum(1 for g in gaps if g > STUTTER_THRESHOLD_MS)
        print(f"    Gaps > {FRAME_BUDGET_MS:.1f}ms (missed frame):  {missed_frames}")
        print(f"    Gaps > {STUTTER_THRESHOLD_MS}ms (visible stutter): {stutter_events}")
        print()

    # ── Simulate pmux coalescing windows ──
    # Group events into COALESCE_MS windows to predict what pmux renders per frame.
    # Each window = one render. Count cumulative chars per render vs chars sent.
    print(f"  Coalescing simulation ({COALESCE_MS}ms windows):")

    windows = []  # list of (window_end_ns, cumulative_chars_received)
    cum_chars = 0
    if events:
        window_start = events[0][0]
        window_chars = 0
        for ts, data in events:
            printable = sum(1 for c in data if 32 <= ord(c) < 127)
            if (ts - window_start) / 1_000_000 > COALESCE_MS:
                # Close current window
                cum_chars += window_chars
                windows.append((window_start, cum_chars, window_chars))
                window_start = ts
                window_chars = printable
            else:
                window_chars += printable
        # Close last window
        cum_chars += window_chars
        windows.append((window_start, cum_chars, window_chars))

    print(f"    Render count:       {len(windows)}")
    if windows:
        chars_per_render = [w[2] for w in windows]
        print(f"    Chars per render:   min={min(chars_per_render)} avg={statistics.mean(chars_per_render):.1f} max={max(chars_per_render)}")

    # ── Frame drop analysis: at each render, how many chars are "missing"? ──
    # A char is "missing" if it was sent before this render but hasn't appeared yet.
    print()
    print("  Frame drop analysis:")

    max_deficit = 0
    max_deficit_time_ms = 0
    deficit_events = 0
    for window_start_ns, cum_received, _ in windows:
        # Count how many chars were sent before this window started
        chars_sent_by_now = 0
        for st in send_times:
            if st <= window_start_ns:
                chars_sent_by_now += 1
            else:
                break
        deficit = chars_sent_by_now - cum_received
        if deficit > 0:
            deficit_events += 1
            if deficit > max_deficit:
                max_deficit = deficit
                max_deficit_time_ms = (window_start_ns - burst_start) / 1_000_000

    if deficit_events > 0:
        print(f"    ⚠️  Renders with char deficit: {deficit_events}/{len(windows)}")
        print(f"    ⚠️  Max char deficit:          {max_deficit} chars (at t={max_deficit_time_ms:.1f}ms)")
        print(f"    (A char deficit = chars sent but not yet echoed at render time)")
        print(f"    (This is the \"empty then filled\" pattern the user sees)")
    else:
        print(f"    ✅ No char deficit detected in {len(windows)} renders")

    # ── Per-char echo latency ──
    # For each send, find the first %output that includes that char's echo
    print()
    print("  Per-char echo latency (send → first %output containing echo):")

    # Build cumulative received chars at each event
    char_echo_latencies = []
    cum_at_event = []
    running = 0
    for ts, data in events:
        printable = sum(1 for c in data if 32 <= ord(c) < 127)
        running += printable
        cum_at_event.append((ts, running))

    for i, st in enumerate(send_times):
        # Find first event where cumulative >= i+1
        for ts, cum in cum_at_event:
            if cum >= i + 1:
                lat_ms = (ts - st) / 1_000_000
                char_echo_latencies.append(lat_ms)
                break

    if char_echo_latencies:
        print(f"    Count:  {len(char_echo_latencies)}/{N_BURST}")
        print(f"    p50:    {pct(char_echo_latencies, 0.50):.3f} ms")
        print(f"    p95:    {pct(char_echo_latencies, 0.95):.3f} ms")
        print(f"    Max:    {max(char_echo_latencies):.3f} ms")

        slow_chars = sum(1 for l in char_echo_latencies if l > FRAME_BUDGET_MS)
        very_slow = sum(1 for l in char_echo_latencies if l > STUTTER_THRESHOLD_MS)
        print(f"    > {FRAME_BUDGET_MS:.1f}ms: {slow_chars}  > {STUTTER_THRESHOLD_MS}ms: {very_slow}")

print()


# ═══════════════════════════════════════════════════════════════════════
# Test 3: Rapid capture-pane polling during burst
# ═══════════════════════════════════════════════════════════════════════
print("--- Test 3: Capture-pane intermediate state check ---")
print("(Send 80 chars via CC, immediately poll capture-pane for completeness)")
print()

N_T3 = 80
ctl.send_ctrl("u")
ctl._drain(0.3)

# Send 80 chars as fast as possible
t3_start = time.perf_counter_ns()
for i in range(N_T3):
    ctl.send_char_hex(97 + (i % 26))
t3_send_done = time.perf_counter_ns()

# Immediately poll capture-pane at max speed
captures = []
t3_deadline = time.perf_counter() + 2.0
all_found = False
while time.perf_counter() < t3_deadline:
    content = capture_pane_content()
    ts = time.perf_counter_ns()
    # Find the last non-empty line (cat echoes on current line)
    lines = [l for l in content.strip().split("\n") if l.strip()]
    last_line = lines[-1] if lines else ""
    # Count printable ASCII (skip any control chars)
    n_chars = sum(1 for c in last_line if 32 < ord(c) < 127)
    captures.append((ts, n_chars, last_line))
    if n_chars >= N_T3:
        all_found = True
        break

t3_total_ms = (time.perf_counter_ns() - t3_start) / 1_000_000
t3_send_ms = (t3_send_done - t3_start) / 1_000_000

print(f"  Chars sent:     {N_T3}")
print(f"  Send time:      {t3_send_ms:.1f} ms")
print(f"  Captures taken: {len(captures)}")

if captures:
    final_count = captures[-1][1]
    print(f"  Final count:    {final_count}/{N_T3} {'✅' if final_count >= N_T3 else '⚠️ INCOMPLETE'}")
    print(f"  Total time:     {t3_total_ms:.1f} ms")
    print()

    # Show capture progression
    print("  Capture progression (time_ms, chars_visible/expected):")
    prev_count = -1
    for ts, n, line in captures:
        t_ms = (ts - t3_start) / 1_000_000
        if n != prev_count:  # Only show when count changes
            bar = "█" * min(n, 80) + "░" * max(0, min(N_T3 - n, 80 - min(n, 80)))
            deficit = N_T3 - n if n < N_T3 else 0
            deficit_str = f"  (deficit: {deficit})" if deficit > 0 else ""
            print(f"    {t_ms:7.1f}ms  {n:3d}/{N_T3} {bar}{deficit_str}")
            prev_count = n

    # Check for non-monotonic progress (chars disappearing = rendering bug)
    max_seen = 0
    regressions = 0
    for _, n, _ in captures:
        if n < max_seen:
            regressions += 1
        max_seen = max(max_seen, n)

    if regressions > 0:
        print(f"\n    ⚠️  REGRESSIONS DETECTED: {regressions} captures showed fewer chars than before!")
        print(f"    (This indicates the terminal buffer was overwritten with stale data)")
    else:
        print(f"\n    ✅ Monotonic progress (no regressions)")

print()

# Clean up
ctl.send_ctrl("u")
ctl._drain(0.1)


# ═══════════════════════════════════════════════════════════════════════
# Test 4: Full string correctness verification
# ═══════════════════════════════════════════════════════════════════════
print("--- Test 4: String correctness after burst ---")
print("(Send known string, verify exact content in capture-pane)")
print()

ctl.send_ctrl("u")
ctl._drain(0.3)

expected = "".join(chr(97 + (i % 26)) for i in range(60))

for ch in expected:
    ctl.send_char(ch)
time.sleep(0.5)

content = capture_pane_content()
lines = [l for l in content.strip().split("\n") if l.strip()]
last_line = lines[-1].strip() if lines else ""

if expected in last_line:
    print(f"  ✅ PASS: All {len(expected)} chars correct")
elif last_line:
    # Find first mismatch
    actual_printable = "".join(c for c in last_line if 32 < ord(c) < 127)
    match_len = 0
    for i, (e, a) in enumerate(zip(expected, actual_printable)):
        if e == a:
            match_len += 1
        else:
            break
    print(f"  ⚠️  Mismatch at position {match_len}")
    print(f"    Expected: {expected[:40]}...")
    print(f"    Actual:   {actual_printable[:40]}...")
    # Check for holes (spaces where chars should be)
    holes = sum(1 for i, c in enumerate(actual_printable) if c == ' ' and i < len(expected))
    if holes > 0:
        print(f"    ⚠️  {holes} HOLES (spaces where chars should be) — frame drop evidence!")
else:
    print(f"  ❌ FAIL: No content found")

print()


# ═══════════════════════════════════════════════════════════════════════
# Test 5: Real shell mode — the actual user scenario
# ═══════════════════════════════════════════════════════════════════════
print("--- Test 5: Real shell fast typing (%output event analysis) ---")
print("(Exit cat → shell with minimal PS1 → rapid chars → analyze echo pattern)")
print("(Shell echo includes escape sequences that stress the rendering pipeline)")
print()

# Exit cat, go back to shell
ctl.send_ctrl("c")
ctl._drain(0.5)

# Set minimal prompt to reduce noise, but keep shell behavior
ctl._send(f"send-keys -t {pane_id} 'export PS1=\"$ \"' Enter")
ctl._drain(0.8)
ctl._send(f"send-keys -t {pane_id} 'unset RPROMPT' Enter")
ctl._drain(0.3)

N_SHELL = 80  # chars to send in shell mode

# Clear line
ctl.send_ctrl("u")
ctl._drain(0.3)

# Send chars rapidly in shell mode
shell_send_times = []
shell_burst_start = time.perf_counter_ns()
for i in range(N_SHELL):
    ctl.send_char_hex(97 + (i % 26))
    shell_send_times.append(time.perf_counter_ns())
shell_send_done = time.perf_counter_ns()

# Collect %output events
shell_events = ctl.collect_outputs(timeout=3.0, expected_chars=N_SHELL)
shell_all_received = time.perf_counter_ns()

shell_send_ms = (shell_send_done - shell_burst_start) / 1_000_000
shell_total_ms = (shell_all_received - shell_burst_start) / 1_000_000

if not shell_events:
    print("  ERROR: No output events received!")
else:
    # Count total bytes and printable chars
    total_bytes = sum(len(data.encode("utf-8", "replace")) for _, data in shell_events)
    total_printable = 0
    for _, data in shell_events:
        total_printable += sum(1 for c in data if 32 <= ord(c) < 127)

    delivery_ms = (shell_events[-1][0] - shell_events[0][0]) / 1_000_000 if len(shell_events) > 1 else 0

    print(f"  Chars sent:           {N_SHELL}")
    print(f"  Output events:        {len(shell_events)}")
    print(f"  Total bytes received: {total_bytes}")
    print(f"  Printable chars:      {total_printable}")
    print(f"  Bytes per input char: {total_bytes/N_SHELL:.1f} (>1 = escape sequences)")
    print(f"  Send duration:        {shell_send_ms:.1f} ms")
    print(f"  Delivery duration:    {delivery_ms:.1f} ms")
    print(f"  Total duration:       {shell_total_ms:.1f} ms")
    print()

    # Inter-event gaps
    shell_gaps = []
    for i in range(1, len(shell_events)):
        g = (shell_events[i][0] - shell_events[i-1][0]) / 1_000_000
        shell_gaps.append(g)

    if shell_gaps:
        print("  Inter-event gaps:")
        print(f"    Min:    {min(shell_gaps):.3f} ms")
        print(f"    p50:    {pct(shell_gaps, 0.50):.3f} ms")
        print(f"    p95:    {pct(shell_gaps, 0.95):.3f} ms")
        print(f"    Max:    {max(shell_gaps):.3f} ms")

        missed = sum(1 for g in shell_gaps if g > FRAME_BUDGET_MS)
        stutter = sum(1 for g in shell_gaps if g > STUTTER_THRESHOLD_MS)
        print(f"    Gaps > {FRAME_BUDGET_MS:.1f}ms (missed frame):  {missed}")
        print(f"    Gaps > {STUTTER_THRESHOLD_MS}ms (visible stutter): {stutter}")
        print()

    # Coalescing simulation for shell mode
    shell_windows = []
    cum = 0
    if shell_events:
        ws = shell_events[0][0]
        wc = 0
        wb = 0
        for ts, data in shell_events:
            p = sum(1 for c in data if 32 <= ord(c) < 127)
            b = len(data.encode("utf-8", "replace"))
            if (ts - ws) / 1_000_000 > COALESCE_MS:
                cum += wc
                shell_windows.append((ws, cum, wc, wb))
                ws = ts
                wc = p
                wb = b
            else:
                wc += p
                wb += b
        cum += wc
        shell_windows.append((ws, cum, wc, wb))

    print(f"  Coalescing simulation ({COALESCE_MS}ms windows):")
    print(f"    Render count: {len(shell_windows)}")
    if shell_windows:
        chars_per = [w[2] for w in shell_windows]
        bytes_per = [w[3] for w in shell_windows]
        print(f"    Chars per render: min={min(chars_per)} avg={statistics.mean(chars_per):.1f} max={max(chars_per)}")
        print(f"    Bytes per render: min={min(bytes_per)} avg={statistics.mean(bytes_per):.1f} max={max(bytes_per)}")
        print()

        # Show each render window
        print("    Render windows:")
        for i, (ws, cum, wc, wb) in enumerate(shell_windows):
            t = (ws - shell_burst_start) / 1_000_000
            print(f"      [{i+1:2d}] t={t:7.1f}ms  chars={wc:3d}  bytes={wb:4d}  cumulative={cum:3d}/{N_SHELL}")

    # Frame drop analysis for shell mode
    print()
    print("  Frame drop analysis (shell mode):")
    max_def = 0
    def_renders = 0
    for ws_ns, cum_recv, _, _ in shell_windows:
        sent_by_now = sum(1 for st in shell_send_times if st <= ws_ns)
        deficit = sent_by_now - cum_recv
        if deficit > 0:
            def_renders += 1
            max_def = max(max_def, deficit)

    if def_renders > 0:
        print(f"    ⚠️  Renders with char deficit: {def_renders}/{len(shell_windows)}")
        print(f"    ⚠️  Max char deficit:          {max_def} chars")
        print(f"    (Chars were sent but not yet echoed → user sees 'empty then filled')")
    else:
        print(f"    ✅ No char deficit detected in {len(shell_windows)} renders")

    # Raw event data dump (first 10 events) for debugging
    print()
    print("  Raw output events (first 10, showing escape sequences):")
    for i, (ts, data) in enumerate(shell_events[:10]):
        t = (ts - shell_burst_start) / 1_000_000
        # Show repr to make escape sequences visible
        raw_repr = repr(data)
        if len(raw_repr) > 100:
            raw_repr = raw_repr[:97] + "..."
        print(f"    [{i+1:2d}] t={t:7.1f}ms  {raw_repr}")
    if len(shell_events) > 10:
        print(f"    ... ({len(shell_events) - 10} more events)")

print()

# ── Cleanup ──
ctl.send_ctrl("c")
ctl._drain(0.1)
ctl.close()

print("========================================")
print("  Done.")
print("========================================")
PYEOF
