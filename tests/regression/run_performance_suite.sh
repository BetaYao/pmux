#!/bin/bash
# 性能测试套件 - 运行所有性能测试并生成综合报告

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PMUX_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "================================"
echo "pmux Performance Test Suite"
echo "================================"
echo ""
echo "This will run comprehensive performance tests:"
echo "  1. Startup Performance (cold & hot)"
echo "  2. Frame Rate Performance"
echo "  3. Typing Performance"
echo "  4. Memory Usage"
echo "  5. TUI Performance"
echo ""
echo "Estimated time: 3-5 minutes"
echo ""

read -p "Press Enter to continue or Ctrl+C to cancel..."

# 创建结果目录
RESULTS_DIR="$PMUX_DIR/tests/regression/results/performance_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

echo ""
echo "Results will be saved to: $RESULTS_DIR"
echo ""

# 构建 release 版本
echo "Building release version for accurate measurements..."
cd "$PMUX_DIR"
cargo build --release 2>&1 | tail -3
echo ""

# 运行各个性能测试（1–3 使用 performance 目录下脚本，release 二进制）
export PMUX_ROOT="$PMUX_DIR"
export PMUX_BIN="$PMUX_DIR/target/release/pmux"
source "$SCRIPT_DIR/lib/test_utils.sh"

cd "$PMUX_DIR"

# 1. 启动性能
echo "================================"
echo "1/5: Startup Performance"
echo "================================"
bash "$PMUX_DIR/tests/performance/startup_benchmark.sh" 2>&1 | tee "$RESULTS_DIR/startup.log"
echo ""

# 2. 帧率性能
echo "================================"
echo "2/5: Frame Rate Performance"
echo "================================"
bash "$PMUX_DIR/tests/performance/framerate_benchmark.sh" 2>&1 | tee "$RESULTS_DIR/framerate.log"
echo ""

# 3. 输入性能
echo "================================"
echo "3/5: Typing Performance"
echo "================================"
bash "$PMUX_DIR/tests/performance/typing_benchmark.sh" 2>&1 | tee "$RESULTS_DIR/typing.log"
echo ""

# 4. 内存使用
echo "================================"
echo "4/5: Memory Usage"
echo "================================"

# 启动应用并测量内存
"$PMUX_DIR/target/release/pmux" &
PMUX_PID=$!
sleep 5

# 采样内存使用
MEMORY_SAMPLES=()
for i in {1..10}; do
    MEM=$(ps -o rss= -p $PMUX_PID 2>/dev/null | awk '{print $1 / 1024}')
    MEMORY_SAMPLES+=($MEM)
    echo "Sample $i: ${MEM}MB"
    sleep 1
done

# 计算平均和峰值
MEM_SUM=0
MEM_PEAK=0
for m in "${MEMORY_SAMPLES[@]}"; do
    MEM_SUM=$(echo "$MEM_SUM + $m" | bc)
    if (( $(echo "$m > $MEM_PEAK" | bc -l) )); then
        MEM_PEAK=$m
    fi
done
MEM_AVG=$(echo "scale=1; $MEM_SUM / ${#MEMORY_SAMPLES[@]}" | bc)

echo ""
echo "Memory Usage Summary:"
echo "  Average: ${MEM_AVG}MB"
echo "  Peak: ${MEM_PEAK}MB"
echo ""

kill -9 $PMUX_PID 2>/dev/null || true

echo "${MEMORY_SAMPLES[@]}" > "$RESULTS_DIR/memory_samples.txt"
echo "Average: ${MEM_AVG}MB" >> "$RESULTS_DIR/memory_summary.txt"
echo "Peak: ${MEM_PEAK}MB" >> "$RESULTS_DIR/memory_summary.txt"

echo ""

# 5. TUI 性能
echo "================================"
echo "5/5: TUI Performance"
echo "================================"
"$PMUX_DIR/target/release/pmux" &
PMUX_PID=$!
sleep 5

# 启动 vim
osascript -e 'tell application "System Events" to tell process "pmux" to set frontmost to true'
osascript -e 'tell application "System Events" to tell process "pmux" to keystroke "vim README.md"'
osascript -e 'tell application "System Events" to tell process "pmux" to keystroke return'
sleep 3

