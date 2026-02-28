#!/bin/bash
# 文字输入性能测试 - 测量打字延迟和吞吐量

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/../regression/lib/test_utils.sh"

echo "================================"
echo "Typing Performance Test"
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

# 测试 1: 单字符输入延迟
log_info "Test 1: Single character input latency"
SINGLE_CHAR_LATENCIES=()

for i in {1..50}; do
    START=$(date +%s.%N)
    send_keystroke "x"
    END=$(date +%s.%N)
    LATENCY=$(echo "$END - $START" | bc)
    SINGLE_CHAR_LATENCIES+=($LATENCY)
    
    if [ $((i % 10)) -eq 0 ]; then
        log_info "  Progress: $i/50"
    fi
    
    sleep 0.02
done

# 统计
MIN_LATENCY=${SINGLE_CHAR_LATENCIES[0]}
MAX_LATENCY=${SINGLE_CHAR_LATENCIES[0]}
SUM_LATENCY=0

for l in "${SINGLE_CHAR_LATENCIES[@]}"; do
    if (( $(echo "$l < $MIN_LATENCY" | bc -l) )); then
        MIN_LATENCY=$l
    fi
    if (( $(echo "$l > $MAX_LATENCY" | bc -l) )); then
        MAX_LATENCY=$l
    fi
    SUM_LATENCY=$(echo "$SUM_LATENCY + $l" | bc)
done

AVG_LATENCY=$(echo "scale=6; $SUM_LATENCY / ${#SINGLE_CHAR_LATENCIES[@]}" | bc)

echo ""
echo "Single Character Input:"
echo "  Min latency: ${MIN_LATENCY}s"
echo "  Max latency: ${MAX_LATENCY}s"
echo "  Avg latency: ${AVG_LATENCY}s"
echo ""

# 评估
LATENCY_MS=$(echo "scale=2; $AVG_LATENCY * 1000" | bc)
if (( $(echo "$AVG_LATENCY < 0.01" | bc -l) )); then
    log_info "✓ Single char latency: EXCELLENT (${LATENCY_MS}ms < 10ms)"
    add_report_result "Single Char Latency" "PASS" "${LATENCY_MS}ms avg"
elif (( $(echo "$AVG_LATENCY < 0.05" | bc -l) )); then
    log_warn "⚠ Single char latency: ACCEPTABLE (${LATENCY_MS}ms < 50ms)"
    add_report_result "Single Char Latency" "WARN" "${LATENCY_MS}ms avg"
else
    log_error "✗ Single char latency: POOR (${LATENCY_MS}ms > 50ms)"
    add_report_result "Single Char Latency" "FAIL" "${LATENCY_MS}ms avg"
fi

# 清理输入
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down control'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "c"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up control'
sleep 0.5

# 测试 2: 连续输入吞吐量
log_info "Test 2: Continuous typing throughput"

