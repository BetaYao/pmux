---
name: requesting-code-review
description: Perform or request code reviews for pmux. Use when the user asks for a code review, review my code, or when reviewing changes before merge. Apply pmux design principles and success criteria.
---

# Requesting Code Review

When the user asks for a code review (or says "requesting-code-review"), perform a structured review using pmux-specific criteria.

## Request Context (User Provides)

- **Scope**: Which files or PR? (default: recent changes, staged files, or specified path)
- **Focus**: Architecture, correctness, tests, performance? (default: all)
- **Phase**: Is this Runtime refactoring Phase 1–4? (affects criteria)

## Review Checklist

### 1. Design Alignment

- [ ] Matches `design.md` (UI → Runtime API, no tmux in UI, streaming over polling)
- [ ] Success criteria: UI 不包含 tmux 调用, 无 polling loop, 新 backend 可接入
- [ ] UI 操作大方向 unchanged (7 items in §2)

### 2. Correctness & Safety

- [ ] Error handling with `thiserror`, no unwrap in hot paths
- [ ] No tmux/IO on main thread (use spawn, channel)
- [ ] Proper resource cleanup (drop, shutdown)

### 3. Tests

- [ ] New logic has tests (TDD: Arrange-Act-Assert)
- [ ] `cargo test` passes
- [ ] Use `tempfile::TempDir` for filesystem tests

### 4. Style & Conventions

- [ ] Follows existing patterns (GPUI Render, serde, etc.)
- [ ] No unnecessary allocations in hot paths
- [ ] Chinese error messages where user-facing

## Output Format

```markdown
## Code Review: [scope]

### Summary
[1–2 sentence verdict]

### Critical
- [ ] Issue 1
- [ ] Issue 2

### Suggestions
- [ ] Suggestion 1

### Positive
- [ ] What's done well
```

## Quick Review (User Says "quick review")

Focus on: design alignment, obvious bugs, missing tests. Skip style nitpicks.
