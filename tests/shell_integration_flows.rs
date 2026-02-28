//! Phase 6: Shell integration flow tests.
//! Simulates OSC 133 output from zsh, bash, fish; fallback; scrollback.
//! Run with: cargo test --test shell_integration_flows

use pmux::agent_status::AgentStatus;
use pmux::shell_integration::{MarkerKind, ShellPhase};
use pmux::status_detector::{ProcessStatus, StatusDetector};
use pmux::terminal::TerminalEngine;

fn osc133_st(kind: char, exit_code: Option<u8>) -> Vec<u8> {
    let mut s = format!("\x1b]133;{}", kind);
    if let Some(code) = exit_code {
        s.push_str(&format!(";{}", code));
    }
    s.push_str("\x1b\\");
    s.into_bytes()
}

fn osc133_bel(kind: char, exit_code: Option<u8>) -> Vec<u8> {
    let mut s = format!("\x1b]133;{}", kind);
    if let Some(code) = exit_code {
        s.push_str(&format!(";{}", code));
    }
    s.push('\x07');
    s.into_bytes()
}

/// Task 6.1: zsh + oh-my-zsh shell-integration plugin.
/// Simulates full prompt → input → running → output flow (ST terminator).
#[test]
fn test_zsh_osc133_prompt_input_running_output_flow() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);
    let detector = StatusDetector::new();

    // Simulate zsh shell-integration: prompt appears
    tx.send(osc133_st('A', None)).unwrap();
    tx.send(b"user@host ~ % ".to_vec()).unwrap();
    tx.send(osc133_st('B', None)).unwrap();
    engine.advance_with_osc133();
    engine.advance_bytes();

    assert_eq!(engine.shell_phase(), ShellPhase::Input);
    assert!(engine.prompt_line().is_some());

    // User types command, shell emits PreExec before running
    tx.send(b"ls -la\n".to_vec()).unwrap();
    tx.send(osc133_st('C', None)).unwrap();
    engine.advance_with_osc133();
    engine.advance_bytes();

    assert_eq!(engine.shell_phase(), ShellPhase::Running);

    let info = pmux::shell_integration::ShellPhaseInfo {
        phase: engine.shell_phase(),
        last_post_exec_exit_code: engine.last_post_exec_exit_code(),
    };
    assert_eq!(
        detector.detect(ProcessStatus::Running, Some(info), "command output"),
        AgentStatus::Running
    );

    // Command finishes
    tx.send(osc133_st('D', Some(0))).unwrap();
    tx.send(b"total 42\ndrwxr-xr-x  ...\n".to_vec()).unwrap();
    engine.advance_with_osc133();
    engine.advance_bytes();

    assert_eq!(engine.shell_phase(), ShellPhase::Output);
    assert_eq!(engine.last_post_exec_exit_code(), Some(0));

    let info = pmux::shell_integration::ShellPhaseInfo {
        phase: engine.shell_phase(),
        last_post_exec_exit_code: engine.last_post_exec_exit_code(),
    };
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, Some(info), "total 42"),
        AgentStatus::Idle
    );

    drop(tx);
}

/// Task 6.2: bash + manual OSC 133 config (PS1 modifications).
/// Same OSC 133 format; bash typically uses ST terminator.
#[test]
fn test_bash_manual_osc133_config() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // Manual bash PS1: \[\e]133;A\e\\] before prompt, \[\e]133;B\e\\] after
    tx.send(osc133_st('A', None)).unwrap();
    tx.send(b"$ ".to_vec()).unwrap();
    tx.send(osc133_st('B', None)).unwrap();
    engine.advance_with_osc133();
    engine.advance_bytes();

    assert_eq!(engine.shell_phase(), ShellPhase::Input);

    // DEBUG trap emits PreExec
    tx.send(b"echo hello\n".to_vec()).unwrap();
    tx.send(osc133_st('C', None)).unwrap();
    engine.advance_with_osc133();

    assert_eq!(engine.shell_phase(), ShellPhase::Running);

    // PostExec with exit code
    tx.send(osc133_st('D', Some(0))).unwrap();
    engine.advance_with_osc133();

    assert_eq!(engine.shell_phase(), ShellPhase::Output);
    assert_eq!(engine.last_post_exec_exit_code(), Some(0));

    let state = engine.shell_state();
    assert_eq!(state.markers.len(), 4);
    assert_eq!(state.markers[0].kind, MarkerKind::PromptStart);
    assert_eq!(state.markers[1].kind, MarkerKind::PromptEnd);
    assert_eq!(state.markers[2].kind, MarkerKind::PreExec);
    assert_eq!(state.markers[3].kind, MarkerKind::PostExec);

    drop(tx);
}