TEST_STRING="The quick brown fox jumps over the lazy dog. "
STRING_LENGTH=${#TEST_STRING}

START=$(date +%s.%N)
send_keystroke "$TEST_STRING"
END=$(date +%s.%N)
DURATION=$(echo "$END - $START" | bc)

# 计算吞吐量 (字符/秒)
THROUGHPUT=$(echo "scale=2; $STRING_LENGTH / $DURATION" | bc)

echo ""
echo "Continuous Typing:"
echo "  String length: $STRING_LENGTH chars"
echo "  Duration: ${DURATION}s"
echo "  Throughput: ${THROUGHPUT} chars/sec"
echo ""

if (( $(echo "$THROUGHPUT > 50" | bc -l) )); then
    log_info "✓ Typing throughput: EXCELLENT (${THROUGHPUT} chars/sec)"
    add_report_result "Typing Throughput" "PASS" "${THROUGHPUT} chars/sec"
elif (( $(echo "$THROUGHPUT > 20" | bc -l) )); then
    log_warn "⚠ Typing throughput: ACCEPTABLE (${THROUGHPUT} chars/sec)"
    add_report_result "Typing Throughput" "WARN" "${THROUGHPUT} chars/sec"
else
    log_error "✗ Typing throughput: POOR (${THROUGHPUT} chars/sec)"
    add_report_result "Typing Throughput" "FAIL" "${THROUGHPUT} chars/sec"
fi

# 清理
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down control'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "c"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up control'
sleep 0.5

# 测试 3: 粘贴大量文本
log_info "Test 3: Large text paste performance"

# 生成大文本（100行）
LARGE_TEXT=""
for i in {1..100}; do
    LARGE_TEXT="${LARGE_TEXT}Line $i: This is a test line for performance testing. "
done

LARGE_LENGTH=${#LARGE_TEXT}
echo "  Generated text: $LARGE_LENGTH characters"

START=$(date +%s.%N)
# 分段发送（避免 AppleScript 长度限制）
for i in {1..10}; do
    CHUNK=$(echo "$LARGE_TEXT" | cut -c$(( (i-1)*500 + 1 ))-$(( i*500 )))
    if [ -n "$CHUNK" ]; then
        send_keystroke "$CHUNK"
        sleep 0.1
    fi
done
END=$(date +%s.%N)
PASTE_DURATION=$(echo "$END - $START" | bc)

PASTE_THROUGHPUT=$(echo "scale=2; $LARGE_LENGTH / $PASTE_DURATION" | bc)

echo ""
echo "Large Text Paste:"
echo "  Duration: ${PASTE_DURATION}s"
echo "  Throughput: ${PASTE_THROUGHPUT} chars/sec"
echo ""

if (( $(echo "$PASTE_THROUGHPUT > 100" | bc -l) )); then
    log_info "✓ Large paste performance: GOOD"
    add_report_result "Large Text Paste" "PASS" "${PASTE_THROUGHPUT} chars/sec"
else
    log_warn "⚠ Large paste performance: SLOW"
    add_report_result "Large Text Paste" "WARN" "${PASTE_THROUGHPUT} chars/sec"
fi

# 清理
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down control'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "c"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up control'
sleep 0.5

# 测试 4: 快捷键响应
log_info "Test 4: Keyboard shortcut response"

SHORTCUT_LATENCIES=()

for i in {1..20}; do
    START=$(date +%s.%N)
    # Cmd+C (复制，即使没内容也测试响应)
    osascript_cmd 'tell application "System Events" to tell process "pmux" to key down command'
    osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "c"'
    osascript_cmd 'tell application "System Events" to tell process "pmux" to key up command'
    END=$(date +%s.%N)
    LATENCY=$(echo "$END - $START" | bc)
    SHORTCUT_LATENCIES+=($LATENCY)
    sleep 0.1
done

# 统计
SUM_SHORTCUT=0
for l in "${SHORTCUT_LATENCIES[@]}"; do
    SUM_SHORTCUT=$(echo "$SUM_SHORTCUT + $l" | bc)
done
AVG_SHORTCUT=$(echo "scale=6; $SUM_SHORTCUT / ${#SHORTCUT_LATENCIES[@]}" | bc)
SHORTCUT_MS=$(echo "scale=2; $AVG_SHORTCUT * 1000" | bc)

echo ""
echo "Shortcut Response:"
echo "  Avg latency: ${AVG_SHORTCUT}s (${SHORTCUT_MS}ms)"
echo ""

if (( $(echo "$AVG_SHORTCUT < 0.05" | bc -l) )); then
    log_info "✓ Shortcut latency: GOOD (${SHORTCUT_MS}ms)"
    add_report_result "Shortcut Latency" "PASS" "${SHORTCUT_MS}ms avg"
else
    log_warn "⚠ Shortcut latency: SLOW (${SHORTCUT_MS}ms)"
    add_report_result "Shortcut Latency" "WARN" "${SHORTCUT_MS}ms avg"
fi

# 截图
SCREENSHOT=$(take_screenshot "typing_performance")
log_info "Screenshot saved: $SCREENSHOT"

stop_pmux

# 生成详细报告
REPORT_FILE="tests/regression/results/typing_performance_$(date +%Y%m%d_%H%M%S).csv"
mkdir -p "tests/regression/results"
echo "Metric,Value,Unit,Status" > "$REPORT_FILE"
echo "Single Char Latency,${LATENCY_MS},ms,$([ $(echo "$AVG_LATENCY < 0.01" | bc) -eq 1 ] && echo PASS || echo WARN)" >> "$REPORT_FILE"
echo "Typing Throughput,${THROUGHPUT},chars/sec,$([ $(echo "$THROUGHPUT > 50" | bc) -eq 1 ] && echo PASS || echo WARN)" >> "$REPORT_FILE"
echo "Large Paste Throughput,${PASTE_THROUGHPUT},chars/sec,$([ $(echo "$PASTE_THROUGHPUT > 100" | bc) -eq 1 ] && echo PASS || echo WARN)" >> "$REPORT_FILE"
echo "Shortcut Latency,${SHORTCUT_MS},ms,$([ $(echo "$AVG_SHORTCUT < 0.05" | bc) -eq 1 ] && echo PASS || echo WARN)" >> "$REPORT_FILE"

echo ""
echo "Performance report saved to: $REPORT_FILE"

echo ""
echo "================================"
echo "Typing Performance Test Complete"
echo "================================"
echo ""

exit 0