# 在 TUI 中测试响应
TUI_RESPONSES=()
for i in {1..20}; do
    START=$(date +%s.%N)
    osascript -e 'tell application "System Events" to tell process "pmux" to key code 126' 2>/dev/null  # up arrow
    END=$(date +%s.%N)
    DELAY=$(echo "$END - $START" | bc)
    TUI_RESPONSES+=($DELAY)
    sleep 0.1
done

# 计算平均响应
TUI_SUM=0
for d in "${TUI_RESPONSES[@]}"; do
    TUI_SUM=$(echo "$TUI_SUM + $d" | bc)
done
TUI_AVG=$(echo "scale=6; $TUI_SUM / ${#TUI_RESPONSES[@]}" | bc)
TUI_MS=$(echo "scale=2; $TUI_AVG * 1000" | bc)

echo ""
echo "TUI Response (vim arrow keys):"
echo "  Average: ${TUI_MS}ms"
echo ""

# 退出 vim
osascript -e 'tell application "System Events" to tell process "pmux" to keystroke ":q!"' 2>/dev/null
osascript -e 'tell application "System Events" to tell process "pmux" to keystroke return' 2>/dev/null
sleep 1

kill -9 $PMUX_PID 2>/dev/null || true

echo "${TUI_RESPONSES[@]}" > "$RESULTS_DIR/tui_response_samples.txt"
echo "Average: ${TUI_MS}ms" > "$RESULTS_DIR/tui_summary.txt"

# 生成综合报告
echo ""
echo "================================"
echo "Generating Performance Report"
echo "================================"
echo ""

cat > "$RESULTS_DIR/performance_report.md" << EOF
# pmux Performance Test Report

**Date:** $(date)  
**Commit:** $(git rev-parse --short HEAD 2>/dev/null || echo "N/A")  
**System:** $(sw_vers -productName) $(sw_vers -productVersion)

## Summary

| Metric | Value | Status |
|--------|-------|--------|
| Startup (Cold) | TBD | See startup.log |
| Startup (Hot) | TBD | See startup.log |
| Frame Rate | TBD | See framerate.log |
| Typing Latency | TBD | See typing.log |
| Memory (Avg) | ${MEM_AVG}MB | $( (( $(echo "$MEM_AVG < 200" | bc -l) )) && echo "✅ Good" || echo "⚠️ High" ) |
| Memory (Peak) | ${MEM_PEAK}MB | - |
| TUI Response | ${TUI_MS}ms | $( (( $(echo "$TUI_AVG < 0.05" | bc -l) )) && echo "✅ Good" || echo "⚠️ Slow" ) |

## Detailed Results

### 1. Startup Performance
See: \`startup.log\`

### 2. Frame Rate Performance
See: \`framerate.log\`

### 3. Typing Performance
See: \`typing.log\`

### 4. Memory Usage
- Samples: \`memory_samples.txt\`
- Summary: \`memory_summary.txt\`

### 5. TUI Performance
- Samples: \`tui_response_samples.txt\`
- Summary: \`tui_summary.txt\`

## Raw Data Files

$(ls -1 "$RESULTS_DIR"/*.csv 2>/dev/null || echo "No CSV files")

## Recommendations

Based on this test run:

$( (( $(echo "$MEM_AVG > 200" | bc -l) )) && echo "- ⚠️ Memory usage is high. Consider optimizing memory allocation." )
$( (( $(echo "$TUI_AVG > 0.05" | bc -l) )) && echo "- ⚠️ TUI response is slow. Check terminal rendering performance." )

EOF

echo ""
echo "================================"
echo "Performance Test Suite Complete"
echo "================================"
echo ""
echo "Report saved to: $RESULTS_DIR/performance_report.md"
echo ""
echo "Quick Summary:"
echo "  Memory: ${MEM_AVG}MB (avg), ${MEM_PEAK}MB (peak)"
echo "  TUI Response: ${TUI_MS}ms"
echo ""

# 打开报告（如果在 macOS 且不是 CI）
if [ -z "$CI" ] && command -v open >/dev/null 2>&1; then
    open "$RESULTS_DIR/performance_report.md"
fi

exit 0
