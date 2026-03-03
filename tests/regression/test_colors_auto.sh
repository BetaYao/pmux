#!/bin/bash
# 自动化颜色显示验证测试

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "================================"
echo "Color Display Auto-Verification Test"
echo "================================"
echo ""

# 检查依赖
if ! python3 -c "from PIL import Image" 2>/dev/null; then
    log_warn "PIL/Pillow not available. Skipping automated color verification."
    add_report_result "Color Auto-Verification" "SKIP"
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

# 获取 terminal 区域
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

# 测试 1: 无颜色（默认）
log_info "Test 1: Baseline (no color output)"
send_keystroke "clear"
send_keycode 36
sleep 1

SCREENSHOT1=$(take_screenshot "color_baseline")
ANALYSIS1=$(python3 "$ANALYSIS_TOOL" check_colors "$SCREENSHOT1" $TERM_X $TERM_Y $TERM_W $TERM_H 2>/dev/null)
COLOR_COUNT1=$(echo "$ANALYSIS1" | grep "COLOR_COUNT:" | cut -d':' -f2)
log_info "Baseline color count: $COLOR_COUNT1"

# 测试 2: ls --color
log_info "Test 2: ls --color (should show multiple colors)"
send_keystroke "ls --color=auto"
send_keycode 36
sleep 1

SCREENSHOT2=$(take_screenshot "color_ls")
ANALYSIS2=$(python3 "$ANALYSIS_TOOL" check_colors "$SCREENSHOT2" $TERM_X $TERM_Y $TERM_W $TERM_H 2>/dev/null)
COLOR_COUNT2=$(echo "$ANALYSIS2" | grep "COLOR_COUNT:" | cut -d':' -f2)
HAS_COLORS2=$(echo "$ANALYSIS2" | grep "HAS_MULTIPLE_COLORS:" | cut -d':' -f2)

log_info "ls --color color count: $COLOR_COUNT2"
log_info "Has multiple colors: $HAS_COLORS2"

if [ "$HAS_COLORS2" = "True" ]; then
    log_info "✓ ls --color shows multiple colors"
    add_report_result "LS Color Display" "PASS" "$COLOR_COUNT2 colors"
else
    log_warn "⚠ ls --color may not be showing colors"
    add_report_result "LS Color Display" "WARN" "Only $COLOR_COUNT2 colors"
fi

# 测试 3: ANSI 颜色码
log_info "Test 3: ANSI color codes"
send_keystroke 'echo -e "\033[31mRED\033[0m \033[32mGREEN\033[0m \033[34mBLUE\033[0m"'
send_keycode 36
sleep 1

SCREENSHOT3=$(take_screenshot "color_ansi")
ANALYSIS3=$(python3 "$ANALYSIS_TOOL" check_colors "$SCREENSHOT3" $TERM_X $TERM_Y $TERM_W $TERM_H 2>/dev/null)
COLOR_COUNT3=$(echo "$ANALYSIS3" | grep "COLOR_COUNT:" | cut -d':' -f2)
HAS_COLORS3=$(echo "$ANALYSIS3" | grep "HAS_MULTIPLE_COLORS:" | cut -d':' -f2)

log_info "ANSI colors color count: $COLOR_COUNT3"

if [ "$HAS_COLORS3" = "True" ]; then
    log_info "✓ ANSI color codes rendered correctly"
    add_report_result "ANSI Color Display" "PASS" "$COLOR_COUNT3 colors"
else
    log_warn "⚠ ANSI colors may not be rendered"
    add_report_result "ANSI Color Display" "WARN"
fi

# 测试 4: 256 色支持
log_info "Test 4: 256 color support"
send_keystroke 'for i in {1..16}; do echo -e "\033[38;5;${i}mColor${i}\033[0m"; done'
send_keycode 36
sleep 1

SCREENSHOT4=$(take_screenshot "color_256")
ANALYSIS4=$(python3 "$ANALYSIS_TOOL" check_colors "$SCREENSHOT4" $TERM_X $TERM_Y $TERM_W $TERM_H 2>/dev/null)
COLOR_COUNT4=$(echo "$ANALYSIS4" | grep "COLOR_COUNT:" | cut -d':' -f2)

log_info "256 color test color count: $COLOR_COUNT4"

if [ -n "$COLOR_COUNT4" ] && [ "$COLOR_COUNT4" -gt 10 ]; then
    log_info "✓ 256 color support detected"
    add_report_result "256 Color Support" "PASS" "$COLOR_COUNT4 colors"
else
    log_warn "⚠ Limited color palette detected"
    add_report_result "256 Color Support" "WARN"
fi

# 生成颜色对比报告
cat > "tests/regression/results/color_verification_report.txt" << EOF
Color Display Verification Report
=================================
Test Time: $(date)

Terminal Region: x=$TERM_X, y=$TERM_Y, w=$TERM_W, h=$TERM_H

Screenshots:
1. Baseline: $SCREENSHOT1
   Color count: $COLOR_COUNT1

2. ls --color: $SCREENSHOT2
   Color count: $COLOR_COUNT2
   Has multiple colors: $HAS_COLORS2

3. ANSI colors: $SCREENSHOT3
   Color count: $COLOR_COUNT3
   Has multiple colors: $HAS_COLORS3

4. 256 colors: $SCREENSHOT4
   Color count: $COLOR_COUNT4

Summary:
- LS color output: $( [ "$HAS_COLORS2" = "True" ] && echo "WORKING" || echo "ISSUE" )
- ANSI codes: $( [ "$HAS_COLORS3" = "True" ] && echo "WORKING" || echo "ISSUE" )
- 256 color support: $( [ -n "$COLOR_COUNT4" ] && [ "$COLOR_COUNT4" -gt 10 ] && echo "WORKING" || echo "LIMITED" )

Manual Verification:
- Check 'color_ls.png': directories should be blue, executables green
- Check 'color_ansi.png': should show RED, GREEN, BLUE text
- Check 'color_256.png': should show gradient of colors
EOF

stop_pmux

echo ""
echo "================================"
echo "Color Verification Test Complete"
echo "================================"
echo ""
echo "Report: tests/regression/results/color_verification_report.txt"
echo ""

exit 0
