#!/bin/bash
# 启动时间性能测试 - 详细测量冷启动和热启动

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"

echo "================================"
echo "Startup Performance Benchmark"
echo "================================"
echo ""

ITERATIONS=10
COLD_START_TIMES=()
HOT_START_TIMES=()

echo "Testing Cold Start (clearing caches)..."
echo ""

for i in $(seq 1 $ITERATIONS); do
    log_info "Cold start iteration $i/$ITERATIONS"
    
    # 清理缓存
    killall -9 pmux 2>/dev/null || true
    sync
    sleep 0.5
    
    # 测量启动时间
    START_TIME=$(date +%s.%N)
    "$PMUX_ROOT/target/debug/pmux" &
    PID=$!
    
    # 等待窗口出现
    WINDOW_APPEARED=0
    for j in {1..100}; do
        if osascript -e 'tell application "System Events" to tell process "pmux" to count windows' 2>/dev/null | grep -q "1"; then
            WINDOW_APPEARED=1
            break
        fi
        sleep 0.05
    done
    
    END_TIME=$(date +%s.%N)
    DURATION=$(echo "$END_TIME - $START_TIME" | bc)
    
    if [ "$WINDOW_APPEARED" = "1" ]; then
        COLD_START_TIMES+=($DURATION)
        log_info "  Cold start $i: ${DURATION}s"
    else
        log_error "  Cold start $i: Window did not appear"
    fi
    
    # 清理
    kill -9 $PID 2>/dev/null || true
    sleep 1
done

echo ""
echo "Testing Hot Start (no cache clearing)..."
echo ""

for i in $(seq 1 $ITERATIONS); do
    log_info "Hot start iteration $i/$ITERATIONS"
    
    # 热启动 - 不清理缓存
    START_TIME=$(date +%s.%N)
    ./target/debug/pmux &
    PID=$!
    
    # 等待窗口出现
    WINDOW_APPEARED=0
    for j in {1..100}; do
        if osascript -e 'tell application "System Events" to tell process "pmux" to count windows' 2>/dev/null | grep -q "1"; then
            WINDOW_APPEARED=1
            break
        fi
        sleep 0.05
    done
    
    END_TIME=$(date +%s.%N)
    DURATION=$(echo "$END_TIME - $START_TIME" | bc)
    
    if [ "$WINDOW_APPEARED" = "1" ]; then
        HOT_START_TIMES+=($DURATION)
        log_info "  Hot start $i: ${DURATION}s"
    else
        log_error "  Hot start $i: Window did not appear"
    fi
    
    # 清理
    kill -9 $PID 2>/dev/null || true
    sleep 0.5
done

# 计算统计数据
echo ""
echo "================================"
echo "Startup Performance Results"
echo "================================"
echo ""

# 冷启动统计
echo "Cold Start Statistics:"
if [ ${#COLD_START_TIMES[@]} -gt 0 ]; then
    COLD_MIN=${COLD_START_TIMES[0]}
    COLD_MAX=${COLD_START_TIMES[0]}
    COLD_SUM=0
    
    for t in "${COLD_START_TIMES[@]}"; do
        # 比较最小值
        if (( $(echo "$t < $COLD_MIN" | bc -l) )); then
            COLD_MIN=$t
        fi
        # 比较最大值
        if (( $(echo "$t > $COLD_MAX" | bc -l) )); then
            COLD_MAX=$t
        fi
        COLD_SUM=$(echo "$COLD_SUM + $t" | bc)
    done
    
    COLD_AVG=$(echo "scale=4; $COLD_SUM / ${#COLD_START_TIMES[@]}" | bc)
    
    echo "  Count: ${#COLD_START_TIMES[@]} runs"
    echo "  Min: ${COLD_MIN}s"
    echo "  Max: ${COLD_MAX}s"
    echo "  Avg: ${COLD_AVG}s"
    echo ""
    
    # 评估
    if (( $(echo "$COLD_AVG < 3.0" | bc -l) )); then
        log_info "✓ Cold start performance: EXCELLENT (< 3s)"
        add_report_result "Cold Start" "PASS" "${COLD_AVG}s (avg)"
    elif (( $(echo "$COLD_AVG < 5.0" | bc -l) )); then
        log_warn "⚠ Cold start performance: ACCEPTABLE (3-5s)"
        add_report_result "Cold Start" "WARN" "${COLD_AVG}s (avg, expected < 3s)"
    else
        log_error "✗ Cold start performance: POOR (> 5s)"
        add_report_result "Cold Start" "FAIL" "${COLD_AVG}s (avg, expected < 3s)"
    fi
else
    log_error "✗ No cold start data collected"
    add_report_result "Cold Start" "FAIL" "No data"
fi

# 热启动统计
echo ""
echo "Hot Start Statistics:"
if [ ${#HOT_START_TIMES[@]} -gt 0 ]; then
    HOT_MIN=${HOT_START_TIMES[0]}
    HOT_MAX=${HOT_START_TIMES[0]}
    HOT_SUM=0
    
    for t in "${HOT_START_TIMES[@]}"; do
        if (( $(echo "$t < $HOT_MIN" | bc -l) )); then
            HOT_MIN=$t
        fi
        if (( $(echo "$t > $HOT_MAX" | bc -l) )); then
            HOT_MAX=$t
        fi
        HOT_SUM=$(echo "$HOT_SUM + $t" | bc)
    done
    
    HOT_AVG=$(echo "scale=4; $HOT_SUM / ${#HOT_START_TIMES[@]}" | bc)
    
    echo "  Count: ${#HOT_START_TIMES[@]} runs"
    echo "  Min: ${HOT_MIN}s"
    echo "  Max: ${HOT_MAX}s"
    echo "  Avg: ${HOT_AVG}s"
    echo ""
    
    if (( $(echo "$HOT_AVG < 1.5" | bc -l) )); then
        log_info "✓ Hot start performance: EXCELLENT (< 1.5s)"
        add_report_result "Hot Start" "PASS" "${HOT_AVG}s (avg)"
    elif (( $(echo "$HOT_AVG < 3.0" | bc -l) )); then
        log_warn "⚠ Hot start performance: ACCEPTABLE (1.5-3s)"
        add_report_result "Hot Start" "WARN" "${HOT_AVG}s (avg, expected < 1.5s)"
    else
        log_error "✗ Hot start performance: POOR (> 3s)"
        add_report_result "Hot Start" "FAIL" "${HOT_AVG}s (avg, expected < 1.5s)"
    fi
else
    log_error "✗ No hot start data collected"
    add_report_result "Hot Start" "FAIL" "No data"
fi

# 生成详细报告
REPORT_FILE="tests/regression/results/startup_performance_$(date +%Y%m%d_%H%M%S).csv"
mkdir -p "tests/regression/results"
echo "Iteration,Cold Start (s),Hot Start (s)" > "$REPORT_FILE"
for i in $(seq 0 $((${#COLD_START_TIMES[@]}-1))); do
    echo "$((i+1)),${COLD_START_TIMES[$i]},${HOT_START_TIMES[$i]}" >> "$REPORT_FILE"
done

echo ""
echo "Detailed data saved to: $REPORT_FILE"

echo ""
echo "================================"
echo "Startup Performance Test Complete"
echo "================================"

exit 0
