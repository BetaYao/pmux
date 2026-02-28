//! Phase 4: Status detector OSC 133 shell phase integration tests.
//! Run with: cargo test --test status_detector_osc133

use pmux::agent_status::AgentStatus;
use pmux::shell_integration::{MarkerKind, ParsedMarker, ShellMarker, ShellPhase, ShellPhaseInfo};
use pmux::status_detector::{ProcessStatus, StatusDetector};
use pmux::terminal::TerminalEngine;

#[test]
fn test_detect_with_process_exited() {
    let detector = StatusDetector::new();
    let status = detector.detect(ProcessStatus::Exited, None, "any content");
    assert_eq!(status, AgentStatus::Exited);
}

#[test]
fn test_detect_with_process_error() {
    let detector = StatusDetector::new();
    let status = detector.detect(ProcessStatus::Error, None, "any content");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_detect_with_osc133_running() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Running,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "$ ls -la\nsome output");
    assert_eq!(status, AgentStatus::Running);
}

#[test]
fn test_detect_with_osc133_output_error() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(1),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "command output");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_detect_with_osc133_output_success() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(0),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "$ echo done\ndone");
    assert_eq!(status, AgentStatus::Idle);
}

#[test]
fn test_detect_with_osc133_input_waiting() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Input,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "$ ");
    assert_eq!(status, AgentStatus::Waiting);
}

#[test]
fn test_detect_text_fallback() {
    let detector = StatusDetector::new();
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "AI is thinking"),
        AgentStatus::Running
    );
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "? What next?"),
        AgentStatus::Waiting
    );
}

#[test]
fn test_process_overrides_osc133() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Running,
        last_post_exec_exit_code: None,
    };
    // Process Exited overrides OSC 133 Running
    let status = detector.detect(ProcessStatus::Exited, Some(info), "any content");
    assert_eq!(status, AgentStatus::Exited);
    
    // Process Error overrides OSC 133 Running
    let status = detector.detect(ProcessStatus::Error, Some(info), "any content");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_osc133_overrides_text() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Running,
        last_post_exec_exit_code: None,
    };
    // OSC 133 Running should override text "error" pattern
    let status = detector.detect(ProcessStatus::Running, Some(info), "error in log");
    assert_eq!(status, AgentStatus::Running);
}

#[test]
fn test_integration_with_terminal_engine() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);
    drop(tx);

    let detector = StatusDetector::new();

    let info = ShellPhaseInfo {
        phase: engine.shell_phase(),
        last_post_exec_exit_code: engine.last_post_exec_exit_code(),
    };
    assert_eq!(info.phase, ShellPhase::Unknown);
    let status = detector.detect(ProcessStatus::Running, Some(info), "hello");
    assert_eq!(status, AgentStatus::Idle);

    {
        let mut state = engine.shell_state();
        let marker = ShellMarker::from_parsed(
            ParsedMarker {
                kind: MarkerKind::PreExec,
                exit_code: None,
            },
            0,
            0,
        );
        state.add_marker(marker);
    }
    let info = ShellPhaseInfo {
        phase: engine.shell_phase(),
        last_post_exec_exit_code: engine.last_post_exec_exit_code(),
    };
    assert_eq!(info.phase, ShellPhase::Running);
    let status = detector.detect(ProcessStatus::Running, Some(info), "any content");
    assert_eq!(status, AgentStatus::Running);

    {
        let mut state = engine.shell_state();
        let marker = ShellMarker::from_parsed(
            ParsedMarker {
                kind: MarkerKind::PostExec,
                exit_code: Some(1),
            },
            1,
            0,
        );
        state.add_marker(marker);
    }
    let info = ShellPhaseInfo {
        phase: engine.shell_phase(),
        last_post_exec_exit_code: engine.last_post_exec_exit_code(),
    };
    assert_eq!(info.phase, ShellPhase::Output);
    assert_eq!(info.last_post_exec_exit_code, Some(1));
    let status = detector.detect(ProcessStatus::Running, Some(info), "output");
    assert_eq!(status, AgentStatus::Error);
}
