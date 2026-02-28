#!/bin/bash
# 性能测试主运行脚本

set -e
set -o pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"

echo "=========================================="
echo "  pmux 性能测试套件"
echo "=========================================="
echo ""
echo "性能测试测量以下指标："
echo "  - 启动时间（冷/热启动）"
echo "  - 渲染帧率"
echo "  - 打字延迟和吞吐量"
echo ""

# 解析参数
SKIP_BUILD=false
BASELINE_MODE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --baseline)
            BASELINE_MODE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-build    跳过编译步骤"
            echo "  --baseline      建立性能基线"
            echo "  -h, --help      显示帮助"
            echo ""
            echo "性能测试脚本："
            echo "  startup_benchmark.sh     - 启动时间基准测试"
            echo "  framerate_benchmark.sh - 渲染帧率测试"
            echo "  typing_benchmark.sh    - 输入性能测试"
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
if [ "$BASELINE_MODE" = true ]; then
    init_report "performance_baseline" "pmux Performance Baseline"
    log_info "建立性能基线模式"
else
    init_report "performance" "pmux Performance Tests"
fi

# 定义性能测试
PERF_TESTS=(
    "startup_benchmark.sh:启动时间"
    "framerate_benchmark.sh:渲染帧率"
    "typing_benchmark.sh:输入性能"
)

TOTAL_TESTS=${#PERF_TESTS[@]}
CURRENT=0
OVERALL_FAILED=0

# 运行测试
for test_info in "${PERF_TESTS[@]}"; do
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
    sleep 3
done

# 生成性能报告摘要
echo ""
echo "=========================================="
echo "  性能测试完成"
echo "=========================================="
echo "总计测试: $TOTAL_TESTS"
echo "通过: $((TOTAL_TESTS - OVERALL_FAILED))"
echo "失败: $OVERALL_FAILED"
echo ""

if [ "$BASELINE_MODE" = true ]; then
    echo "✓ 性能基线已建立"
    echo ""
    echo "基线数据保存位置："
    echo "  ${REPORT_DIR:-tests/regression/results}/baseline.json"
else
    echo "详细性能报告：${REPORT_FILE:-${REPORT_DIR:-tests/regression/results}/report_*.md}"
fi

exit $OVERALL_FAILED
