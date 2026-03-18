#!/bin/bash
# E2E test: Terminal output preserved after tab switch round-trip
#
# Prerequisites:
#   - macOS desktop with Accessibility permissions
#   - pmux config with 2+ workspace tabs
#   - swiftc available (Xcode CLI tools)
#
# Run: bash tests/e2e/test_terminal_tab_switch.sh

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"
trap cleanup EXIT

echo "=== Terminal Tab Switch State Preservation Test ==="
echo ""

# Build OCR tool if needed
ensure_ocr_tool

# Check tab count before starting
TAB_COUNT=$(detect_tab_count)
echo "Detected $TAB_COUNT workspace tab(s) in config"
if [ "$TAB_COUNT" -lt 2 ]; then
    echo "SKIP: Need 2+ workspace tabs in pmux config (~/.config/pmux/config.json)"
    echo "Add a second workspace via ⌘N or the sidebar '+' button, then re-run."
    exit 0
fi

# Start pmux
start_pmux
wait_for_window 30
sleep 3  # let workspace fully load

# Step 1: Type a unique marker command
echo ""
echo "--- Step 1: Type marker command ---"
send_text "echo PMUX_E2E_MARKER_1234"
send_special_key 36  # Return
sleep 1

# Step 2: Verify output appears
echo "--- Step 2: Verify marker visible ---"
assert_contains "marker_visible" "PMUX_E2E_MARKER_1234"

# Step 3: Switch to tab 2 (⌘2)
echo "--- Step 3: Switch to tab 2 ---"
send_key "cmd" "2"
sleep 2  # wait for tab 2 to load

# Step 4: Verify marker is NOT visible on tab 2
echo "--- Step 4: Verify marker not on tab 2 ---"
assert_not_contains "tab2_no_marker" "PMUX_E2E_MARKER_1234"

# Step 5: Switch back to tab 1 (⌘1)
echo "--- Step 5: Switch back to tab 1 ---"
send_key "cmd" "1"
sleep 3  # wait for tmux session recovery

# Step 6: Verify marker is still visible
echo "--- Step 6: Verify marker preserved ---"
assert_contains "marker_preserved" "PMUX_E2E_MARKER_1234"

echo ""
report
