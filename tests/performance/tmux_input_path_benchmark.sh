#!/bin/bash
# tmux input-path benchmark (no AppleScript).
# Measures control-mode input injection latency and compares against local PTY baseline.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"
PMUX_PID="${PMUX_PID:-}"

ITERATIONS="${1:-220}"
WARMUP="${2:-20}"
TIMEOUT_MS="${3:-250}"
PMUX_UI_ITERS="${4:-20}"
PMUX_UI_TIMEOUT_MS="${5:-3000}"
PROFILE="${6:-relaxed}"

echo "================================"
echo "tmux Input Path Benchmark"
echo "================================"
echo ""

case "$PROFILE" in
    strict)
        THRESH_SINGLE_P50=2.0
        THRESH_SINGLE_P95=6.0
        THRESH_BURST_P50=0.5
        THRESH_RATIO_P50=25.0
        ;;
    relaxed)
        THRESH_SINGLE_P50=8.0
        THRESH_SINGLE_P95=20.0
        THRESH_BURST_P50=3.0
        THRESH_RATIO_P50=60.0
        ;;
    *)
        log_error "Unknown profile: $PROFILE (expected: strict|relaxed)"
        exit 1
        ;;
esac
log_info "Threshold profile: $PROFILE (single_p50<=${THRESH_SINGLE_P50}ms, single_p95<=${THRESH_SINGLE_P95}ms, burst_p50<=${THRESH_BURST_P50}ms, ratio_p50<=${THRESH_RATIO_P50}x)"

if ! command -v tmux >/dev/null 2>&1; then
    log_error "tmux not found; cannot run benchmark"
    exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
    log_error "python3 not found; cannot run benchmark"
    exit 1
fi

SESSION="__pmux_perf_${RANDOM}_$$"
cleanup_session() {
    tmux kill-session -t "$SESSION" >/dev/null 2>&1 || true
}
trap cleanup_session EXIT

log_info "Creating isolated tmux session: $SESSION"
tmux new-session -d -s "$SESSION" -x 120 -y 40
sleep 0.15

PANE="$(tmux list-panes -t "$SESSION" -F '#{pane_id}' | head -1 || true)"
if [ -z "$PANE" ]; then
    log_error "Failed to resolve benchmark pane"
    exit 1
fi
log_info "Benchmark pane: $PANE"

RESULTS_FILE="$PMUX_ROOT/tests/regression/results/tmux_input_path_$(date +%Y%m%d_%H%M%S).csv"
SUMMARY_FILE="$PMUX_ROOT/tests/regression/results/tmux_input_path_report_$(date +%Y%m%d_%H%M%S).md"
mkdir -p "$PMUX_ROOT/tests/regression/results"

PY_OUTPUT="$(
python3 - "$SESSION" "$PANE" "$ITERATIONS" "$WARMUP" "$TIMEOUT_MS" "$THRESH_SINGLE_P50" "$THRESH_SINGLE_P95" "$THRESH_BURST_P50" "$THRESH_RATIO_P50" <<'PYEOF'
import os
import pty
import select
import subprocess
import sys
import time
import tty
from statistics import median

session = sys.argv[1]
pane_id = sys.argv[2]
iterations = int(sys.argv[3])
warmup = int(sys.argv[4])
timeout_ms = int(sys.argv[5])
timeout_s = timeout_ms / 1000.0
th_single_p50 = float(sys.argv[6])
th_single_p95 = float(sys.argv[7])
th_burst_p50 = float(sys.argv[8])
th_ratio_p50 = float(sys.argv[9])

def percentile(vals, p):
    if not vals:
        return 0.0
    s = sorted(vals)
    idx = int((len(s) - 1) * p)
    return s[idx]

