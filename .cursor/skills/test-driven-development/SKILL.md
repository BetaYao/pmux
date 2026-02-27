---
name: test-driven-development
description: Follow TDD when implementing new features or fixing bugs in pmux. Use when implementing plans from docs/plans/, adding new modules, or when the user asks for test-driven development.
---

# Test-Driven Development

When implementing features in pmux, follow Red-Green-Refactor cycle. Write failing tests first, then implement until green, then refactor.

## Cycle

1. **Red** — Write a failing test that defines the desired behavior
2. **Green** — Implement the minimum code to satisfy the test
3. **Refactor** — Improve the code while keeping tests green

## Structure: Arrange-Act-Assert

```rust
#[test]
fn test_example() {
    // Arrange: set up inputs and state
    let mut manager = WorkspaceManager::new();
    manager.add_workspace(PathBuf::from("/tmp/repo"));

    // Act: perform the operation
    let result = manager.close_tab(0);

    // Assert: verify the outcome
    assert!(result.is_ok());
    assert_eq!(manager.tab_count(), 0);
}
```

## Test Location

- **Unit tests**: Inline in source file under `#[cfg(test)] mod tests`
- **Integration tests**: `tests/` directory
- **Test modules**: `*_test.rs` files (e.g. `workspace_manager_test.rs`) that `#[cfg(test)]` include in the parent module

## Plan Integration

When implementing tasks from `docs/plans/*.md`:

- **Step 1** is often "Write failing tests" — define the API and behavior before implementation
- Run `cargo test` after each step to verify
- Use `cargo test module_name::` to run tests for a specific module

## Filesystem Tests

Use `tempfile::TempDir` for isolation:

```rust
let temp = tempfile::tempdir().unwrap();
let path = temp.path().join("repo");
```

## Commands

```bash
# Run all tests
RUSTUP_TOOLCHAIN=stable cargo test

# Run a specific test
cargo test test_workspace_tab_creation

# Run tests in a module
cargo test workspace_manager::
```

## When to Skip TDD

- Trivial one-liners (e.g. getters)
- UI-only changes with no logic
- Prototyping or spike work (add tests when stabilizing)
