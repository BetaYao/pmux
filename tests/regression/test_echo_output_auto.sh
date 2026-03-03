#!/bin/bash
# Echo output test: type "echo hello world", press Enter, verify output via OCR

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "================================"
echo "Echo Output Verification Test"
echo "================================"
echo ""

if ! command -v tesseract &>/dev/null; then
    log_warn "tesseract not installed. Skipping echo output test."
    add_report_result "Echo Output" "SKIP" "tesseract not installed"
    exit 0
fi
if ! python3 -c "from PIL import Image" 2>/dev/null; then
    log_warn "PIL/Pillow not available. Skipping echo output test."
    add_report_result "Echo Output" "SKIP" "Pillow not installed"
    exit 0
fi

ANALYSIS_TOOL="$SCRIPT_DIR/lib/image_analysis.py"

log_info "Step 1: Start pmux"
cat > "$HOME/.config/pmux/state.json" << 'EOF'
{
  "workspaces": ["/Users/matt.chow/workspace/saas-mono"],
  "active_workspace_index": 0
}
EOF

stop_pmux
sleep 1
start_pmux || exit 1
sleep 5
activate_window
sleep 1

WINDOW_INFO=$(osascript -e 'tell application "System Events" to tell process "pmux" to get {position, size} of window 1' 2>/dev/null) || {
    log_error "pmux window not found"
    add_report_result "Echo Output" "FAIL" "Window not found"
    stop_pmux
    exit 1
}

WIN_X=$(echo "$WINDOW_INFO" | cut -d',' -f1 | tr -d ' ')
WIN_Y=$(echo "$WINDOW_INFO" | cut -d',' -f2 | tr -d ' ')
WIN_W=$(echo "$WINDOW_INFO" | cut -d',' -f3 | tr -d ' ')
WIN_H=$(echo "$WINDOW_INFO" | cut -d',' -f4 | tr -d ' ')

log_info "Window: x=$WIN_X, y=$WIN_Y, w=$WIN_W, h=$WIN_H"

# Terminal region within the window (window-relative, since screenshot is window-only)
SIDEBAR_W=250
TERM_X=$((SIDEBAR_W + 10))
TERM_Y=80
TERM_W=$((WIN_W - SIDEBAR_W - 20))
TERM_H=$((WIN_H - 150))

log_info "Terminal region (window-relative): x=$TERM_X, y=$TERM_Y, w=$TERM_W, h=$TERM_H"

log_info "Step 2: Send command via tmux (tests output rendering pipeline)"
# Derive tmux session name from workspace path (same logic as pmux uses)
WORKSPACE_DIR="saas-mono"
TMUX_SESSION="pmux-${WORKSPACE_DIR}"

# Find the target pane in the session
TMUX_TARGET=$(tmux list-panes -t "$TMUX_SESSION" -F "#{session_name}:#{window_name}.#{pane_id}" 2>/dev/null | head -1)
if [ -z "$TMUX_TARGET" ]; then
    log_error "Could not find tmux pane in session '$TMUX_SESSION'"
    add_report_result "Echo Output" "FAIL" "No tmux session"
    stop_pmux
    exit 1
fi
log_info "tmux target: $TMUX_TARGET"

# Clear terminal, then run echo command
tmux send-keys -t "$TMUX_TARGET" "clear" Enter
sleep 2
tmux send-keys -t "$TMUX_TARGET" "echo hello world" Enter
sleep 3

log_info "Step 4: Take screenshot"
SCREENSHOT=$(take_screenshot "echo_output")
log_info "Screenshot: $SCREENSHOT"

log_info "Step 5: OCR verification (terminal region only)"
# Retina displays use 2x pixel scaling; crop coordinates must be in pixels
SCALE=2
OCR_X=$((TERM_X * SCALE))
OCR_Y=$((TERM_Y * SCALE))
OCR_W=$((TERM_W * SCALE))
OCR_H=$((TERM_H * SCALE))
log_info "OCR crop (px): x=$OCR_X, y=$OCR_Y, w=$OCR_W, h=$OCR_H"
OCR_RESULT=$(python3 "$ANALYSIS_TOOL" ocr_region "$SCREENSHOT" $OCR_X $OCR_Y $OCR_W $OCR_H 2>/dev/null) || true
OCR_OK=$(echo "$OCR_RESULT" | grep "^OK:" | cut -d':' -f2)
OCR_TEXT=$(echo "$OCR_RESULT" | grep "^TEXT:" | cut -d':' -f2-)

log_info "OCR ok=$OCR_OK"
log_info "OCR text: $OCR_TEXT"

# Generate report
REPORT_DIR="$SCRIPT_DIR/results"
mkdir -p "$REPORT_DIR"
cat > "$REPORT_DIR/echo_output_report.txt" << EOF
Echo Output Verification Report
===============================
Test Time: $(date)

Screenshot: $SCREENSHOT
OCR OK: $OCR_OK
OCR Text: $OCR_TEXT

Expected:
  - "hello world" appears >= 2 times (once in typed command, once in output)
  - "echo" appears >= 1 time (the typed command)
EOF

echo ""
echo "================================"
echo "Echo Output Result"
echo "================================"
echo ""

if [ "$OCR_OK" != "True" ]; then
    log_error "✗ OCR failed: $OCR_TEXT"
    add_report_result "Echo Output" "FAIL" "OCR failed"
    stop_pmux
    exit 1
fi

# Count occurrences (case-insensitive).
# Screen should contain:
#   line 1: "echo hello world"  (the typed command)
#   line 2: "hello world"       (the output)
# So "hello world" >= 2, "echo" >= 1.
HW_COUNT=$(echo "$OCR_TEXT" | grep -oi "hello world" | wc -l | tr -d ' ')
ECHO_COUNT=$(echo "$OCR_TEXT" | grep -oi "echo" | wc -l | tr -d ' ')

log_info "Occurrence counts: 'hello world'=$HW_COUNT, 'echo'=$ECHO_COUNT"

PASS=true
if [ "$HW_COUNT" -lt 2 ]; then
    log_error "✗ Expected 'hello world' >= 2 times (command + output), got $HW_COUNT"
    PASS=false
fi
if [ "$ECHO_COUNT" -lt 1 ]; then
    log_error "✗ Expected 'echo' >= 1 time (the typed command), got $ECHO_COUNT"
    PASS=false
fi

if [ "$PASS" = true ]; then
    log_info "✓ Input+output verified: 'hello world' x$HW_COUNT, 'echo' x$ECHO_COUNT"
    add_report_result "Echo Output" "PASS" "hello world x$HW_COUNT, echo x$ECHO_COUNT"
    stop_pmux
    exit 0
else
    log_error "  OCR text: $OCR_TEXT"
    add_report_result "Echo Output" "FAIL" "hello world x$HW_COUNT, echo x$ECHO_COUNT"
    stop_pmux
    exit 1
fi