def fmt(x):
    return f"{x:.3f}"

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
        self.drain(0.3)

    def close(self):
        try:
            self.send_raw("detach")
            self.proc.stdin.flush()
        except Exception:
            pass
        try:
            self.proc.terminate()
            self.proc.wait(timeout=1.0)
        except Exception:
            pass

    def send_raw(self, cmd: str):
        self.proc.stdin.write((cmd + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def drain(self, budget_s: float):
        deadline = time.perf_counter() + budget_s
        while time.perf_counter() < deadline:
            ready, _, _ = select.select([self.fd], [], [], 0.01)
            if not ready:
                continue
            chunk = os.read(self.fd, 65536)
            if not chunk:
                break
            self.buf += chunk
            if b"\n" in self.buf:
                self.buf = self.buf.split(b"\n")[-1]

    def wait_for_output_event(self, pane: str, timeout: float) -> bool:
        deadline = time.perf_counter() + timeout
        while time.perf_counter() < deadline:
            ready, _, _ = select.select([self.fd], [], [], 0.005)
            if not ready:
                continue
            chunk = os.read(self.fd, 65536)
            if not chunk:
                return False
            self.buf += chunk
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                line = line.rstrip(b"\r")
                # %output %<pane> ...
                if line.startswith(b"%output "):
                    try:
                        rest = line[len(b"%output "):]
                        out_pane = rest.split(b" ", 1)[0].decode("utf-8", "ignore")
                    except Exception:
                        continue
                    if out_pane == pane:
                        return True
        return False

def bench_tmux_control(single_iters, warmup_iters):
    ctl = TmuxControl(session)
    single = []
    timeouts = 0
    burst = []
    burst_timeouts = 0
    burst_chars = 64

    # warmup
    for i in range(warmup_iters):
        ctl.drain(0.02)
        ch = 97 + (i % 26)
        ctl.send_raw(f"send-keys -H -t {pane_id} {ch:02x}")
        ctl.wait_for_output_event(pane_id, timeout_s)

    # single-char latency: command send -> first %output for this pane
    for i in range(single_iters):
        ctl.drain(0.02)
        ch = 97 + (i % 26)
        start = time.perf_counter_ns()
        ctl.send_raw(f"send-keys -H -t {pane_id} {ch:02x}")
        ok = ctl.wait_for_output_event(pane_id, timeout_s)
        if ok:
            single.append((time.perf_counter_ns() - start) / 1_000_000.0)
        else:
            timeouts += 1

    # burst latency: 64 chars in one command; convert to per-char latency
    for i in range(single_iters):
        ctl.drain(0.02)
        payload = " ".join(f"{97 + ((i + j) % 26):02x}" for j in range(burst_chars))
        start = time.perf_counter_ns()
        ctl.send_raw(f"send-keys -H -t {pane_id} {payload}")
        ok = ctl.wait_for_output_event(pane_id, timeout_s)
        if ok:
            total_ms = (time.perf_counter_ns() - start) / 1_000_000.0
            burst.append(total_ms / burst_chars)
        else:
            burst_timeouts += 1

    # cleanup typed chars in cat
    try:
        ctl.send_raw(f"send-keys -t {pane_id} C-u")
    except Exception:
        pass
    ctl.close()
    return single, timeouts, burst, burst_timeouts

def bench_local_pty(single_iters, warmup_iters):
    master_fd, slave_fd = pty.openpty()
    tty.setraw(master_fd)
    proc = subprocess.Popen(
        ["cat"],
        stdin=slave_fd,
        stdout=slave_fd,
        stderr=slave_fd,
        close_fds=True,
    )
    os.close(slave_fd)
    local = []
    timeouts = 0

    for _ in range(warmup_iters):
        os.write(master_fd, b"x")
        select.select([master_fd], [], [], timeout_s)
        try:
            os.read(master_fd, 4096)
        except OSError:
            pass

    for i in range(single_iters):
        ch = bytes([97 + (i % 26)])
        start = time.perf_counter_ns()
        os.write(master_fd, ch)
        ready, _, _ = select.select([master_fd], [], [], timeout_s)
        if not ready:
            timeouts += 1
            continue
        data = os.read(master_fd, 4096)
        if ch in data:
            local.append((time.perf_counter_ns() - start) / 1_000_000.0)
        else:
            # single retry window for scheduler jitter
            ready2, _, _ = select.select([master_fd], [], [], timeout_s)
            if ready2:
                data2 = os.read(master_fd, 4096)
                if ch in data2:
                    local.append((time.perf_counter_ns() - start) / 1_000_000.0)
                    continue
            timeouts += 1

    try:
        proc.terminate()
        proc.wait(timeout=1.0)
    except Exception:
        pass
    try:
        os.close(master_fd)
    except Exception:
        pass
    return local, timeouts

single, single_timeouts, burst_per_char, burst_timeouts = bench_tmux_control(iterations, warmup)
local_single, local_timeouts = bench_local_pty(iterations, warmup)

def summarize(vals):
    if not vals:
        return {
            "count": 0,
            "min": 0.0,
            "max": 0.0,
            "p50": 0.0,
            "p90": 0.0,
            "p95": 0.0,
            "p99": 0.0,
            "mean": 0.0,
        }
    return {
        "count": len(vals),
        "min": min(vals),
        "max": max(vals),
        "p50": percentile(vals, 0.50),
        "p90": percentile(vals, 0.90),
        "p95": percentile(vals, 0.95),
        "p99": percentile(vals, 0.99),
        "mean": sum(vals) / len(vals),
    }

sum_single = summarize(single)
sum_burst = summarize(burst_per_char)
sum_local = summarize(local_single)

ratio_p50 = (sum_single["p50"] / sum_local["p50"]) if sum_local["p50"] > 0 else 0.0
ratio_p95 = (sum_single["p95"] / sum_local["p95"]) if sum_local["p95"] > 0 else 0.0

# thresholds (initial conservative gates)
single_p50_ok = sum_single["p50"] <= th_single_p50
single_p95_ok = sum_single["p95"] <= th_single_p95
burst_p50_ok = sum_burst["p50"] <= th_burst_p50
ratio_ok = ratio_p50 <= th_ratio_p50
timeout_ok = (single_timeouts + burst_timeouts + local_timeouts) == 0

if all([single_p50_ok, single_p95_ok, burst_p50_ok, ratio_ok, timeout_ok]):
    status = "PASS"
elif all([sum_single["count"] > 0, sum_burst["count"] > 0, sum_local["count"] > 0]):
    status = "WARN"
else:
    status = "FAIL"

print(f"STATUS={status}")
print(f"SINGLE_COUNT={sum_single['count']}")
print(f"SINGLE_TIMEOUTS={single_timeouts}")
print(f"SINGLE_MIN_MS={fmt(sum_single['min'])}")
print(f"SINGLE_MEAN_MS={fmt(sum_single['mean'])}")
print(f"SINGLE_P50_MS={fmt(sum_single['p50'])}")
print(f"SINGLE_P90_MS={fmt(sum_single['p90'])}")
print(f"SINGLE_P95_MS={fmt(sum_single['p95'])}")
print(f"SINGLE_P99_MS={fmt(sum_single['p99'])}")
print(f"SINGLE_MAX_MS={fmt(sum_single['max'])}")

print(f"BURST_COUNT={sum_burst['count']}")
print(f"BURST_TIMEOUTS={burst_timeouts}")
print(f"BURST_PER_CHAR_P50_MS={fmt(sum_burst['p50'])}")
print(f"BURST_PER_CHAR_P95_MS={fmt(sum_burst['p95'])}")
print(f"BURST_PER_CHAR_MEAN_MS={fmt(sum_burst['mean'])}")

print(f"LOCAL_COUNT={sum_local['count']}")
print(f"LOCAL_TIMEOUTS={local_timeouts}")
print(f"LOCAL_P50_MS={fmt(sum_local['p50'])}")
print(f"LOCAL_P95_MS={fmt(sum_local['p95'])}")

print(f"RATIO_P50={fmt(ratio_p50)}")
print(f"RATIO_P95={fmt(ratio_p95)}")
PYEOF
)"

eval "$PY_OUTPUT"

echo ""
echo "Single-char (tmux control-mode):"
echo "  count=$SINGLE_COUNT timeout=$SINGLE_TIMEOUTS p50=${SINGLE_P50_MS}ms p95=${SINGLE_P95_MS}ms p99=${SINGLE_P99_MS}ms"
echo "Burst per-char (tmux control-mode):"
echo "  count=$BURST_COUNT timeout=$BURST_TIMEOUTS p50=${BURST_PER_CHAR_P50_MS}ms p95=${BURST_PER_CHAR_P95_MS}ms"
echo "Local PTY baseline:"
echo "  count=$LOCAL_COUNT timeout=$LOCAL_TIMEOUTS p50=${LOCAL_P50_MS}ms p95=${LOCAL_P95_MS}ms"
echo "Ratios:"
echo "  tmux/local p50=${RATIO_P50}x p95=${RATIO_P95}x"
echo "Status: $STATUS"
echo ""

PMUX_UI_STATUS="SKIP"
PMUX_UI_COUNT=0
PMUX_UI_TIMEOUTS=0
PMUX_UI_P50_MS=0.000
PMUX_UI_P95_MS=0.000
PMUX_UI_MEAN_MS=0.000
PMUX_UI_MIN_MS=0.000
PMUX_UI_MAX_MS=0.000

# Optional: pmux-in-the-loop E2E probe.
# This measures: AppleScript keystroke -> GPUI/input path -> runtime.send_input -> tmux/shell -> capture-pane visible.
if [ -x "${PMUX_BIN:-$PMUX_ROOT/target/debug/pmux}" ] && [ -f "$PMUX_ROOT/target/debug/pmux" ]; then
    log_info "Running pmux UI-in-the-loop E2E probe (${PMUX_UI_ITERS} iterations)"
    backup_config

    WORKSPACE_PATH="/Users/matt.chow/workspace/saas-mono"
    if [ ! -d "$WORKSPACE_PATH" ]; then
        WORKSPACE_PATH="$PMUX_ROOT"
    fi
    WORKSPACE_NAME="$(basename "$WORKSPACE_PATH")"
    TMUX_SESSION="pmux-${WORKSPACE_NAME}"

    cat > "$PMUX_CONFIG_DIR/config.json" <<EOF
{
  "workspace_paths": ["$WORKSPACE_PATH"],
  "active_workspace_index": 0
}
EOF

    stop_pmux
    sleep 1
    start_pmux >/dev/null 2>&1 || true
    sleep 4
    activate_window >/dev/null 2>&1 || true
    ensure_english_input >/dev/null 2>&1 || true
    sleep 1
    click_terminal_area >/dev/null 2>&1 || true

    TMUX_TARGET=""
    for attempt in $(seq 1 12); do
        TMUX_TARGET=$(tmux list-panes -t "$TMUX_SESSION" -F "#{session_name}:#{window_name}.#{pane_id}" 2>/dev/null | head -1 || true)
        if [ -n "$TMUX_TARGET" ]; then
            break
        fi
        sleep 1
    done

    if [ -n "$TMUX_TARGET" ]; then
        # reset screen
        send_keystroke "clear" >/dev/null 2>&1 || true
        send_keycode 36 >/dev/null 2>&1 || true
        sleep 0.6

        PMUX_UI_OUTPUT="$(
python3 - "$TMUX_TARGET" "$PMUX_UI_ITERS" "$PMUX_UI_TIMEOUT_MS" <<'PYEOF'
import subprocess
import sys
import time

target = sys.argv[1]
iters = int(sys.argv[2])
timeout_ms = int(sys.argv[3])
timeout_s = timeout_ms / 1000.0

def percentile(vals, p):
    if not vals:
        return 0.0
    s = sorted(vals)
    idx = int((len(s) - 1) * p)
    return s[idx]

lat = []
timeouts = 0
for i in range(iters):
    marker = f"PMUX_E2E_{int(time.time()*1000)}_{i}"
    # Send plain marker via pmux UI input path (no spaces, no shell execution).
    cmd1 = f'tell application "System Events" to tell process "pmux" to keystroke "{marker}"'
    start = time.perf_counter_ns()
    subprocess.run(["osascript", "-e", cmd1], capture_output=True)

    found = False
    deadline = time.perf_counter() + timeout_s
    while time.perf_counter() < deadline:
        out = subprocess.run(
            ["tmux", "capture-pane", "-t", target, "-p"],
            capture_output=True,
            text=True,
        )
        if marker in out.stdout:
            found = True
            break
        time.sleep(0.01)

    if found:
        lat.append((time.perf_counter_ns() - start) / 1_000_000.0)
        # Clear current shell input line (Ctrl+U) so next marker is independent.
        subprocess.run(
            ["osascript", "-e", 'tell application "System Events" to tell process "pmux" to key down control'],
            capture_output=True,
        )
        subprocess.run(
            ["osascript", "-e", 'tell application "System Events" to tell process "pmux" to keystroke "u"'],
            capture_output=True,
        )
        subprocess.run(
            ["osascript", "-e", 'tell application "System Events" to tell process "pmux" to key up control'],
            capture_output=True,
        )
    else:
        timeouts += 1

if lat:
    print(f"PMUX_UI_COUNT={len(lat)}")
    print(f"PMUX_UI_TIMEOUTS={timeouts}")
    print(f"PMUX_UI_MIN_MS={min(lat):.3f}")
    print(f"PMUX_UI_MEAN_MS={sum(lat)/len(lat):.3f}")
    print(f"PMUX_UI_P50_MS={percentile(lat, 0.5):.3f}")
    print(f"PMUX_UI_P95_MS={percentile(lat, 0.95):.3f}")
    print(f"PMUX_UI_MAX_MS={max(lat):.3f}")
else:
    print("PMUX_UI_COUNT=0")
    print(f"PMUX_UI_TIMEOUTS={timeouts}")
    print("PMUX_UI_MIN_MS=0.000")
    print("PMUX_UI_MEAN_MS=0.000")
    print("PMUX_UI_P50_MS=0.000")
    print("PMUX_UI_P95_MS=0.000")
    print("PMUX_UI_MAX_MS=0.000")
PYEOF
)"
        eval "$PMUX_UI_OUTPUT"
        if [ "$PMUX_UI_COUNT" -gt 0 ]; then
            PMUX_UI_STATUS="INFO"
        else
            PMUX_UI_STATUS="WARN"
        fi
    else
        PMUX_UI_STATUS="SKIP"
    fi

    stop_pmux >/dev/null 2>&1 || true
    restore_config
