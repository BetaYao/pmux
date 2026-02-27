---
name: subagent-driven-development
description: Use mcp_task subagents to implement plans from docs/plans/ in parallel. Use when implementing Runtime refactoring phases, executing multi-step plans, or when the user asks for subagent-driven development.
---

# Subagent-Driven Development

When implementing plans from `docs/plans/` (especially Runtime Phase 1–4), delegate tasks to subagents for parallel execution and faster progress.

## When to Use

- Implementing a plan from `docs/plans/*.md`
- Multi-step refactoring with independent tasks
- User explicitly requests subagent-driven development

## Subagent Types

| Type | Use For | Example |
|------|---------|---------|
| `explore` | Codebase search, finding files/patterns | "Find all tmux:: usages in src/ui/" |
| `shell` | Git, cargo, file ops | "Run cargo test", "Create src/runtime/mod.rs" |
| `generalPurpose` | Complex multi-step tasks | "Implement Task 1 of Phase 1 plan" |

## Workflow

1. **Read the plan** (e.g. `docs/plans/2026-02-runtime-phase1-streaming-terminal.md`)
2. **Identify parallelizable tasks** — Tasks with no dependencies can run in parallel
3. **Launch subagents** with `mcp_task`:
   - Provide full context: plan path, task description, relevant code snippets
   - Specify what to return (e.g. "List files created/modified", "Report test results")
4. **Synthesize results** — Merge subagent outputs, fix conflicts, run final verification

## Example: Phase 1 Implementation

```text
Launch 2 subagents in parallel:

1. explore: "In pmux repo, find where capture_pane and terminal polling are used. Return file paths and line numbers."
2. generalPurpose: "Create src/runtime/mod.rs and src/runtime/pty_bridge.rs skeleton per docs/plans/2026-02-runtime-phase1-streaming-terminal.md Task 1. Add pub mod runtime to lib.rs."
```

## Prompt Guidelines

- **Be specific**: Include plan file path, task number, and expected output format
- **Provide context**: Attach or cite design.md, plan sections
- **Request concrete output**: "Return: list of modified files" or "Return: any compile errors"
- **Avoid vague prompts**: "Implement the plan" is too broad; "Implement Task 2 Steps 1–2" is better

## Limits

- Max 4 concurrent subagents
- Subagents have no access to parent conversation; include all needed context in the prompt
- Prefer `fast` model for straightforward tasks; use default for deep reasoning

## Plan Index

| Phase | Plan File |
|-------|-----------|
| 1 | `docs/plans/2026-02-runtime-phase1-streaming-terminal.md` |
| 2 | `docs/plans/2026-02-runtime-phase2-runtime-abstraction.md` |
| 3 | `docs/plans/2026-02-runtime-phase3-agent-runtime.md` |
| 4 | `docs/plans/2026-02-runtime-phase4-input-rewrite.md` |
