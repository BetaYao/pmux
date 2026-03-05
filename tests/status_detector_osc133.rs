//! Phase 4: Status detector OSC 133 shell phase integration tests.
//! Run with: cargo test --test status_detector_osc133

use pmux::agent_status::AgentStatus;
use pmux::shell_integration::{ShellPhase, ShellPhaseInfo};
use pmux::status_detector::{ProcessContext, ProcessStatus, StatusDetector};

#[test]
fn test_detect_with_process_exited() {
    let detector = StatusDetector::new();
    let status = detector.detect(ProcessStatus::Exited, None, "any content", ProcessContext::default());
    assert_eq!(status, AgentStatus::Exited);
}

#[test]
fn test_detect_with_process_error() {
    let detector = StatusDetector::new();
    let status = detector.detect(ProcessStatus::Error, None, "any content", ProcessContext::default());
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_detect_with_osc133_running() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Running,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "$ ls -la\nsome output", ProcessContext::default());
    assert_eq!(status, AgentStatus::Running);
}

#[test]
fn test_detect_with_osc133_output_error() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(1),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "command output", ProcessContext::default());
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_detect_with_osc133_output_success() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(0),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "$ echo done\ndone", ProcessContext::default());
    assert_eq!(status, AgentStatus::Idle);
}

#[test]
fn test_detect_with_osc133_input_waiting() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Input,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "$ ", ProcessContext::default());
    assert_eq!(status, AgentStatus::Waiting);
}

#[test]
fn test_detect_text_fallback() {
    let detector = StatusDetector::new();
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "AI is thinking", ProcessContext::default()),
        AgentStatus::Running
    );
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "? What next?", ProcessContext::default()),
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
    let status = detector.detect(ProcessStatus::Exited, Some(info), "any content", ProcessContext::default());
    assert_eq!(status, AgentStatus::Exited);

    // Process Error overrides OSC 133 Running
    let status = detector.detect(ProcessStatus::Error, Some(info), "any content", ProcessContext::default());
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
    let status = detector.detect(ProcessStatus::Running, Some(info), "error in log", ProcessContext::default());
    assert_eq!(status, AgentStatus::Running);
}

#[test]
fn test_integration_with_shell_phase_info() {
    let detector = StatusDetector::new();

    let info = ShellPhaseInfo {
        phase: ShellPhase::Unknown,
        last_post_exec_exit_code: None,
    };
    assert_eq!(
        detector.detect(ProcessStatus::Running, Some(info), "hello", ProcessContext::default()),
        AgentStatus::Idle
    );

    let info = ShellPhaseInfo {
        phase: ShellPhase::Running,
        last_post_exec_exit_code: None,
    };
    assert_eq!(
        detector.detect(ProcessStatus::Running, Some(info), "any content", ProcessContext::default()),
        AgentStatus::Running
    );

    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(1),
    };
    assert_eq!(
        detector.detect(ProcessStatus::Running, Some(info), "output", ProcessContext::default()),
        AgentStatus::Error
    );
}
