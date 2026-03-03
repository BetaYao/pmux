#!/bin/bash
# 精简版回归测试脚本
# 只运行核心的回归测试（自动化视觉验证）

set -e
set -o pipefail  # 确保 pipeline 中任一端失败时整体失败（否则 tee 会掩盖测试脚本的 exit 1）

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "=========================================="
echo "  pmux 回归测试套件（精简版）"
echo "=========================================="
echo ""
echo "这是回归测试的核心套件，专注于："
echo "  - 自动化视觉验证"
echo "  - 关键路径覆盖"
echo ""
echo "完整功能测试请运行："
echo "  tests/functional/run_all.sh"
echo ""
echo "完整性能测试请运行："
echo "  tests/performance/run_all.sh"
echo ""
echo "完整 E2E 测试请运行："
echo "  tests/e2e/run_all.sh"
echo ""
echo "=========================================="
echo ""

# 解析参数
SKIP_BUILD=false
CI_MODE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --ci)
            CI_MODE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-build    跳过编译步骤"
            echo "  --ci            CI 模式（无交互）"
            echo "  -h, --help      显示帮助"
            echo ""
            echo "回归测试仅包含核心的自动化视觉验证："
            echo "  1. 窗口可见性检测（启动后截图验证）"
            echo "  2. 终端输入输出验证（echo hello world + OCR）"
            echo "  3. Sidebar 状态颜色检测"
            echo "  4. 终端光标位置检测"
            echo "  5. ANSI 颜色显示检测"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# 编译项目
if [ "$SKIP_BUILD" = false ]; then
    log_info "Building pmux..."
    cd "$PMUX_ROOT"
    if ! cargo build 2>&1 | tee /tmp/pmux_build.log; then
        log_error "Build failed! Check /tmp/pmux_build.log"
        exit 1
    fi
    log_info "Build successful"
else
    log_info "Skipping build (using existing binary)"
fi

# 初始化报告
init_report "regression" "pmux Regression Tests (Core)"

# 定义核心回归测试（窗口可见性放首位，其他测试依赖窗口正常显示）
CORE_TESTS=(
    "test_window_visibility_auto.sh:窗口可见性检测"
    "test_echo_output_auto.sh:终端输入输出验证"
    "test_sidebar_status_auto.sh:Sidebar 状态颜色检测"
    "test_cursor_position_auto.sh:终端光标位置检测"
    "test_colors_auto.sh:ANSI 颜色显示检测"
)

TOTAL_TESTS=${#CORE_TESTS[@]}
CURRENT=0
OVERALL_FAILED=0

# 运行测试
for test_info in "${CORE_TESTS[@]}"; do
    IFS=':' read -r test_file test_name <<< "$test_info"
    CURRENT=$((CURRENT + 1))
    
    echo ""
    echo "=========================================="
    echo "[$CURRENT/$TOTAL_TESTS] $test_name"
    echo "=========================================="
    
    test_path="$SCRIPT_DIR/$test_file"
    if [ -f "$test_path" ]; then
        # 清理残留 tmux session + stale socket，避免干扰下一个测试
        tmux kill-server 2>/dev/null || true
        rm -rf "/tmp/tmux-$(id -u)" 2>/dev/null || true
        sleep 1

        # 运行测试
        if bash "$test_path" 2>&1 | tee /tmp/last_test_output.log; then
            log_info "✓ $test_name: PASS"
        else
            log_error "✗ $test_name: FAIL"
            OVERALL_FAILED=$((OVERALL_FAILED + 1))
        fi
    else
        log_error "Test file not found: $test_path"
        OVERALL_FAILED=$((OVERALL_FAILED + 1))
    fi
    
    # 确保清理
    cleanup
    sleep 2
done

# 生成报告摘要
echo ""
echo "=========================================="
echo "  回归测试完成"
echo "=========================================="
echo "总计测试: $TOTAL_TESTS"
echo "通过: $((TOTAL_TESTS - OVERALL_FAILED))"
echo "失败: $OVERALL_FAILED"
echo ""

if [ $OVERALL_FAILED -eq 0 ]; then
    echo "✓ 所有回归测试通过"
    echo ""
    echo "建议下一步："
    echo "  - 运行功能测试: tests/functional/run_all.sh"
    echo "  - 运行性能测试: tests/performance/run_all.sh"
    exit 0
else
    echo "✗ 有 $OVERALL_FAILED 个测试失败"
    echo ""
    echo "查看详细报告: $REPORT_DIR/"
    exit 1
fi
