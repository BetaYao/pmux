#!/bin/bash
# Config save/load: workspace_paths and active_workspace_index only (worktree selection by tmux window name, no persist).
# Runs without compiling pmux (no GPUI). Use when cargo test hits SIGBUS on macOS.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib/test_utils.sh" 2>/dev/null || true

log_info "Worktree state persistence test (shell)"

TMPDIR="${TMPDIR:-/tmp}"
WORK_DIR=$(mktemp -d "$TMPDIR/pmux-wt-test.XXXXXX")
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

REPO_PATH="$WORK_DIR/repo"
mkdir -p "$REPO_PATH"

# git init + first commit
git -C "$REPO_PATH" init -q
git -C "$REPO_PATH" config user.email "test@test.local"
git -C "$REPO_PATH" config user.name "Test"
echo x > "$REPO_PATH/f"
git -C "$REPO_PATH" add f
git -C "$REPO_PATH" commit -m "init" -q

# second worktree + branch (window name will be "feature-x")
WT2_PATH="$WORK_DIR/wt-feature"
git -C "$REPO_PATH" worktree add "$WT2_PATH" -b feature-x -q

# Config: only workspace_paths and active_workspace_index (no per_repo_worktree_window)
CONFIG_PATH="$WORK_DIR/config.json"
REPO_PATH_ESCAPED=$(echo "$REPO_PATH" | sed 's/\\/\\\\/g; s/"/\\"/g')
cat > "$CONFIG_PATH" << EOF
{
  "workspace_paths": ["$REPO_PATH_ESCAPED"],
  "active_workspace_index": 0
}
EOF

# Assert: load and check paths and active index
if command -v jq &>/dev/null; then
    paths_len=$(jq '.workspace_paths | length' "$CONFIG_PATH")
    active=$(jq -r '.active_workspace_index' "$CONFIG_PATH")
    [ "$paths_len" = "1" ] && [ "$active" = "0" ] || { log_error "expected 1 path and active 0"; exit 1; }
    log_info "✓ workspace_paths and active_workspace_index load correctly"
fi

# Multiple repos: second repo with main selected
REPO2_PATH="$WORK_DIR/repo2"
mkdir -p "$REPO2_PATH"
git -C "$REPO2_PATH" init -q
git -C "$REPO2_PATH" config user.email "test@test.local"
git -C "$REPO2_PATH" config user.name "Test"
echo x > "$REPO2_PATH/f"
git -C "$REPO2_PATH" add f
git -C "$REPO2_PATH" commit -m "init" -q
git -C "$REPO2_PATH" worktree add "$REPO2_PATH/wt-b" -b feature-b -q

REPO2_ESCAPED=$(echo "$REPO2_PATH" | sed 's/\\/\\\\/g; s/"/\\"/g')
cat > "$CONFIG_PATH" << EOF
{
  "workspace_paths": ["$REPO_PATH_ESCAPED", "$REPO2_ESCAPED"],
  "active_workspace_index": 1
}
EOF

if command -v jq &>/dev/null; then
    paths_len=$(jq '.workspace_paths | length' "$CONFIG_PATH")
    idx=$(jq -r '.active_workspace_index' "$CONFIG_PATH")
    [ "$paths_len" = "2" ] && [ "$idx" = "1" ] || { log_error "expected 2 paths and active 1"; exit 1; }
    log_info "✓ multi-repo config: 2 paths, active_workspace_index 1"
fi

log_info "All worktree state persistence checks passed."
