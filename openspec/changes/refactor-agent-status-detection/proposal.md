# Change: Refactor Agent Status Detection

## Why

Current status detection relies primarily on text pattern matching, which is unreliable. Per design.md, Agent status should come primarily from process lifecycle and OSC 133 markers, with text patterns as fallback only.

## What Changes

- Add `Exited` status to `AgentStatus` enum
- Refactor `StatusDetector` to prioritize: process lifecycle > OSC 133 > text fallback
- Add process handle and exit monitoring to `Agent` model
- Update `StatusPublisher` to integrate process status

## Impact

- Affected specs: agent-status-realtime-updates, shell-integration
- Affected code:
  - `src/agent_status.rs` - Add Exited variant
  - `src/status_detector.rs` - Refactor priority logic
  - `src/runtime/agent.rs` - Add process handle
  - `src/runtime/status_publisher.rs` - Integrate process status
