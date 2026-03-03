#!/bin/bash
# 自动化测试：终端输入 + Enter 后内容不消失
# 验证键盘输入和回车键 bug 已修复

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PMUX_ROOT="${PMUX_ROOT:-$(cd "$SCRIPT_DIR/../.." && pwd)}"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "================================"
echo "Terminal Input + Enter Test"
echo "================================"
echo ""

# 检查依赖
if ! python3 -c "from PIL import Image" 2>/dev/null; then
    log_warn "PIL/Pillow not available. Skipping."
    exit 0
fi

ANALYSIS_TOOL="$SCRIPT_DIR/lib/image_analysis.py"

# 使用 pmux 仓库自身作为 workspace
REPO_PATH="$PMUX_ROOT"
log_info "Using workspace: $REPO_PATH"

log_info "Step 1: Setup state and start pmux"
mkdir -p "$HOME/.config/pmux"
cat > "$HOME/.config/pmux/state.json" << EOF
{
  "workspaces": ["$REPO_PATH"],
  "active_workspace_index": 0
}
EOF

stop_pmux
sleep 1
start_pmux || { log_error "Failed to start pmux"; exit 1; }
sleep 5
activate_window
sleep 1

# 获取窗口和 terminal 区域
WINDOW_INFO=$(osascript -e 'tell application "System Events" to tell process "pmux" to get {position, size} of window 1' 2>/dev/null)
WIN_X=$(echo "$WINDOW_INFO" | cut -d',' -f1 | tr -d ' ')
WIN_Y=$(echo "$WINDOW_INFO" | cut -d',' -f2 | tr -d ' ')
WIN_W=$(echo "$WINDOW_INFO" | cut -d',' -f3 | tr -d ' ')
WIN_H=$(echo "$WINDOW_INFO" | cut -d',' -f4 | tr -d ' ')

SIDEBAR_W=250
TERM_X=$((SIDEBAR_W + 10))
TERM_Y=80
TERM_W=$((WIN_W - SIDEBAR_W - 20))
TERM_H=$((WIN_H - 150))

log_info "Terminal region: x=$TERM_X, y=$TERM_Y, w=$TERM_W, h=$TERM_H"

# Step 2: 点击 terminal 区域确保焦点
log_info "Step 2: Click terminal area to focus"
click_terminal_area
sleep 0.5

# Step 3: 输入 echo 命令
log_info "Step 3: Type 'echo ENTER_TEST_XYZ'"
send_keystroke "echo ENTER_TEST_XYZ"
sleep 0.5

# 截图：Enter 之前
SCREENSHOT_BEFORE=$(take_screenshot "terminal_before_enter")
ANALYSIS_BEFORE=$(python3 "$ANALYSIS_TOOL" analyze_region "$SCREENSHOT_BEFORE" $TERM_X $TERM_Y $TERM_W $TERM_H 2>/dev/null || echo "VARIANCE:0")
VARIANCE_BEFORE=$(echo "$ANALYSIS_BEFORE" | grep "VARIANCE:" | cut -d':' -f2)
log_info "Variance before Enter: $VARIANCE_BEFORE"

# Step 4: 按 Enter
log_info "Step 4: Press Enter"
send_keycode 36
sleep 2

# 截图：Enter 之后
SCREENSHOT_AFTER=$(take_screenshot "terminal_after_enter")
ANALYSIS_AFTER=$(python3 "$ANALYSIS_TOOL" analyze_region "$SCREENSHOT_AFTER" $TERM_X $TERM_Y $TERM_W $TERM_H 2>/dev/null || echo "VARIANCE:0")
VARIANCE_AFTER=$(echo "$ANALYSIS_AFTER" | grep "VARIANCE:" | cut -d':' -f2)
log_info "Variance after Enter: $VARIANCE_AFTER"

# Step 5: 验证 - 如果 terminal 消失，variance 会大幅下降（变为灰色占位或空白）
# 有内容的 terminal 方差通常 > 100；空白/灰占位 < 50
VARIANCE_NUM=$(echo "$VARIANCE_AFTER" | cut -d'.' -f1)
if [ -z "$VARIANCE_NUM" ]; then
    VARIANCE_NUM=0
fi

if [ "$VARIANCE_NUM" -gt 50 ]; then
    log_info "✓ Terminal has content after Enter (variance=$VARIANCE_AFTER)"
    add_report_result "Terminal After Enter" "PASS" "Variance: $VARIANCE_AFTER"
else
    log_error "✗ Terminal may have disappeared (variance=$VARIANCE_AFTER, expected > 50)"
    add_report_result "Terminal After Enter" "FAIL" "Variance: $VARIANCE_AFTER - terminal likely blank"
fi

# 可选：用 OCR 检查是否有 ENTER_TEST_XYZ（若安装 tesseract）
if command -v tesseract >/dev/null 2>&1; then
    OCR_RESULT=$(python3 "$ANALYSIS_TOOL" ocr "$SCREENSHOT_AFTER" 2>/dev/null || true)
    if echo "$OCR_RESULT" | grep -q "ENTER_TEST_XYZ"; then
        log_info "✓ OCR found 'ENTER_TEST_XYZ' in output"
        add_report_result "OCR Output Verification" "PASS"
    else
        log_warn "OCR did not find 'ENTER_TEST_XYZ' (may be font/quality dependent)"
    fi
fi

# 生成报告
REPORT_FILE="tests/regression/results/terminal_enter_report.txt"
mkdir -p "$(dirname "$REPORT_FILE")"
cat > "$REPORT_FILE" << EOF
Terminal Enter Test Report
==========================
Test Time: $(date)

Screenshots:
- Before Enter: $SCREENSHOT_BEFORE (variance: $VARIANCE_BEFORE)
- After Enter:  $SCREENSHOT_AFTER (variance: $VARIANCE_AFTER)

Result: Variance after Enter = $VARIANCE_AFTER
EOF

stop_pmux

echo ""
echo "================================"
echo "Terminal Enter Test Complete"
echo "================================"
echo ""
echo "Screenshots:"
echo "  Before: $SCREENSHOT_BEFORE"
echo "  After:  $SCREENSHOT_AFTER"
echo ""
echo "Report: $REPORT_FILE"
echo ""

exit 0
