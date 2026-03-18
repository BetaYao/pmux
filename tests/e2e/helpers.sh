#!/bin/bash
# E2E test helpers for pmux
# Requires: macOS, Accessibility permissions for terminal app

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/results"
OCR_TOOL="$SCRIPT_DIR/ocr_tool"
OCR_SOURCE="$SCRIPT_DIR/ocr_tool.swift"
PMUX_PID=""
WINDOW_ID=""
PASS_COUNT=0
FAIL_COUNT=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

ensure_ocr_tool() {
    if [ ! -f "$OCR_TOOL" ] || [ "$OCR_SOURCE" -nt "$OCR_TOOL" ]; then
        echo "Compiling ocr_tool..."
        swiftc -o "$OCR_TOOL" "$OCR_SOURCE" -framework Vision -framework CoreGraphics -framework ImageIO
    fi
}

start_pmux() {
    echo "Building and launching pmux..."
    mkdir -p "$RESULTS_DIR"
    cd "$SCRIPT_DIR/../.."
    RUSTUP_TOOLCHAIN=stable cargo run &
    PMUX_PID=$!
    cd "$SCRIPT_DIR"
    echo "pmux PID: $PMUX_PID"
}

wait_for_window() {
    local timeout=${1:-30}
    local elapsed=0
    echo "Waiting for pmux window (timeout: ${timeout}s)..."
    while [ $elapsed -lt $timeout ]; do
        # GPUI windows are invisible to System Events, use CGWindowListCopyWindowInfo via Swift
        WINDOW_ID=$(swift -e '
            import CoreGraphics
            let windows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []
            for w in windows {
                let owner = w["kCGWindowOwnerName"] as? String ?? ""
                if owner == "pmux" {
                    if let wid = w["kCGWindowNumber"] as? Int {
                        print(wid)
                        break
                    }
                }
            }
        ' 2>/dev/null | head -1 || true)
        if [ -n "$WINDOW_ID" ] && [ "$WINDOW_ID" != "" ]; then
            echo "Found pmux window (CGWindowID): $WINDOW_ID"
            sleep 1  # extra settle time
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    echo "ERROR: pmux window not found after ${timeout}s"
    return 1
}

send_key() {
    local modifiers="$1"
    local key="$2"
    local using_clause=""

    for mod in $modifiers; do
        case "$mod" in
            cmd)   using_clause="${using_clause}command down, " ;;
            shift) using_clause="${using_clause}shift down, " ;;
            alt)   using_clause="${using_clause}option down, " ;;
            ctrl)  using_clause="${using_clause}control down, " ;;
        esac
    done

    # Remove trailing ", "
    using_clause="${using_clause%, }"

    osascript -e "
        tell application \"System Events\"
            tell (first process whose name is \"pmux\")
                set frontmost to true
            end tell
            delay 0.1
            keystroke \"$key\" using {$using_clause}
        end tell
    "
    sleep 0.5
}

send_special_key() {
    local key_code="$1"
    local modifiers="${2:-}"
    local using_clause=""

    for mod in $modifiers; do
        case "$mod" in
            cmd)   using_clause="${using_clause}command down, " ;;
            shift) using_clause="${using_clause}shift down, " ;;
            alt)   using_clause="${using_clause}option down, " ;;
            ctrl)  using_clause="${using_clause}control down, " ;;
        esac
    done
    using_clause="${using_clause%, }"

    if [ -n "$using_clause" ]; then
        osascript -e "
            tell application \"System Events\"
                key code $key_code using {$using_clause}
            end tell
        "
    else
        osascript -e "
            tell application \"System Events\"
                key code $key_code
            end tell
        "
    fi
    sleep 0.5
}

send_text() {
    local text="$1"
    osascript -e "
        tell application \"System Events\"
            tell (first process whose name is \"pmux\")
                set frontmost to true
            end tell
            delay 0.1
        end tell
    "
    for (( i=0; i<${#text}; i++ )); do
        local char="${text:$i:1}"
        osascript -e "
            tell application \"System Events\"
                keystroke \"$char\"
            end tell
        "
        sleep 0.05
    done
    sleep 0.3
}

take_screenshot() {
    local name="$1"
    local output="$RESULTS_DIR/${name}.png"
    # Use screencapture with window selection by process
    # -l requires CGWindowID; fall back to capturing the front window
    screencapture -o -x -l "$WINDOW_ID" "$output" 2>/dev/null || \
        screencapture -o -x "$output"
    echo "$output"
}

ocr() {
    local image="$1"
    "$OCR_TOOL" "$image" 2>/dev/null || true
}

assert_contains() {
    local step_name="$1"
    local expected="$2"
    local max_retries=3
    local retry=0

    while [ $retry -lt $max_retries ]; do
        local screenshot
        screenshot=$(take_screenshot "${step_name}_attempt${retry}")
        local text
        text=$(ocr "$screenshot")

        if echo "$text" | grep -qi "$expected"; then
            echo -e "${GREEN}[PASS]${NC} $step_name — found \"$expected\""
            PASS_COUNT=$((PASS_COUNT + 1))
            return 0
        fi

        retry=$((retry + 1))
        if [ $retry -lt $max_retries ]; then
            echo "  Retry $retry/$max_retries for \"$expected\"..."
            sleep 1
        fi
    done

    echo -e "${RED}[FAIL]${NC} $step_name — \"$expected\" not found"
    echo "  OCR output was:"
    echo "$text" | sed 's/^/    /'
    FAIL_COUNT=$((FAIL_COUNT + 1))
    return 1
}

assert_not_contains() {
    local step_name="$1"
    local unexpected="$2"
    local max_retries=3
    local retry=0

    while [ $retry -lt $max_retries ]; do
        local screenshot
        screenshot=$(take_screenshot "${step_name}_attempt${retry}")
        local text
        text=$(ocr "$screenshot")

        if ! echo "$text" | grep -qi "$unexpected"; then
            echo -e "${GREEN}[PASS]${NC} $step_name — \"$unexpected\" not found (as expected)"
            PASS_COUNT=$((PASS_COUNT + 1))
            return 0
        fi

        retry=$((retry + 1))
        if [ $retry -lt $max_retries ]; then
            echo "  Retry $retry/$max_retries waiting for \"$unexpected\" to disappear..."
            sleep 1
        fi
    done

    echo -e "${RED}[FAIL]${NC} $step_name — \"$unexpected\" still present"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    return 1
}

report() {
    local total=$((PASS_COUNT + FAIL_COUNT))
    echo ""
    echo "=== ${PASS_COUNT}/${total} tests passed ==="
    if [ $FAIL_COUNT -gt 0 ]; then
        echo "Screenshots saved to: $RESULTS_DIR"
        return 1
    fi
    return 0
}

detect_tab_count() {
    # Read tab count from pmux config file
    local config="$HOME/.config/pmux/config.json"
    if [ -f "$config" ]; then
        # Count "workspaces" array entries
        local count
        count=$(python3 -c "
import json, sys
try:
    c = json.load(open('$config'))
    print(len(c.get('workspaces', [])))
except:
    print(0)
" 2>/dev/null || echo 0)
        echo "$count"
    else
        echo "0"
    fi
}

cleanup() {
    if [ -n "$PMUX_PID" ] && kill -0 "$PMUX_PID" 2>/dev/null; then
        echo "Stopping pmux (PID: $PMUX_PID)..."
        kill "$PMUX_PID" 2>/dev/null || true
        wait "$PMUX_PID" 2>/dev/null || true
    fi
}
