## ADDED Requirements

### Requirement: Process Lifecycle Status

Agent status SHALL be derived primarily from process lifecycle events (spawn, running, exit, crash).

#### Scenario: Agent running when process active
- **WHEN** the agent's process is running
- **AND** no exit has occurred
- **THEN** the agent status SHALL be Running

#### Scenario: Agent exited on process termination
- **WHEN** the agent's process exits with code 0
- **THEN** the agent status SHALL be Exited
- **AND** the terminal content SHALL remain visible

#### Scenario: Agent error on process crash
- **WHEN** the agent's process exits with non-zero code
- **THEN** the agent status SHALL be Error
- **AND** a notification SHALL be published

### Requirement: OSC 133 Status Priority

OSC 133 shell markers SHALL take priority over text pattern detection for status determination.

#### Scenario: Running from PreExec marker
- **WHEN** OSC 133 PreExec (C) marker is received
- **THEN** agent status SHALL be Running
- **AND** text patterns SHALL be ignored

#### Scenario: WaitingInput from Input phase
- **WHEN** OSC 133 PromptEnd (B) marker is received
- **AND** no PreExec marker follows
- **THEN** agent status SHALL be Waiting

#### Scenario: Error from PostExec with exit code
- **WHEN** OSC 133 PostExec (D) marker is received
- **AND** exit code is non-zero
- **THEN** agent status SHALL be Error

### Requirement: Text Pattern Fallback

Text pattern detection SHALL only be used as fallback when process status and OSC 133 are unavailable.

#### Scenario: Fallback when OSC 133 unavailable
- **WHEN** no OSC 133 markers have been received
- **AND** process is running
- **THEN** text patterns SHALL be used to detect Running/Waiting/Error

#### Scenario: No fallback when process exited
- **WHEN** process has exited
- **THEN** text patterns SHALL NOT override Exited or Error status
