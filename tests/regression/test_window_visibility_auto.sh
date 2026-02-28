#!/bin/bash
# 窗口可见性测试：启动 pmux 后通过截图验证程序窗口正常显示

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "================================"
echo "Window Visibility Test"
echo "================================"
echo ""

# 检查依赖
if ! python3 -c "from PIL import Image" 2>/dev/null; then
    log_warn "PIL/Pillow not available. Skipping window visibility test."
    add_report_result "Window Visibility" "SKIP" "Pillow not installed"
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

# 检查窗口是否存在
WINDOW_INFO=$(osascript -e 'tell application "System Events" to tell process "pmux" to get {position, size} of window 1' 2>/dev/null) || {
    log_error "pmux window not found - process may have crashed or window failed to create"
    add_report_result "Window Visibility" "FAIL" "Window not found"
    stop_pmux
    exit 1
}

WIN_X=$(echo "$WINDOW_INFO" | cut -d',' -f1 | tr -d ' ')
WIN_Y=$(echo "$WINDOW_INFO" | cut -d',' -f2 | tr -d ' ')
WIN_W=$(echo "$WINDOW_INFO" | cut -d',' -f3 | tr -d ' ')
WIN_H=$(echo "$WINDOW_INFO" | cut -d',' -f4 | tr -d ' ')

log_info "Window: x=$WIN_X, y=$WIN_Y, w=$WIN_W, h=$WIN_H"

if [ -z "$WIN_W" ] || [ -z "$WIN_H" ] || [ "$WIN_W" -lt 100 ] || [ "$WIN_H" -lt 100 ]; then
    log_error "Window dimensions invalid (w=$WIN_W, h=$WIN_H)"
    add_report_result "Window Visibility" "FAIL" "Invalid dimensions"
    stop_pmux
    exit 1
fi

# 截图并验证
log_info "Step 2: Take screenshot and verify window content"
SCREENSHOT=$(take_screenshot "window_visibility")
log_info "Screenshot: $SCREENSHOT"

ANALYSIS=$(python3 "$ANALYSIS_TOOL" verify_window "$SCREENSHOT" 400 300 2>/dev/null) || {
    log_error "Image analysis failed"
    add_report_result "Window Visibility" "FAIL" "Analysis error"
    stop_pmux
    exit 1
}

OK=$(echo "$ANALYSIS" | grep "^OK:" | cut -d':' -f2)
REASON=$(echo "$ANALYSIS" | grep "^REASON:" | cut -d':' -f2-)
AVG_BRIGHT=$(echo "$ANALYSIS" | grep "^AVG_BRIGHTNESS:" | cut -d':' -f2)
VARIANCE=$(echo "$ANALYSIS" | grep "^VARIANCE:" | cut -d':' -f2)
WIDTH=$(echo "$ANALYSIS" | grep "^WIDTH:" | cut -d':' -f2)
HEIGHT=$(echo "$ANALYSIS" | grep "^HEIGHT:" | cut -d':' -f2)

log_info "Analysis: ok=$OK, reason=$REASON, brightness=$AVG_BRIGHT, variance=$VARIANCE, size=${WIDTH}x${HEIGHT}"

# 评估结果
echo ""
echo "================================"
echo "Window Visibility Result"
echo "================================"
echo ""

# 生成报告
REPORT_DIR="$SCRIPT_DIR/results"
mkdir -p "$REPORT_DIR"
cat > "$REPORT_DIR/window_visibility_report.txt" << EOF
Window Visibility Report
========================
Test Time: $(date)

Screenshot: $SCREENSHOT
Result: $OK
Reason: $REASON
Avg Brightness: $AVG_BRIGHT
Variance: $VARIANCE
Size: ${WIDTH}x${HEIGHT}

Notes:
- PASS: Window shows dark theme (pmux UI) with visible content
- FAIL reasons: window_too_small, window_too_bright (blank/transparent), window_too_flat (no content)
EOF

if [ "$OK" = "True" ]; then
    log_info "✓ Window is visible and shows normal pmux UI (dark theme, content present)"
    add_report_result "Window Visibility" "PASS" "brightness=$AVG_BRIGHT variance=$VARIANCE"
    stop_pmux
    exit 0
else
    log_error "✗ Window verification failed: $REASON"
    log_error "  (avg_brightness=$AVG_BRIGHT, variance=$VARIANCE, size=${WIDTH}x${HEIGHT})"
    add_report_result "Window Visibility" "FAIL" "reason=$REASON"
    stop_pmux
    exit 1
fi
