---
name: writing-plans
description: Create comprehensive implementation plans for multi-step development tasks. Use when specifications exist before coding, when the user asks for writing-plans, or when brainstorming produces an approved design to implement.
---

# Writing Plans

Create implementation plans assuming the engineer has minimal context for the codebase but is a skilled programmer. Document everything: which files to touch, code samples, test commands, expected output.

**Announce at start:** "I'm using the writing-plans skill to create the implementation plan."

**Context:** Run in a dedicated worktree when possible (see `using-git-worktrees`). Plans live in `docs/plans/`.

**Save to:** `docs/plans/YYYY-MM-DD-<feature-name>.md`

---

## Bite-Sized Task Granularity

Each step is one action (2–5 minutes):

- Write the failing test — step
- Run it to verify failure — step
- Implement minimal code to pass — step
- Run tests to verify pass — step
- Commit — step

---

## Plan Document Header

```markdown
# [Feature Name] Implementation Plan

> **For Claude:** Use TDD when implementing. Consider `subagent-driven-development` for parallel tasks.

**Goal:** [One sentence]
**Architecture:** [2–3 sentences]
**Tech Stack:** Rust, GPUI, [others]

---
```

---

## Task Structure

```
### Task N: [Component Name]

**Files:**
- Create: src/path/to/module.rs
- Modify: src/existing.rs:42–56
- Test: tests/integration_test.rs

**Step 1: Write the failing test**
[Complete test code]

**Step 2: Run test to verify failure**
Run: cargo test module_name::test_name
Expected: FAIL (e.g. "function not defined" or assertion)

**Step 3: Write minimal implementation**
[Complete implementation code]

**Step 4: Run test to verify pass**
Run: cargo test module_name::test_name
Expected: PASS

**Step 5: Commit**
git add src/... tests/...
git commit -m "feat: add X"
```

---

## Rules

- **Exact file paths** — never "add to config module"
- **Complete code** — not "add validation logic"
- **Exact commands** — with expected output
- **Reference skills** — e.g. `test-driven-development`, `subagent-driven-development`
- **DRY, YAGNI, TDD** — frequent commits

---

## pmux Conventions

- Plans in `docs/plans/` with date prefix
- Unit tests inline under `#[cfg(test)] mod tests`
- Integration tests in `tests/`
- Use `tempfile::TempDir` for filesystem isolation
- Add plan to `docs/plans/README.md` index when created

---

## Execution Handoff

After saving the plan, offer:

> Plan saved to `docs/plans/YYYY-MM-DD-feature.md`. Two options:
>
> 1. **Subagent-Driven** (this session) — `subagent-driven-development` skill, fresh subagent per task, review between tasks
> 2. **Parallel Session** — Open new session, batch execution with `executing-plans` skill
>
> Which approach?
