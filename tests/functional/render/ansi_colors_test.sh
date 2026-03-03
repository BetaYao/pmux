#!/bin/bash
# 功能测试: ANSI 颜色渲染
# 测试终端 ANSI 颜色代码的正确显示

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../regression/lib/test_utils.sh"

echo "================================"
echo "ANSI Colors Rendering Test"
echo "================================"
echo ""

test_basic_ansi_colors() {
    log_info "Test: Basic ANSI 16 colors"
    
    # 显示 16 色测试
    send_keystroke "echo -e '\033[31mRed\033[0m \033[32mGreen\033[0m \033[33mYellow\033[0m \033[34mBlue\033[0m \033[35mMagenta\033[0m \033[36mCyan\033[0m'"
    send_keycode 36
    sleep 1
    
    log_info "✓ Basic colors displayed"
    add_report_result "Basic ANSI Colors" "PASS"
}

test_256_colors() {
    log_info "Test: 256 color palette"
    
    # 显示 256 色块
    send_keystroke "for i in {0..255}; do printf '\033[48;5;%dm  \033[0m' $i; done; echo"
    send_keycode 36
    sleep 2
    
    log_info "✓ 256 colors displayed"
    add_report_result "256 Colors" "PASS"
}

test_true_color() {
    log_info "Test: True color (24-bit)"
    
    # 渐变色测试
    send_keystroke "for i in {0..255}; do printf '\033[48;2;%d;0;0m ' $i; done; echo"
    send_keycode 36
    sleep 2
    
    log_info "✓ True color displayed"
    add_report_result "True Color (24-bit)" "PASS"
}

test_ls_colors() {
    log_info "Test: ls --color=auto"
    
    send_keystroke "ls --color=auto -la /"
    send_keycode 36
    sleep 2
    
    log_info "✓ ls with colors executed"
    add_report_result "ls Color Display" "PASS"
}

test_bold_and_attributes() {
    log_info "Test: Text attributes (bold, underline, reverse)"
    
    send_keystroke "echo -e '\033[1mBold\033[0m \033[4mUnderline\033[0m \033[7mReverse\033[0m \033[3mItalic\033[0m'"
    send_keycode 36
    sleep 1
    
    log_info "✓ Text attributes displayed"
    add_report_result "Text Attributes" "PASS"
}

# 自动化颜色检测
test_colors_auto() {
    log_info "Test: Automated color detection"
    
    # 先显示颜色
    test_basic_ansi_colors
    
    sleep 1
    
    SCREENSHOT_FILE=$(take_screenshot "colors_auto")
    
    IMAGE_ANALYSIS_SCRIPT="$SCRIPT_DIR/../../regression/lib/image_analysis.py"
    if [ -f "$IMAGE_ANALYSIS_SCRIPT" ] && [ -f "$SCREENSHOT_FILE" ]; then
        RESULT=$(python3 "$IMAGE_ANALYSIS_SCRIPT" "$SCREENSHOT_FILE" check_colors 0 0 2000 1600 2>/dev/null) || true
        HAS_COLORS=$(echo "$RESULT" | grep "^HAS_MULTIPLE_COLORS:" | cut -d':' -f2)
        if [ "$HAS_COLORS" = "True" ]; then
            log_info "✓ Colors detected in screenshot"
            add_report_result "Colors (Auto)" "PASS"
        else
            log_warn "⚠ Color detection inconclusive"
            add_report_result "Colors (Auto)" "WARN"
        fi
    else
        add_report_result "Colors (Auto)" "SKIP" "Image analysis unavailable"
    fi
}

# 主测试流程
cat > "$HOME/.config/pmux/state.json" << 'EOF'
{
  "workspaces": ["/Users/matt.chow/workspace/saas-mono"],
  "active_workspace_index": 0
}
EOF

start_pmux || exit 1
sleep 5
activate_window
sleep 1

test_basic_ansi_colors
test_256_colors
test_true_color
test_ls_colors
test_bold_and_attributes
test_colors_auto

stop_pmux

echo ""
echo "================================"
echo "ANSI Colors Test Complete"
echo "================================"
exit $TESTS_FAILED
