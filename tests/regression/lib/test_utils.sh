#!/bin/bash
# 回归测试工具库

# 路径定义
export PMUX_ROOT="${PMUX_ROOT:-/Users/matt.chow/workspace/pmux}"
export PMUX_CONFIG_DIR="${PMUX_CONFIG_DIR:-$HOME/.config/pmux}"

# 确保配置目录存在
mkdir -p "$PMUX_CONFIG_DIR"

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# 测试计数
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_SKIPPED=0

# 日志函数
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 断言函数
assert_equals() {
    local expected="$1"
    local actual="$2"
    local message="${3:-Assertion failed}"
    
    if [ "$expected" = "$actual" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_info "✓ $message"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "✗ $message"
        log_error "  Expected: $expected"
        log_error "  Actual: $actual"
        return 1
    fi
}

assert_not_empty() {
    local value="$1"
    local message="${2:-Value should not be empty}"
    
    if [ -n "$value" ]; then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_info "✓ $message"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "✗ $message"
        return 1
    fi
}

assert_less_than() {
    local value="$1"
    local max="$2"
    local message="${3:-Value should be less than $max}"
    
    if (( $(echo "$value < $max" | bc -l) )); then
        TESTS_PASSED=$((TESTS_PASSED + 1))
        log_info "✓ $message"
        return 0
    else
        TESTS_FAILED=$((TESTS_FAILED + 1))
        log_error "✗ $message"
        log_error "  Value: $value, Max: $max"
        return 1
    fi
}

# 应用控制函数（PMUX_BIN 可选，未设置时用 target/debug/pmux）
start_pmux() {
    log_info "Starting pmux..."
    local bin="${PMUX_BIN:-$PMUX_ROOT/target/debug/pmux}"
    "$bin" &
    PMUX_PID=$!
    sleep 3
    
    if ! ps -p $PMUX_PID > /dev/null; then
        log_error "Failed to start pmux"
        return 1
    fi
    
    log_info "pmux started with PID $PMUX_PID"
    return 0
}

stop_pmux() {
    log_info "Stopping pmux..."
    if [ -n "$PMUX_PID" ]; then
        kill -9 $PMUX_PID 2>/dev/null || true
        wait $PMUX_PID 2>/dev/null || true
    fi
    killall -9 pmux 2>/dev/null || true
    sleep 1
}

# AppleScript 辅助函数
osascript_cmd() {
    local script="$1"
    # Use perl for timeout since macOS doesn't have timeout command
    perl -e 'alarm 5; exec "osascript", "-e", shift' "$script" 2>&1
}

get_window_count() {
    osascript_cmd 'tell application "System Events" to tell process "pmux" to count windows'
}

get_window_position() {
    osascript_cmd 'tell application "System Events" to tell process "pmux" to get position of window 1'
}

activate_window() {
    osascript_cmd 'tell application "System Events" to tell process "pmux" to set frontmost to true'
}

send_keystroke() {
    local keys="$1"
    # Escape backslashes and double quotes for AppleScript string
    local keys_escaped
    keys_escaped="${keys//\\/\\\\}"
    keys_escaped="${keys_escaped//\"/\\\"}"
    osascript_cmd "tell application \"System Events\" to tell process \"pmux\" to keystroke \"$keys_escaped\""
}

send_keycode() {
    local code="$1"
    osascript_cmd "tell application \"System Events\" to tell process \"pmux\" to key code $code"
}

# 点击 pmux 主内容区（terminal 区域），确保焦点在 terminal 上再发送按键
click_terminal_area() {
    osascript_cmd 'tell application "System Events"
        tell process "pmux"
            set pos to position of window 1
            set sz to size of window 1
            set clickX to round ((item 1 of pos) + (item 1 of sz) * 0.82)
            set clickY to round ((item 2 of pos) + (item 2 of sz) * 0.55)
        end tell
        click at {clickX, clickY}
    end tell' 2>/dev/null || true
}

# 性能测试函数
measure_startup_time() {
    local start_time=$(date +%s.%N)
    "$PMUX_ROOT/target/debug/pmux" &
    local pid=$!
    
    # 等待窗口出现
    for i in {1..30}; do
        if osascript -e 'tell application "System Events" to tell process "pmux" to count windows' 2>/dev/null | grep -q "1"; then
            break
        fi
        sleep 0.1
    done
    
    local end_time=$(date +%s.%N)
    kill -9 $pid 2>/dev/null
    
    # 计算时间差
    local duration=$(echo "$end_time - $start_time" | bc)
    echo "$duration"
}

get_memory_usage() {
    local pid=$(pgrep -f "target/debug/pmux" | head -1)
    if [ -n "$pid" ]; then
        ps -o rss= -p $pid 2>/dev/null | awk '{print $1 / 1024}'  # Convert to MB
    else
        echo "0"
    fi
}

# Get the CGWindowID for the pmux window (for screencapture -l)
get_pmux_window_id() {
    swift -e '
import CoreGraphics
let options = CGWindowListOption(arrayLiteral: .optionOnScreenOnly, .excludeDesktopElements)
if let list = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]] {
    for w in list {
        if let name = w["kCGWindowOwnerName"] as? String, name == "pmux",
           let layer = w["kCGWindowLayer"] as? Int, layer == 0,
           let num = w["kCGWindowNumber"] as? Int {
            print(num)
            break
        }
    }
}
' 2>/dev/null
}

