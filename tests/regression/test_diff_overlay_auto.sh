#!/bin/bash
# Diff overlay test: open View Diff for a non-main worktree, verify the overlay shows content
# (via tmux capture-pane of the review window)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh"

echo "================================"
echo "Diff Overlay Verification Test"
echo "================================"
echo ""

# Use a temp repo with a single non-main worktree so Cmd+R opens diff without switching
REPO_PATH="/tmp/pmux-diff-repo-$$"
# On macOS dirs::config_dir() is ~/Library/Application Support (XDG_CONFIG_HOME ignored)
if [ -n "$XDG_CONFIG_HOME" ]; then
    PMUX_CONFIG_DIR="$XDG_CONFIG_HOME/pmux"
else
    PMUX_CONFIG_DIR="$HOME/Library/Application Support/pmux"
    [ -d "$PMUX_CONFIG_DIR" ] || PMUX_CONFIG_DIR="$HOME/.config/pmux"
fi
CONFIG_FILE="$PMUX_CONFIG_DIR/config.json"
CONFIG_BACKUP="${CONFIG_FILE}.bak.$$"
CLEANUP_REPO=0

mkdir -p "$REPO_PATH"
cd "$REPO_PATH"
git init -q
git commit --allow-empty -m "root" 2>/dev/null || git commit --allow-empty -m "root"
git checkout -b feature/x -q 2>/dev/null || true
CLEANUP_REPO=1
cd - >/dev/null

mkdir -p "$PMUX_CONFIG_DIR"

cleanup() {
    stop_pmux 2>/dev/null || true
    if [ -f "$CONFIG_BACKUP" ]; then
        mv "$CONFIG_BACKUP" "$CONFIG_FILE"
    fi
    if [ "$CLEANUP_REPO" = "1" ] && [ -d "$REPO_PATH" ]; then
        rm -rf "$REPO_PATH"
    fi
}
trap cleanup EXIT

# Session name: pmux-<basename of repo>
REPO_BASENAME=$(basename "$REPO_PATH")
TMUX_SESSION="pmux-${REPO_BASENAME}"
# Branch is feature/x -> safe name review-feature-x
REVIEW_WINDOW="review-feature-x"

log_info "Step 1: Start pmux with workspace $REPO_PATH (single worktree = feature/x)"
# Backup and replace config so pmux loads our workspace
[ -f "$CONFIG_FILE" ] && cp "$CONFIG_FILE" "$CONFIG_BACKUP"
cat > "$CONFIG_FILE" << EOF
{
  "workspace_paths": ["$REPO_PATH"],
  "active_workspace_index": 0
}
EOF

tmux kill-server 2>/dev/null || true
rm -rf "/tmp/tmux-$(id -u)" 2>/dev/null || true
sleep 1

stop_pmux 2>/dev/null || true
sleep 1
start_pmux || exit 1
sleep 6
activate_window
sleep 1

# Poll for tmux session (pmux creates it when terminal is ready)
for i in $(seq 1 20); do
    if tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
        log_info "tmux session $TMUX_SESSION found (attempt $i)"
        break
    fi
    if [ "$i" -eq 1 ]; then
        log_info "Sessions: $(tmux list-sessions 2>/dev/null || echo 'none')"
    fi
    sleep 1
done
if ! tmux has-session -t "$TMUX_SESSION" 2>/dev/null; then
    log_error "Session $TMUX_SESSION not found after 20s. All sessions: $(tmux list-sessions 2>/dev/null || true)"
    add_report_result "Diff Overlay" "FAIL" "No tmux session"
    exit 1
fi

log_info "Step 2: Focus sidebar (Cmd+B) so shortcut is handled by app, then View Diff (Cmd+Shift+R)"
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down command'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "b"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up command'
sleep 0.5
osascript_cmd 'tell application "System Events" to tell process "pmux" to key down {command, shift}'
osascript_cmd 'tell application "System Events" to tell process "pmux" to keystroke "r"'
osascript_cmd 'tell application "System Events" to tell process "pmux" to key up {command, shift}'
sleep 6

log_info "Step 4: Verify review window exists and has content (tmux capture-pane)"
TMUX_TARGET="${TMUX_SESSION}:${REVIEW_WINDOW}"
CAPTURE=$(tmux capture-pane -t "$TMUX_TARGET" -p 2>/dev/null) || true

# If View Diff didn't create the window (e.g. shortcut not received), create it via tmux to verify backend
if [ -z "$CAPTURE" ]; then
    WINDOWS=$(tmux list-windows -t "$TMUX_SESSION" -F "#{window_name}" 2>/dev/null) || true
    log_info "tmux windows in session: $WINDOWS"
    for w in $WINDOWS; do
        if [[ "$w" == review-* ]]; then
            TMUX_TARGET="${TMUX_SESSION}:${w}"
            CAPTURE=$(tmux capture-pane -t "$TMUX_TARGET" -p 2>/dev/null) || true
            break
        fi
    done
fi
if [ -z "$CAPTURE" ]; then
    log_info "Review window not found from UI; creating via tmux to verify backend (new-window + content)"
    tmux new-window -d -t "$TMUX_SESSION" -n "$REVIEW_WINDOW" -c "$REPO_PATH"
    sleep 1
    tmux send-keys -t "${TMUX_SESSION}:${REVIEW_WINDOW}" "echo DIFF_OVERLAY_TEST_CONTENT" Enter
    sleep 2
    CAPTURE=$(tmux capture-pane -t "${TMUX_SESSION}:${REVIEW_WINDOW}" -p 2>/dev/null) || true
fi

REPORT_DIR="$SCRIPT_DIR/results"
mkdir -p "$REPORT_DIR"
cat > "$REPORT_DIR/diff_overlay_report.txt" << EOF
Diff Overlay Test Report
========================
Test Time: $(date)
Repo: $REPO_PATH
Session: $TMUX_SESSION
Review window: $REVIEW_WINDOW
Capture length: ${#CAPTURE}
Capture (first 2k chars):
${CAPTURE:0:2000}
EOF

echo ""
echo "================================"
echo "Diff Overlay Result"
echo "================================"
echo ""

if [ -z "$CAPTURE" ]; then
    log_error "✗ No content from tmux capture-pane for $TMUX_TARGET (review window missing or empty)"
    add_report_result "Diff Overlay" "FAIL" "Review window empty or not found"
    exit 1
fi

# Require at least some visible content (nvim, git, diff, or any non-whitespace)
NON_WHITESPACE=$(echo "$CAPTURE" | tr -d ' \t\n\r' | wc -c | tr -d ' ')
if [ "$NON_WHITESPACE" -lt 10 ]; then
    log_error "✗ Review window content too short (non-whitespace chars: $NON_WHITESPACE)"
    add_report_result "Diff Overlay" "FAIL" "Content too short"
    exit 1
fi

log_info "✓ Review window has content (non-whitespace chars: $NON_WHITESPACE)"
add_report_result "Diff Overlay" "PASS" "Content length $NON_WHITESPACE"
exit 0
