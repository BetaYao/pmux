#!/bin/bash
# 功能测试: Terminal 基本命令执行
# 测试常用 shell 命令在 terminal 中的执行

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../../regression/lib/test_utils.sh"

echo "================================"
echo "Terminal Basic Commands Test"
echo "================================"
echo ""

COMMANDS_TO_TEST=(
    "pwd"
    "ls"
    "ls -la"
    "echo hello"
    "clear"
    "cat /etc/passwd | head -5"
    "find . -maxdepth 1 -type f"
)

test_command_execution() {
    local cmd="$1"
    log_info "Testing: $cmd"
    
    send_keystroke "$cmd"
    sleep 0.3
    send_keycode 36  # Return
    sleep 1
    
    # 检查应用是否仍然运行
    if pgrep -f "target/debug/pmux" > /dev/null; then
        return 0
    else
        return 1
    fi
}

setup() {
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
}

teardown() {
    stop_pmux
}

# 主测试流程
setup

PASSED=0
FAILED=0

for cmd in "${COMMANDS_TO_TEST[@]}"; do
    if test_command_execution "$cmd"; then
        log_info "✓ Command passed: $cmd"
        add_report_result "Command: $cmd" "PASS"
        PASSED=$((PASSED + 1))
    else
        log_error "✗ Command failed: $cmd"
        add_report_result "Command: $cmd" "FAIL"
        FAILED=$((FAILED + 1))
    fi
done

# 大输出测试
log_info "Test: Large output handling (1000 lines)"
send_keystroke "seq 1 1000"
send_keycode 36
sleep 3

if pgrep -f "target/debug/pmux" > /dev/null; then
    log_info "✓ Large output handled"
    add_report_result "Large Output" "PASS"
else
    log_error "✗ Crashed on large output"
    add_report_result "Large Output" "FAIL"
fi

teardown

echo ""
echo "================================"
echo "Commands: $PASSED passed, $FAILED failed"
echo "================================"
exit $FAILED
