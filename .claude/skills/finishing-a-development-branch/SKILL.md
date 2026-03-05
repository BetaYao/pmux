---
name: finishing-a-development-branch
description: Workflow for finishing a development branch: commit, push, merge/PR, remove worktree. Use when the user is done with a feature branch, wants to clean up a worktree, or asks about finishing/closing a development branch.
---

# Finishing a Development Branch

Workflow for wrapping up a feature branch and cleaning up its worktree.

## When to Use

- User says work is done on a feature branch
- User wants to remove a worktree or clean up
- User asks how to finish, close, or merge a development branch

## Quick Workflow

### 1. Commit and push

```bash
# From the worktree directory
git add -A && git status
git commit -m "feat: descriptive message"
git push -u origin feature/your-branch
```

### 2. Merge or PR

- **Direct merge**: `git checkout main && git merge feature/your-branch`
- **PR**: Open PR on GitHub/GitLab, review, merge

### 3. Remove worktree (pmux or CLI)

**In pmux**: Sidebar → right-click worktree → Remove worktree (or equivalent). This runs `git worktree remove` and cleans up tmux.

**CLI**:
```bash
# From repo root (main)
cd /path/to/pmux
git worktree remove ../pmux-feature-xxx
```

### 4. Delete branch (after merged)

```bash
git branch -d feature/your-branch    # local
git push origin --delete feature/your-branch   # remote, if needed
```

## pmux-Specific

- **Diff view** (⌘⇧D): Review changes before committing
- **Delete worktree**: Sidebar context menu or delete flow; checks for uncommitted changes
- **Runtime state**: Tmux window is killed on worktree removal

## Checklist

- [ ] All changes committed
- [ ] Pushed to remote
- [ ] Merged via PR or direct merge
- [ ] Worktree removed
- [ ] Branch deleted (optional, after merge)
