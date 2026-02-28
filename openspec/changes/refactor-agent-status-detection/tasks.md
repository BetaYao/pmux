## 1. Add Exited Status

- [ ] 1.1 Add `Exited` variant to `AgentStatus` enum in `src/agent_status.rs`
- [ ] 1.2 Update `color()`, `icon()`, `display_text()` methods for Exited
- [ ] 1.3 Update `StatusCounts` to handle Exited

## 2. Refactor StatusDetector Priority

- [ ] 2.1 Add `ProcessStatus` enum (Running, Exited, Error, Unknown)
- [ ] 2.2 Refactor `detect()` to accept process_status + shell_phase + content
- [ ] 2.3 Implement priority: Process > OSC 133 > Text fallback
- [ ] 2.4 Update tests for new detection logic

## 3. Add Process Handle to Agent

- [ ] 3.1 Add process monitoring trait/object to Agent
- [ ] 3.2 Implement exit monitoring thread
- [ ] 3.3 Publish AgentStateChange on process exit

## 4. Integrate with StatusPublisher

- [ ] 4.1 Update `check_status()` signature to accept ProcessStatus
- [ ] 4.2 Wire process status from Agent to StatusPublisher
- [ ] 4.3 Update integration tests

## 5. Cleanup

- [ ] 5.1 Run tests to verify no regressions
- [ ] 5.2 Update any documentation
