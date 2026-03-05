---
name: using-git-worktrees
description: Use git worktrees for isolated feature development. Use when implementing plans from docs/plans/, starting a new feature branch, or when the user asks for worktree setup.
---

# Using Git Worktrees

Use git worktrees to keep feature work isolated. Plans in `docs/plans/` should ideally run in a dedicated worktree.

## When to Use

- Implementing a plan from `docs/plans/`
- Starting a new feature that shouldn't pollute main
- User asks for worktree setup or mentions "using-git-worktrees"

## Quick Workflow

### Create a worktree for a plan

```bash
# From repo root (main branch)
git worktree add .worktrees/<branch-slug> -b feature/<branch-slug>
cd .worktrees/<branch-slug>
```

Convention: `.worktrees/<branch-slug>` (e.g. `.worktrees/route-b-entity-split` for `feature/route-b-entity-split`).

### After work is done

```bash
# From worktree dir: commit, push, then remove
git add -A && git commit -m "feat: ..."
git push -u origin feature/<branch-slug>

# From repo root (main)
cd /path/to/pmux
git worktree remove .worktrees/<branch-slug>
# Optional: git branch -d feature/<branch-slug>  (if merged)
```

## Cursor Integration

- **New Cursor window**: `cursor .worktrees/<branch-slug>` to open the worktree in a separate window
- **Context**: Main window = docs/review; worktree window = implementation
- **.worktrees/**: Gitignored; worktrees live here to keep the repo tidy.

## Rules

- Worktree path: `.worktrees/<branch-slug>` inside the repo (gitignored)
- When handing off: mention the worktree path so the next session can `cd` there
