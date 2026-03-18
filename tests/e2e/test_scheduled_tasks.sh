#!/bin/bash
# E2E test: Create and delete a scheduled task via keyboard shortcuts
# Requires: macOS, Accessibility permissions, pmux buildable

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

# Cleanup on exit
trap cleanup EXIT

echo "=== E2E Test: Scheduled Tasks Create & Delete ==="
echo ""

# Setup
ensure_ocr_tool
start_pmux
wait_for_window 30

sleep 2  # let UI fully render

# Test 1: Toggle task list with Cmd+Shift+L
echo "--- Test 1: Toggle task list ---"
send_key "cmd shift" "l"
sleep 1
assert_contains "task_list_visible" "Scheduled Tasks" || true

# Test 2: Open TaskDialog with Cmd+Shift+T
echo "--- Test 2: Open TaskDialog ---"
send_key "cmd shift" "t"
sleep 1
assert_contains "task_dialog_open" "New Scheduled Task" || true

# Test 3: Create a task
echo "--- Test 3: Create task ---"
send_text "E2E_Test_Task"
# Tab to skip Cron field (keeps default "0 2 * * *")
send_special_key 48  # Tab key code
sleep 0.3
send_special_key 48  # Tab again to Command field
sleep 0.3
send_text "echo hello"
# Enter to save
send_special_key 36  # Return key code
sleep 1
assert_contains "task_created" "E2E_Test_Task" || true

# Test 4: Delete the task
echo "--- Test 4: Delete task ---"
# Focus task list
send_key "cmd shift" "l"
sleep 0.5
# Arrow down to select the task (it may be the first/only one)
send_special_key 125  # Down arrow key code
sleep 0.3
# Cmd+Shift+Backspace to initiate delete
send_special_key 51 "cmd shift"  # Backspace key code = 51
sleep 0.5
# Enter to confirm
send_special_key 36  # Return key code
sleep 1
assert_not_contains "task_deleted" "E2E_Test_Task" || true

# Report
echo ""
report
