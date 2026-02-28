#!/bin/bash
# 完整测试套件运行脚本
# 按顺序运行所有层级的测试

set -e
set -o pipefail

PMUX_ROOT="/Users/matt.chow/workspace/pmux"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_section() {
    echo ""
    echo "=========================================="
    echo "  $1"
    echo "=========================================="
    echo ""
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 解析参数
SKIP_BUILD=false
QUICK_MODE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        --quick)
            QUICK_MODE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo ""
            echo "Options:"
            echo "  --skip-build    跳过编译步骤"
            echo "  --quick         快速模式（仅回归+功能测试）"
            echo "  -h, --help      显示帮助"
            echo ""
            echo "测试层级（按顺序执行）："
            echo "  1. 回归测试 (regression/) - 核心验证"
            echo "  2. 功能测试 (functional/) - 模块验证"
            echo "  3. 性能测试 (performance/) - 基准测量"
            echo "  4. E2E 测试 (e2e/) - 场景验证"
            echo ""
            echo "快速模式 (--quick) 仅运行前两层，约 20 分钟"
            echo "完整模式 运行所有四层，约 40 分钟"
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
    log_section "编译项目"
    cd "$PMUX_ROOT"
    if ! cargo build 2>&1 | tee /tmp/pmux_build.log; then
        log_error "Build failed! Check /tmp/pmux_build.log"
        exit 1
    fi
    log_info "Build successful"
else
    log_info "Skipping build (using existing binary)"
fi

# 跟踪结果
OVERALL_STATUS=0
START_TIME=$(date +%s)

# 1. 回归测试
log_section "第 1 层：回归测试"
cd "$SCRIPT_DIR/regression"
if ./run_all.sh --skip-build; then
    log_info "✓ 回归测试通过"
else
    log_error "✗ 回归测试失败"
    OVERALL_STATUS=1
fi

# 2. 功能测试
log_section "第 2 层：功能测试"
cd "$SCRIPT_DIR/functional"
if ./run_all.sh --skip-build; then
    log_info "✓ 功能测试通过"
else
    log_error "✗ 功能测试失败"
    OVERALL_STATUS=1
fi

# 3. 性能测试（非快速模式）
if [ "$QUICK_MODE" = false ]; then
    log_section "第 3 层：性能测试"
    cd "$SCRIPT_DIR/performance"
    if ./run_all.sh --skip-build; then
        log_info "✓ 性能测试通过"
    else
        log_warn "⚠ 性能测试部分失败（可能指标不达标）"
    fi
    
    # 4. E2E 测试
    log_section "第 4 层：E2E 场景测试"
    cd "$SCRIPT_DIR/e2e"
    if ./run_all.sh --skip-build; then
        log_info "✓ E2E 测试通过"
    else
        log_error "✗ E2E 测试失败"
        OVERALL_STATUS=1
    fi
else
    log_info "跳过性能测试和 E2E 测试（快速模式）"
fi

# 总结
END_TIME=$(date +%s)
DURATION=$((END_TIME - START_TIME))
MINUTES=$((DURATION / 60))
SECONDS=$((DURATION % 60))

log_section "测试套件完成"
echo "总耗时: ${MINUTES}m ${SECONDS}s"
echo ""

if [ $OVERALL_STATUS -eq 0 ]; then
    echo -e "${GREEN}✓ 所有测试通过${NC}"
    echo ""
    echo "测试报告位置："
    echo "  - 回归测试: tests/regression/results/"
    echo "  - 功能测试: tests/functional/results/"
    if [ "$QUICK_MODE" = false ]; then
        echo "  - 性能测试: tests/performance/results/"
        echo "  - E2E 测试: tests/e2e/results/"
    fi
    exit 0
else
    echo -e "${RED}✗ 部分测试失败${NC}"
    echo ""
    echo "请查看各层级 results/ 目录中的详细报告"
    exit 1
fi
