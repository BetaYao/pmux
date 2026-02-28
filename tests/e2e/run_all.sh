#!/bin/bash
# E2E 场景测试主运行脚本

set -e
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"

echo "=========================================="
echo "  pmux E2E 场景测试"
echo "=========================================="
echo ""
echo "E2E 测试覆盖完整的用户工作流："
echo "  - 新分支创建工作流"
echo "  - Workspace 恢复工作流"
echo "  - Claude Code TUI 场景"
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
init_report "e2e" "pmux E2E Scenario Tests"

# 定义 E2E 测试
E2E_TESTS=(
    "workflow_new_branch.sh:新分支创建工作流"
    "workflow_restore.sh:Workspace 恢复工作流"
    "scenario_claude_code.sh:Claude Code TUI 场景"
)

TOTAL_TESTS=${#E2E_TESTS[@]}
CURRENT=0
OVERALL_FAILED=0

# 运行测试
for test_info in "${E2E_TESTS[@]}"; do
    IFS=':' read -r test_file test_name <<< "$test_info"
    CURRENT=$((CURRENT + 1))
    
    echo ""
    echo "=========================================="
    echo "[$CURRENT/$TOTAL_TESTS] $test_name"
    echo "=========================================="
    
    test_path="$SCRIPT_DIR/$test_file"
    if [ -f "$test_path" ]; then
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
    
    cleanup
    sleep 2
done

# 生成报告摘要
echo ""
echo "=========================================="
echo "  E2E 测试完成"
echo "=========================================="
echo "总计测试: $TOTAL_TESTS"
echo "通过: $((TOTAL_TESTS - OVERALL_FAILED))"
echo "失败: $OVERALL_FAILED"
echo ""

if [ $OVERALL_FAILED -eq 0 ]; then
    echo "✓ 所有 E2E 测试通过"
    echo ""
    echo "报告位置: $REPORT_DIR/"
    exit 0
else
    echo "✗ 有 $OVERALL_FAILED 个测试失败"
    exit 1
fi