# 截图函数 — captures only the pmux window (not the full desktop)
take_screenshot() {
    local name="$1"
    local output_dir="$SCRIPT_DIR/../results/screenshots"
    mkdir -p "$output_dir"
    
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local filename="${name}_${timestamp}.png"
    local filepath="$output_dir/$filename"

    # Get the CGWindowID and capture just the pmux window layer
    local win_id
    win_id=$(get_pmux_window_id)
    if [ -n "$win_id" ]; then
        screencapture -l"$win_id" "$filepath" 2>/dev/null || \
            screencapture "$filepath"
    else
        log_warn "Could not get pmux window ID, falling back to full screen" >&2
        screencapture "$filepath"
    fi
    
    log_info "Screenshot saved: $filepath" >&2
    echo "$filepath"
}

# 测试报告函数
init_report() {
    local report_dir="tests/regression/results"
    mkdir -p "$report_dir"
    export REPORT_DIR="${PMUX_ROOT:-.}/$report_dir"
    
    REPORT_FILE="$report_dir/report_$(date +%Y%m%d_%H%M%S).md"
    
    cat > "$REPORT_FILE" << EOF
# pmux 回归测试报告

**测试时间:** $(date)
**Git Commit:** $(git rev-parse --short HEAD 2>/dev/null || echo "N/A")
**分支:** $(git branch --show-current 2>/dev/null || echo "N/A")

## 测试结果摘要

EOF
}

add_report_section() {
    local section="$1"
    echo -e "\n## $section\n" >> "$REPORT_FILE"
}

add_report_result() {
    local test_name="$1"
    local result="$2"
    local details="${3:-}"
    
    local status_icon="❌"
    if [ "$result" = "PASS" ]; then
        status_icon="✅"
    elif [ "$result" = "SKIP" ]; then
        status_icon="⚠️"
    fi
    
    # 确保 REPORT_FILE 已定义
    if [ -n "$REPORT_FILE" ]; then
        echo "- $status_icon **$test_name**: $result" >> "$REPORT_FILE"
        if [ -n "$details" ]; then
            echo "  - $details" >> "$REPORT_FILE"
        fi
    fi
}

finalize_report() {
    cat >> "$REPORT_FILE" << EOF

## 测试统计

- ✅ 通过: $TESTS_PASSED
- ❌ 失败: $TESTS_FAILED
- ⚠️  跳过: $TESTS_SKIPPED
- **总计:** $((TESTS_PASSED + TESTS_FAILED + TESTS_SKIPPED))

EOF
    
    log_info "Report saved to: $REPORT_FILE"
}

# 清理函数
cleanup() {
    stop_pmux
}

trap cleanup EXIT
