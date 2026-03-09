//! Status detector OSC 133 integration tests (pure OSC 133, no text patterns).
//! Run with: cargo test --test status_detector_osc133

use pmux::agent_status::AgentStatus;
use pmux::shell_integration::{ShellPhase, ShellPhaseInfo};
use pmux::status_detector::{ProcessStatus, StatusDetector};

#[test]
fn test_detect_with_process_exited() {
    let detector = StatusDetector::new();
    let status = detector.detect(ProcessStatus::Exited, None, "");
    assert_eq!(status, AgentStatus::Exited);
}

#[test]
fn test_detect_with_process_error() {
    let detector = StatusDetector::new();
    let status = detector.detect(ProcessStatus::Error, None, "");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_detect_with_osc133_running() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Running,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Running);
}

#[test]
fn test_detect_with_osc133_output_error() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(1),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_detect_with_osc133_output_success() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(0),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Idle);
}

#[test]
fn test_detect_with_osc133_input_idle() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Input,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Idle);
}

#[test]
fn test_detect_with_osc133_prompt_idle() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Prompt,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Idle);
}

#[test]
fn test_no_shell_info_returns_unknown() {
    let detector = StatusDetector::new();
    // Without OSC 133 info and no process lifecycle event, status is Unknown
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "any text"),
        AgentStatus::Unknown
    );
    assert_eq!(
        detector.detect(ProcessStatus::Running, None, "any text"),
        AgentStatus::Unknown
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
    let status = detector.detect(ProcessStatus::Exited, Some(info), "");
    assert_eq!(status, AgentStatus::Exited);

    // Process Error overrides OSC 133 Running
    let status = detector.detect(ProcessStatus::Error, Some(info), "");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_osc133_output_exit127_error() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: Some(127),
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Error);
}

#[test]
fn test_osc133_output_no_exit_code_idle() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Output,
        last_post_exec_exit_code: None,
    };
    // No exit code available → assume success → Idle
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Idle);
}

#[test]
fn test_osc133_unknown_phase() {
    let detector = StatusDetector::new();
    let info = ShellPhaseInfo {
        phase: ShellPhase::Unknown,
        last_post_exec_exit_code: None,
    };
    let status = detector.detect(ProcessStatus::Running, Some(info), "");
    assert_eq!(status, AgentStatus::Unknown);
}