fi

cat > "$RESULTS_FILE" <<EOF
Metric,Value,Unit,Status
profile,${PROFILE},name,INFO
tmux_single_p50,${SINGLE_P50_MS},ms,$([ "$(echo "$SINGLE_P50_MS <= $THRESH_SINGLE_P50" | bc -l)" -eq 1 ] && echo PASS || echo WARN)
tmux_single_p95,${SINGLE_P95_MS},ms,$([ "$(echo "$SINGLE_P95_MS <= $THRESH_SINGLE_P95" | bc -l)" -eq 1 ] && echo PASS || echo WARN)
tmux_burst_per_char_p50,${BURST_PER_CHAR_P50_MS},ms,$([ "$(echo "$BURST_PER_CHAR_P50_MS <= $THRESH_BURST_P50" | bc -l)" -eq 1 ] && echo PASS || echo WARN)
local_single_p50,${LOCAL_P50_MS},ms,INFO
ratio_p50,${RATIO_P50},x,$([ "$(echo "$RATIO_P50 <= $THRESH_RATIO_P50" | bc -l)" -eq 1 ] && echo PASS || echo WARN)
timeouts_total,$((SINGLE_TIMEOUTS + BURST_TIMEOUTS + LOCAL_TIMEOUTS)),count,$([ $((SINGLE_TIMEOUTS + BURST_TIMEOUTS + LOCAL_TIMEOUTS)) -eq 0 ] && echo PASS || echo WARN)
pmux_ui_e2e_p50,${PMUX_UI_P50_MS},ms,${PMUX_UI_STATUS}
pmux_ui_e2e_p95,${PMUX_UI_P95_MS},ms,${PMUX_UI_STATUS}
pmux_ui_e2e_timeouts,${PMUX_UI_TIMEOUTS},count,${PMUX_UI_STATUS}
EOF

