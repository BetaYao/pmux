#!/bin/bash
# 功能测试主运行脚本
# 运行所有功能测试

set -e
set -o pipefail  # 确保 pipeline 中任一端失败时整体失败

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"

echo "=========================================="
echo "  pmux 功能测试套件"
echo "=========================================="
echo ""

# 解析参数
SKIP_BUILD=false
CI_MODE=false
SPECIFIC_TEST=""

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
        --test)
            SPECIFIC_TEST="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-build    跳过编译步骤"
            echo "  --ci            CI 模式（无交互，自动输出报告）"
            echo "  --test NAME     仅运行指定测试"
            echo "  -h, --help      显示帮助"
            echo ""
            echo "Available test modules:"
            echo "  window          Window 功能测试"
            echo "  workspace       Workspace 功能测试"
            echo "  terminal        Terminal 功能测试"
            echo "  pane            Pane 功能测试"
            echo "  input           输入功能测试"
            echo "  tui             TUI 兼容性测试"
            echo "  status          状态检测测试"
            echo "  render          渲染功能测试"
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
init_report "functional" "pmux Functional Tests"

# 定义测试模块
ALL_TESTS=(
    "window/window_creation_test.sh:Window 创建"
    "workspace/workspace_switching_test.sh:Workspace 切换"
    "terminal/basic_commands_test.sh:基本命令"
    "pane/split_operations_test.sh:Pane 操作"
    "input/keyboard_input_test.sh:键盘输入"
    "tui/vim_compatibility_test.sh:Vim 兼容"
    "status/agent_status_test.sh:状态检测"
    "render/ansi_colors_test.sh:ANSI 颜色"
)

# 运行特定测试或所有测试
if [ -n "$SPECIFIC_TEST" ]; then
    log_info "Running specific test: $SPECIFIC_TEST"
    TESTS_TO_RUN=()
    for test in "${ALL_TESTS[@]}"; do
        if [[ "$test" == *"/$SPECIFIC_TEST"* ]]; then
            TESTS_TO_RUN+=("$test")
        fi
    done
    if [ ${#TESTS_TO_RUN[@]} -eq 0 ]; then
        log_error "Test '$SPECIFIC_TEST' not found"
        exit 1
    fi
else
    TESTS_TO_RUN=("${ALL_TESTS[@]}")
fi

TOTAL_TESTS=${#TESTS_TO_RUN[@]}
CURRENT=0
OVERALL_FAILED=0

# 运行测试
for test_info in "${TESTS_TO_RUN[@]}"; do
    IFS=':' read -r test_file test_name <<< "$test_info"
    CURRENT=$((CURRENT + 1))
    
    echo ""
    echo "=========================================="
    echo "[$CURRENT/$TOTAL_TESTS] $test_name"
    echo "=========================================="
    
    test_path="$SCRIPT_DIR/$test_file"
    if [ -f "$test_path" ]; then
        # 运行测试并捕获输出
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
echo "  功能测试完成"
echo "=========================================="
echo "总计测试: $TOTAL_TESTS"
echo "通过: $((TOTAL_TESTS - OVERALL_FAILED))"
echo "失败: $OVERALL_FAILED"
echo ""

if [ $OVERALL_FAILED -eq 0 ]; then
    echo "✓ 所有功能测试通过"
    echo ""
    echo "报告位置: $REPORT_DIR/"
    exit 0
else
    echo "✗ 有 $OVERALL_FAILED 个测试失败"
    echo ""
    echo "查看详细报告: $REPORT_DIR/"
    exit 1
fi
