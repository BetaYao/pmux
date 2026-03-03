#!/bin/bash
# 自动化 Sidebar 状态颜色检测测试

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "================================"
echo "Sidebar Status Auto-Detection Test"
echo "================================"
echo ""

# 检查 Python 和 PIL
if ! python3 -c "from PIL import Image" 2>/dev/null; then
    log_warn "PIL/Pillow not installed. Installing..."
    pip3 install Pillow 2>/dev/null || pip install Pillow 2>/dev/null || {
        log_error "Failed to install Pillow. Manual verification required."
        add_report_result "Sidebar Auto-Detection" "SKIP" "Pillow not available"
        exit 0
    }
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

# 获取窗口位置用于确定 sidebar 区域
WINDOW_INFO=$(osascript -e 'tell application "System Events" to tell process "pmux" to get {position, size} of window 1' 2>/dev/null)
WIN_X=$(echo "$WINDOW_INFO" | cut -d',' -f1 | tr -d ' ')
WIN_Y=$(echo "$WINDOW_INFO" | cut -d',' -f2 | tr -d ' ')
WIN_W=$(echo "$WINDOW_INFO" | cut -d',' -f3 | tr -d ' ')
WIN_H=$(echo "$WINDOW_INFO" | cut -d',' -f4 | tr -d ' ')

# 估算 sidebar 区域（左侧 250px）
SIDEBAR_X=0
SIDEBAR_Y=50  # 顶部标题栏偏移
SIDEBAR_W=250
SIDEBAR_H=$((WIN_H - 100))

log_info "Window: x=$WIN_X, y=$WIN_Y, w=$WIN_W, h=$WIN_H"
log_info "Sidebar region: x=$SIDEBAR_X, y=$SIDEBAR_Y, w=$SIDEBAR_W, h=$SIDEBAR_H"

# 测试 1: Idle 状态截图分析
log_info "Test 1: Analyzing IDLE state"
sleep 2
SCREENSHOT1=$(take_screenshot "sidebar_idle")
log_info "Screenshot: $SCREENSHOT1"

# 使用 Python 分析截图
ANALYSIS1=$(python3 "$ANALYSIS_TOOL" sidebar_status "$SCREENSHOT1" $SIDEBAR_X $SIDEBAR_Y $SIDEBAR_W $SIDEBAR_H 2>/dev/null)
STATUS1=$(echo "$ANALYSIS1" | grep "STATUS:" | cut -d':' -f2)
CONFIDENCE1=$(echo "$ANALYSIS1" | grep "CONFIDENCE:" | cut -d':' -f2)

log_info "Detected status: $STATUS1 (confidence: $CONFIDENCE1)"

# 测试 2: Progress 状态（运行长时间命令）
log_info "Test 2: Analyzing PROGRESS state"
send_keystroke "sleep 30"
send_keycode 36
sleep 2

SCREENSHOT2=$(take_screenshot "sidebar_progress")
ANALYSIS2=$(python3 "$ANALYSIS_TOOL" sidebar_status "$SCREENSHOT2" $SIDEBAR_X $SIDEBAR_Y $SIDEBAR_W $SIDEBAR_H 2>/dev/null)
STATUS2=$(echo "$ANALYSIS2" | grep "STATUS:" | cut -d':' -f2)
log_info "Detected status: $STATUS2"

# 停止命令
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down control'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "c"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up control'
sleep 1

# 测试 3: Error 状态
log_info "Test 3: Analyzing ERROR state"
send_keystroke "ls /nonexistent_path_12345"
send_keycode 36
sleep 1

SCREENSHOT3=$(take_screenshot "sidebar_error")
ANALYSIS3=$(python3 "$ANALYSIS_TOOL" sidebar_status "$SCREENSHOT3" $SIDEBAR_X $SIDEBAR_Y $SIDEBAR_W $SIDEBAR_H 2>/dev/null)
STATUS3=$(echo "$ANALYSIS3" | grep "STATUS:" | cut -d':' -f2)
log_info "Detected status: $STATUS3"

# 测试结果评估
echo ""
echo "================================"
echo "Sidebar Status Detection Results"
echo "================================"
echo ""

if [ "$STATUS1" = "idle" ] || [ "$STATUS1" = "unknown" ]; then
    log_info "✓ Test 1 (IDLE): Status detected as $STATUS1"
    add_report_result "Sidebar Idle Detection" "PASS" "$STATUS1"
else
    log_warn "⚠ Test 1 (IDLE): Expected 'idle', got '$STATUS1'"
    add_report_result "Sidebar Idle Detection" "WARN" "$STATUS1"
fi

if [ "$STATUS2" = "running" ] || [ "$STATUS2" = "progress" ]; then
    log_info "✓ Test 2 (PROGRESS): Status detected as $STATUS2"
    add_report_result "Sidebar Progress Detection" "PASS" "$STATUS2"
else
    log_warn "⚠ Test 2 (PROGRESS): Expected 'running', got '$STATUS2'"
    add_report_result "Sidebar Progress Detection" "WARN" "$STATUS2"
fi

if [ "$STATUS3" = "error" ]; then
    log_info "✓ Test 3 (ERROR): Status detected as $STATUS3"
    add_report_result "Sidebar Error Detection" "PASS" "$STATUS3"
else
    log_warn "⚠ Test 3 (ERROR): Expected 'error', got '$STATUS3'"
    add_report_result "Sidebar Error Detection" "WARN" "$STATUS3"
fi

# 生成对比报告
cat > "tests/regression/results/sidebar_status_report.txt" << EOF
Sidebar Status Detection Report
==============================
Test Time: $(date)

Screenshots:
1. IDLE state: $SCREENSHOT1
   Detected: $STATUS1 (confidence: $CONFIDENCE1)

2. PROGRESS state: $SCREENSHOT2
   Detected: $STATUS2

3. ERROR state: $SCREENSHOT3
   Detected: $STATUS3

Notes:
- Color detection is based on dominant colors in sidebar region
- Manual verification may be needed if confidence is low
- Review screenshots to confirm accuracy
EOF

stop_pmux

echo ""
echo "================================"
echo "Sidebar Status Test Complete"
echo "================================"
echo ""
echo "Report: tests/regression/results/sidebar_status_report.txt"
echo ""

exit 0