cat > "$SUMMARY_FILE" <<EOF
# tmux Input Path Benchmark Report

**Date:** $(date)  
**Session:** \`$SESSION\` (ephemeral)  
**Iterations:** $ITERATIONS (warmup: $WARMUP)  
**Timeout:** ${TIMEOUT_MS}ms  
**Threshold Profile:** \`$PROFILE\`

## Results

| Metric | Value | Threshold | Status |
|---|---:|---:|---|
| tmux single p50 | ${SINGLE_P50_MS} ms | <= ${THRESH_SINGLE_P50} ms | $([ "$(echo "$SINGLE_P50_MS <= $THRESH_SINGLE_P50" | bc -l)" -eq 1 ] && echo PASS || echo WARN) |
| tmux single p95 | ${SINGLE_P95_MS} ms | <= ${THRESH_SINGLE_P95} ms | $([ "$(echo "$SINGLE_P95_MS <= $THRESH_SINGLE_P95" | bc -l)" -eq 1 ] && echo PASS || echo WARN) |
| tmux burst per-char p50 | ${BURST_PER_CHAR_P50_MS} ms | <= ${THRESH_BURST_P50} ms | $([ "$(echo "$BURST_PER_CHAR_P50_MS <= $THRESH_BURST_P50" | bc -l)" -eq 1 ] && echo PASS || echo WARN) |
| local PTY single p50 | ${LOCAL_P50_MS} ms | baseline | INFO |
| ratio p50 (tmux/local) | ${RATIO_P50} x | <= ${THRESH_RATIO_P50} x | $([ "$(echo "$RATIO_P50 <= $THRESH_RATIO_P50" | bc -l)" -eq 1 ] && echo PASS || echo WARN) |
| total timeouts | $((SINGLE_TIMEOUTS + BURST_TIMEOUTS + LOCAL_TIMEOUTS)) | 0 | $([ $((SINGLE_TIMEOUTS + BURST_TIMEOUTS + LOCAL_TIMEOUTS)) -eq 0 ] && echo PASS || echo WARN) |

## Distribution Detail

- tmux single: min=${SINGLE_MIN_MS}ms mean=${SINGLE_MEAN_MS}ms p50=${SINGLE_P50_MS}ms p90=${SINGLE_P90_MS}ms p95=${SINGLE_P95_MS}ms p99=${SINGLE_P99_MS}ms max=${SINGLE_MAX_MS}ms
- tmux burst per-char: mean=${BURST_PER_CHAR_MEAN_MS}ms p50=${BURST_PER_CHAR_P50_MS}ms p95=${BURST_PER_CHAR_P95_MS}ms
- local single: p50=${LOCAL_P50_MS}ms p95=${LOCAL_P95_MS}ms
- ratio: p50=${RATIO_P50}x p95=${RATIO_P95}x
- pmux UI E2E (automation-included): count=${PMUX_UI_COUNT} timeout=${PMUX_UI_TIMEOUTS} min=${PMUX_UI_MIN_MS}ms mean=${PMUX_UI_MEAN_MS}ms p50=${PMUX_UI_P50_MS}ms p95=${PMUX_UI_P95_MS}ms max=${PMUX_UI_MAX_MS}ms

## Verdict

**$STATUS**

## Notes

- pmux UI E2E 指标包含 AppleScript 自动化开销与 shell 执行时间，主要用于趋势观测（同机同环境对比）。
- tmux control-mode 指标不含 GUI 自动化，主要用于锁定输入注入链路回归。
EOF

if [ -n "${REPORT_FILE:-}" ]; then
    add_report_result "tmux Input Path Benchmark" "$STATUS" "p50=${SINGLE_P50_MS}ms, p95=${SINGLE_P95_MS}ms, ratio=${RATIO_P50}x"
fi

log_info "CSV saved: $RESULTS_FILE"
log_info "Markdown report: $SUMMARY_FILE"

if [ "$STATUS" = "FAIL" ]; then
    exit 1
fi

exit 0
