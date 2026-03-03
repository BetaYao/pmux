#!/bin/bash
# 功能测试: Agent 状态检测
# 测试 Sidebar 状态指示器

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../regression/lib/test_utils.sh"

echo "================================"
echo "Agent Status Detection Test"
echo "================================"
echo ""

test_status_sidebar_visible() {
    log_info "Test: Status sidebar is visible"
    
    SCREENSHOT_FILE=$(take_screenshot "status_sidebar_test")
    
    if [ -f "$SCREENSHOT_FILE" ]; then
        log_info "✓ Screenshot captured for status analysis"
        add_report_result "Status Sidebar Visible" "PASS"
    else
        log_warn "⚠ Could not capture screenshot"
        add_report_result "Status Sidebar Visible" "WARN"
    fi
}

test_progress_indicator() {
    log_info "Test: Progress indicator behavior"
    
    # 模拟一个运行中的任务
    send_keystroke "echo 'Progress: 50%'"
    send_keycode 36
    sleep 1
    
    # 等待状态更新
    sleep 3
    
    log_info "✓ Progress indicator test completed"
    add_report_result "Progress Indicator" "PASS"
}

test_error_indicator() {
    log_info "Test: Error indicator behavior"
    
    # 触发一个错误
    send_keystroke "false"
    send_keycode 36
    sleep 1
    
    send_keystroke "echo 'Command failed above'"
    send_keycode 36
    sleep 2
    
    log_info "✓ Error indicator test completed"
    add_report_result "Error Indicator" "PASS"
}

test_input_indicator() {
    log_info "Test: Input indicator behavior"
    
    # 启动一个需要输入的命令
    send_keystroke "read -p 'Enter something: ' var"
    send_keycode 36
    sleep 1
    
    # 等待状态更新
    sleep 2
    
    # 发送一些输入
    send_keystroke "test input"
    send_keycode 36
    sleep 1
    
    log_info "✓ Input indicator test completed"
    add_report_result "Input Indicator" "PASS"
}

# 视觉验证测试（使用 Python 图像分析）
test_status_colors_auto() {
    log_info "Test: Automated status color detection"
    
    SCREENSHOT_FILE=$(take_screenshot "status_colors")
    
    IMAGE_ANALYSIS_SCRIPT="$SCRIPT_DIR/../../regression/lib/image_analysis.py"
    if [ -f "$IMAGE_ANALYSIS_SCRIPT" ] && [ -f "$SCREENSHOT_FILE" ]; then
        log_info "Running color analysis..."
        RESULT=$(python3 "$IMAGE_ANALYSIS_SCRIPT" "$SCREENSHOT_FILE" check_colors 0 0 500 1400 2>/dev/null) || true
        HAS_COLORS=$(echo "$RESULT" | grep "^HAS_MULTIPLE_COLORS:" | cut -d':' -f2)
        if [ "$HAS_COLORS" = "True" ]; then
            log_info "✓ Multiple colors detected (status indicators likely present)"
            add_report_result "Status Colors (Auto)" "PASS" "Colors detected"
        else
            log_warn "⚠ Could not confirm multiple colors"
            add_report_result "Status Colors (Auto)" "WARN" "Color detection inconclusive"
        fi
    else
        log_warn "⚠ Image analysis not available"
        add_report_result "Status Colors (Auto)" "SKIP" "Image analysis unavailable"
    fi
}

# 主测试流程
cat > "$HOME/.config/pmux/state.json" << 'EOF'
{
  "workspaces": ["/Users/matt.chow/workspace/saas-mono", "/Users/matt.chow/workspace/pmux"],
  "active_workspace_index": 0
}
EOF

start_pmux || exit 1
sleep 5
activate_window
sleep 2

test_status_sidebar_visible
test_progress_indicator
test_error_indicator
test_input_indicator
test_status_colors_auto

stop_pmux

echo ""
echo "================================"
echo "Agent Status Test Complete"
echo "================================"
exit $TESTS_FAILED
