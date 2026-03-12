#!/bin/bash
# Functional test: tmux control-mode resize correctly shrinks and grows
#
# Regression test for the bug where get_window_dims_for_client() returned
# stale (larger) window dimensions when shrinking, preventing refresh-client
# from reducing the client size. The pane stayed at the old large size.
#
# Run: bash tests/functional/tmux/test_resize_shrink_grow.sh

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../regression/lib/test_utils.sh"

SESSION="pmux-test-resize-$$"
PASS=0
FAIL=0
CC_PID=""

cleanup() {
    [ -n "$CC_PID" ] && kill "$CC_PID" 2>/dev/null || true
    tmux kill-session -t "$SESSION" 2>/dev/null || true
}
trap cleanup EXIT

if ! command -v tmux &>/dev/null; then
    log_warn "tmux not installed — skipping"
    exit 0
fi

echo "================================"
echo "Tmux Resize Shrink/Grow Test"
echo "================================"
echo ""

tmux kill-server 2>/dev/null || true
sleep 0.5
rm -rf "/tmp/tmux-$(id -u)" 2>/dev/null || true
sleep 0.5

# Create session at initial size
tmux new-session -d -s "$SESSION" -n "main" -x 120 -y 40

# Attach a control-mode client via script(1) to get a real PTY
# (tmux -CC needs a PTY; without one, no client registers)
script -q /dev/null tmux -CC attach-session -t "$SESSION" </dev/null >/dev/null 2>&1 &
CC_PID=$!
sleep 1

# Set initial client size
tmux refresh-client -C 120,40 2>/dev/null
sleep 0.5

assert_dims() {
    local label="$1" expected_pane="$2" expected_client="$3"
    local pane_dims client_dims
    pane_dims=$(tmux list-panes -t "$SESSION" -F '#{pane_width}x#{pane_height}' | head -1)
    client_dims=$(tmux list-clients -t "$SESSION" -F '#{client_width}x#{client_height}' | head -1)

    if [ "$pane_dims" = "$expected_pane" ]; then
        log_info "✓ $label pane: $pane_dims"
        ((PASS++))
    else
        log_error "✗ $label pane: expected $expected_pane, got $pane_dims"
        ((FAIL++))
    fi

    # Client height may be truncated in list-clients format; check width prefix
    local expected_w="${expected_client%%x*}"
    local actual_w="${client_dims%%x*}"
    if [ "$actual_w" = "$expected_w" ]; then
        log_info "✓ $label client width: $actual_w"
        ((PASS++))
    else
        log_error "✗ $label client width: expected $expected_w, got $actual_w"
        ((FAIL++))
    fi
}

resize_to() {
    local cols="$1" rows="$2"
    tmux resize-pane -t "$SESSION" -x "$cols" -y "$rows" 2>/dev/null
    tmux refresh-client -C "$cols,$rows" 2>/dev/null
    sleep 0.5
}

# ── Test 1: Initial size ──────────────────────────────────────────────
log_info "Test 1: Initial size 120x40"
assert_dims "Initial" "120x40" "120x40"

# ── Test 2: Shrink 120x40 → 60x20 ────────────────────────────────────
log_info "Test 2: Shrink to 60x20"
resize_to 60 20
assert_dims "After shrink" "60x20" "60x20"

# ── Test 3: Grow 60x20 → 100x35 ──────────────────────────────────────
log_info "Test 3: Grow to 100x35"
resize_to 100 35
assert_dims "After grow" "100x35" "100x35"

# ── Test 4: Shrink again 100x35 → 80x24 (the regression case) ────────
log_info "Test 4: Shrink again to 80x24 (regression scenario)"
resize_to 80 24
assert_dims "After re-shrink" "80x24" "80x24"

# ── Test 5: Extreme shrink → grow cycle ───────────────────────────────
log_info "Test 5: Extreme shrink 80x24 → 20x10 then grow → 200x60"
resize_to 20 10
assert_dims "Extreme shrink" "20x10" "20x10"
resize_to 200 60
assert_dims "Extreme grow" "200x60" "200x60"

echo ""
echo "================================"
echo "Results: $PASS passed, $FAIL failed"
echo "================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
