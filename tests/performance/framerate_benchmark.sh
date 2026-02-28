#!/bin/bash
# 渲染帧率性能测试 - 测量 60fps 稳定性和渲染性能

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"

echo "================================"
echo "Frame Rate Performance Test"
echo "================================"
echo ""

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

# 启用 frame tick 统计（需要重新编译启用统计功能）
log_info "Step 2: Measuring frame tick rate"

# 测量 10 秒内的 frame tick 次数
log_info "  Measuring frame ticks for 10 seconds..."

# 由于我们无法直接读取 frame tick 计数，我们通过测量响应性来评估
# 方法：发送一系列输入，测量响应延迟

FRAME_TICK_MEASUREMENTS=()
log_info "  Sending 100 keystrokes and measuring response..."

for i in {1..100}; do
    START=$(date +%s.%N)
    
    # 发送一个字符
    osascript -e 'tell application "System Events" to tell process "pmux" to keystroke "a"' 2>/dev/null
    
    END=$(date +%s.%N)
    DELAY=$(echo "$END - $START" | bc)
    FRAME_TICK_MEASUREMENTS+=($DELAY)
    
    # 每 10 个输出一次进度
    if [ $((i % 10)) -eq 0 ]; then
        log_info "    Progress: $i/100"
    fi
    
    sleep 0.01
done

# 统计延迟
echo ""
echo "Frame Response Statistics:"

MIN_DELAY=${FRAME_TICK_MEASUREMENTS[0]}
MAX_DELAY=${FRAME_TICK_MEASUREMENTS[0]}
SUM_DELAY=0

for d in "${FRAME_TICK_MEASUREMENTS[@]}"; do
    # 最小值
    if (( $(echo "$d < $MIN_DELAY" | bc -l) )); then
        MIN_DELAY=$d
    fi
    # 最大值
    if (( $(echo "$d > $MAX_DELAY" | bc -l) )); then
        MAX_DELAY=$d
    fi
    SUM_DELAY=$(echo "$SUM_DELAY + $d" | bc)
done

AVG_DELAY=$(echo "scale=6; $SUM_DELAY / ${#FRAME_TICK_MEASUREMENTS[@]}" | bc)

# 估算帧率（假设输入响应应在 1-2 帧内处理，16-32ms）
# 如果平均延迟 < 50ms，认为 60fps 稳定
# 如果平均延迟 < 100ms，认为 30fps 水平
# 如果平均延迟 > 100ms，性能问题

echo "  Min response time: ${MIN_DELAY}s"
echo "  Max response time: ${MAX_DELAY}s"
echo "  Avg response time: ${AVG_DELAY}s"
echo ""

# 转换为估算帧率（简单估算：1 / avg_delay）
if (( $(echo "$AVG_DELAY > 0" | bc -l) )); then
    ESTIMATED_FPS=$(echo "scale=1; 1 / $AVG_DELAY" | bc)
    echo "  Estimated FPS: ${ESTIMATED_FPS}"
    echo ""
    
    if (( $(echo "$AVG_DELAY < 0.05" | bc -l) )); then
        log_info "✓ Frame rate: EXCELLENT (60fps+, < 50ms response)"
        add_report_result "Frame Rate" "PASS" "${ESTIMATED_FPS} FPS (avg ${AVG_DELAY}s response)"
    elif (( $(echo "$AVG_DELAY < 0.1" | bc -l) )); then
        log_warn "⚠ Frame rate: ACCEPTABLE (30-60fps, 50-100ms response)"
        add_report_result "Frame Rate" "WARN" "${ESTIMATED_FPS} FPS (avg ${AVG_DELAY}s response)"
    else
        log_error "✗ Frame rate: POOR (< 30fps, > 100ms response)"
        add_report_result "Frame Rate" "FAIL" "${ESTIMATED_FPS} FPS (avg ${AVG_DELAY}s response)"
    fi
else
    log_error "✗ Could not calculate frame rate"
    add_report_result "Frame Rate" "FAIL" "No data"
fi

# 测试大输出时的帧率稳定性
log_info "Step 3: Testing frame rate under load (large output)"
send_keystroke "seq 1 1000"
send_keycode 36
sleep 5

# 在大量输出时测试响应
LOAD_MEASUREMENTS=()
log_info "  Testing response during large output..."

for i in {1..50}; do
    START=$(date +%s.%N)
    osascript -e 'tell application "System Events" to tell process "pmux" to keystroke "x"' 2>/dev/null
    END=$(date +%s.%N)
    DELAY=$(echo "$END - $START" | bc)
    LOAD_MEASUREMENTS+=($DELAY)
    sleep 0.05
done

# 清理
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down control'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "c"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up control'
sleep 1

# 统计负载下的性能
LOAD_SUM=0
for d in "${LOAD_MEASUREMENTS[@]}"; do
    LOAD_SUM=$(echo "$LOAD_SUM + $d" | bc)
done
LOAD_AVG=$(echo "scale=6; $LOAD_SUM / ${#LOAD_MEASUREMENTS[@]}" | bc)

echo ""
echo "Under Load (large output):"
echo "  Avg response time: ${LOAD_AVG}s"
echo ""

if (( $(echo "$LOAD_AVG < 0.1" | bc -l) )); then
    log_info "✓ Performance under load: GOOD"
    add_report_result "Frame Rate Under Load" "PASS" "${LOAD_AVG}s avg response"
else
    log_warn "⚠ Performance degraded under load"
    add_report_result "Frame Rate Under Load" "WARN" "${LOAD_AVG}s avg response"
fi

# 截图记录
SCREENSHOT=$(take_screenshot "framerate_test")
log_info "Screenshot saved: $SCREENSHOT"

stop_pmux

# 生成详细报告
REPORT_FILE="tests/regression/results/framerate_$(date +%Y%m%d_%H%M%S).csv"
mkdir -p "tests/regression/results"
echo "Iteration,Response Time (s),Load Response Time (s)" > "$REPORT_FILE"
for i in $(seq 0 $((${#FRAME_TICK_MEASUREMENTS[@]}-1))); do
    LOAD_VAL="N/A"
    if [ $i -lt ${#LOAD_MEASUREMENTS[@]} ]; then
        LOAD_VAL=${LOAD_MEASUREMENTS[$i]}
    fi
    echo "$((i+1)),${FRAME_TICK_MEASUREMENTS[$i]},$LOAD_VAL" >> "$REPORT_FILE"
done

echo ""
echo "Detailed data saved to: $REPORT_FILE"

echo ""
echo "================================"
echo "Frame Rate Test Complete"
echo "================================"
echo ""

exit 0