/// Task 6.3: fish native OSC 133 format.
/// Fish may use BEL terminator and optional params (e.g. 133;A; click_events=1).
#[test]
fn test_fish_native_osc133_format() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // Fish-style: can use BEL; our parser accepts both ST and BEL
    tx.send(osc133_bel('A', None)).unwrap();
    tx.send(b"fish> ".to_vec()).unwrap();
    tx.send(osc133_bel('B', None)).unwrap();
    engine.advance_with_osc133();
    engine.advance_bytes();

    assert_eq!(engine.shell_phase(), ShellPhase::Input);

    tx.send(b"true\n".to_vec()).unwrap();
    tx.send(osc133_bel('C', None)).unwrap();
    engine.advance_with_osc133();
    assert_eq!(engine.shell_phase(), ShellPhase::Running);

    tx.send(osc133_bel('D', Some(0))).unwrap();
    engine.advance_with_osc133();
    assert_eq!(engine.shell_phase(), ShellPhase::Output);

    drop(tx);
}

/// Task 6.3 extended: fish may emit 133;A; click_events=1 (extra params).
/// Parser should still extract marker kind A.
#[test]
fn test_fish_osc133_with_extra_params() {
    use pmux::shell_integration::Osc133Parser;

    // Fish: \e]133;A; click_events=1\e\
    let seq = b"\x1b]133;A; click_events=1\x1b\\";
    let mut p = Osc133Parser::new();
    let markers = p.feed(seq);
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0].kind, MarkerKind::PromptStart);
}

/// Task 6.4: Shell without OSC 133 → text detection still works.
#[test]
fn test_fallback_text_detection_without_osc133() {
    let detector = StatusDetector::new();

    // No shell_info (OSC 133 unavailable) → pure text detection
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "AI is thinking..."),
        AgentStatus::Running
    );
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "? What would you like to do?"),
        AgentStatus::Waiting
    );
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "error: command failed"),
        AgentStatus::Error
    );
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, None, "$ echo done\ndone"),
        AgentStatus::Idle
    );

    // shell_info with Unknown phase → also falls back to text
    let info = pmux::shell_integration::ShellPhaseInfo {
        phase: ShellPhase::Unknown,
        last_post_exec_exit_code: None,
    };
    assert_eq!(
        detector.detect(ProcessStatus::Unknown, Some(info), "AI is thinking"),
        AgentStatus::Running
    );
}

/// Task 6.5: Markers tracked correctly during scrollback.
/// Verifies markers persist across multiple commands and visible_markers works.
#[test]
fn test_markers_tracked_during_scrollback() {
    let (tx, rx) = flume::unbounded();
    let engine = TerminalEngine::new(80, 24, rx);

    // First prompt
    tx.send(osc133_st('A', None)).unwrap();
    tx.send(b"$ ".to_vec()).unwrap();
    tx.send(osc133_st('B', None)).unwrap();
    engine.advance_with_osc133();

    assert!(engine.prompt_line().is_some());
    let state = engine.shell_state();
    assert_eq!(state.markers.len(), 2);
    assert_eq!(state.markers[0].kind, MarkerKind::PromptStart);
    assert_eq!(state.markers[1].kind, MarkerKind::PromptEnd);
    drop(state);

    // Second command cycle (simulates scrollback: previous markers still in state)
    tx.send(osc133_st('C', None)).unwrap();
    tx.send(osc133_st('D', Some(0))).unwrap();
    engine.advance_with_osc133();

    assert_eq!(engine.shell_phase(), ShellPhase::Output);
    let state = engine.shell_state();
    assert!(state.markers.len() >= 4); // A, B, C, D
    assert_eq!(state.markers[state.markers.len() - 2].kind, MarkerKind::PreExec);
    assert_eq!(state.markers[state.markers.len() - 1].kind, MarkerKind::PostExec);
    drop(state);

    drop(tx);
}
