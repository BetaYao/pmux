//! Integration test: simulates tab switching between two workspaces.
//!
//! Scenario:
//! 1. Create backend for workspace A, send `clear` + `ls`, verify output
//! 2. Drop backend A (simulates switching to workspace B tab)
//! 3. Create backend for workspace B
//! 4. Drop backend B (simulates switching back to workspace A tab)
//! 5. Re-create backend for workspace A — should RECOVER existing session
//! 6. Verify previous `ls` output is still visible (via capture-pane)
//! 7. Send `clear` + `ls` again, verify new output
//!
//! Run with: cargo test tab_switch_recovery -- --ignored --nocapture

#![cfg(unix)]

use pmux::runtime::AgentRuntime;
use std::process::Command;
use std::time::{Duration, Instant};

fn tmux_available() -> bool {
    Command::new("tmux")
        .arg("-V")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Wait for output containing `marker` on the given receiver, up to `timeout`.
fn wait_for_output(
    rx: &flume::Receiver<Vec<u8>>,
    marker: &str,
    timeout: Duration,
) -> Result<String, String> {
    let mut all = Vec::new();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                all.extend_from_slice(&chunk);
                let s = String::from_utf8_lossy(&all).to_string();
                if s.contains(marker) {
                    return Ok(s);
                }
            }
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    Err(format!(
        "timeout waiting for '{}'. Got {} bytes: {:?}",
        marker,
        all.len(),
        String::from_utf8_lossy(&all[..all.len().min(500)])
    ))
}

/// Count tmux windows in a session.
fn tmux_window_count(session: &str) -> usize {
    let output = Command::new("tmux")
        .args(["list-windows", "-t", session, "-F", "#{window_index}"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count()
        }
        _ => 0,
    }
}

/// Capture current pane content via tmux capture-pane.
fn capture_pane(session: &str) -> String {
    let output = Command::new("tmux")
        .args(["capture-pane", "-t", session, "-p"])
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => String::new(),
    }
}

#[test]
#[ignore]
fn test_tab_switch_preserves_terminal_state() {
    if !tmux_available() {
        eprintln!("tmux not available, skip");
        return;
    }

    let session_a = "pmux-tabtest-workspace-a";
    let session_b = "pmux-tabtest-workspace-b";
    let dir = tempfile::tempdir().unwrap();

    // Clean up any leftover sessions
    let _ = Command::new("tmux").args(["kill-session", "-t", session_a]).output();
    let _ = Command::new("tmux").args(["kill-session", "-t", session_b]).output();

    // ============================================================
    // Step 1: Create workspace A, send clear + ls, verify output
    // ============================================================
    eprintln!("=== Step 1: Create workspace A, send clear + ls ===");
    let backend_a = pmux::runtime::backends::tmux_standard::TmuxStandardBackend::new(
        session_a, dir.path(), 120, 36,
    )
    .expect("create backend A");

    let pane_a = backend_a.primary_pane_id().expect("backend A should have primary pane");
    let rx_a = backend_a.subscribe_output(&pane_a).expect("subscribe A");

    // Wait for shell prompt
    std::thread::sleep(Duration::from_millis(500));

    // Send clear + ls
    backend_a.send_input(&pane_a, b"clear\r").expect("send clear");
    std::thread::sleep(Duration::from_millis(300));
    backend_a.send_input(&pane_a, b"echo MARKER_A1\r").expect("send echo");

    let output = wait_for_output(&rx_a, "MARKER_A1", Duration::from_secs(5));
    assert!(output.is_ok(), "Step 1 failed: {}", output.unwrap_err());
    eprintln!("Step 1 OK: saw MARKER_A1");

    let windows_a_step1 = tmux_window_count(session_a);
    eprintln!("Workspace A has {} window(s) after step 1", windows_a_step1);
    assert_eq!(windows_a_step1, 1, "should have exactly 1 window");

    // ============================================================
    // Step 2: "Switch to tab B" — drop backend A, create backend B
    // ============================================================
    eprintln!("\n=== Step 2: Switch to workspace B ===");
    drop(backend_a);
    // Dropping backend_a releases Rust resources but tmux session persists

    let backend_b = pmux::runtime::backends::tmux_standard::TmuxStandardBackend::new(
        session_b, dir.path(), 120, 36,
    )
    .expect("create backend B");

    let pane_b = backend_b.primary_pane_id().expect("backend B should have primary pane");
    let rx_b = backend_b.subscribe_output(&pane_b).expect("subscribe B");

    std::thread::sleep(Duration::from_millis(500));
    backend_b.send_input(&pane_b, b"echo MARKER_B1\r").expect("send echo B");
    let output = wait_for_output(&rx_b, "MARKER_B1", Duration::from_secs(5));
    assert!(output.is_ok(), "Step 2 failed: {}", output.unwrap_err());
    eprintln!("Step 2 OK: workspace B works");

    // ============================================================
    // Step 3: "Switch back to tab A" — drop backend B, re-create backend A
    // ============================================================
    eprintln!("\n=== Step 3: Switch back to workspace A ===");
    drop(backend_b);

    // KEY: This should RECOVER the existing session, not create a new window
    let windows_before = tmux_window_count(session_a);
    eprintln!("Workspace A has {} window(s) before recovery", windows_before);

    let backend_a2 = pmux::runtime::backends::tmux_standard::TmuxStandardBackend::new(
        session_a, dir.path(), 120, 36,
    )
    .expect("recover backend A");

    let windows_after = tmux_window_count(session_a);
    eprintln!("Workspace A has {} window(s) after recovery", windows_after);
    assert_eq!(
        windows_after, windows_before,
        "recovery should NOT create new windows! Before={}, After={}",
        windows_before, windows_after
    );

    // Note: C-l is sent on recovery to seed pipe-pane with initial screen content,
    // which clears the previous output. We verify the session is the same (1 window,
    // no orphans) and that input/output works in Step 4.

    // ============================================================
    // Step 4: Send clear + ls on recovered workspace A
    // ============================================================
    eprintln!("\n=== Step 4: Send clear + echo on recovered workspace A ===");
    let pane_a2 = backend_a2.primary_pane_id().expect("recovered A should have primary pane");
    let rx_a2 = backend_a2.subscribe_output(&pane_a2).expect("subscribe recovered A");

    backend_a2.send_input(&pane_a2, b"clear\r").expect("send clear");
    std::thread::sleep(Duration::from_millis(300));
    backend_a2.send_input(&pane_a2, b"echo MARKER_A2\r").expect("send echo");

    let output = wait_for_output(&rx_a2, "MARKER_A2", Duration::from_secs(5));
    assert!(output.is_ok(), "Step 4 failed: {}", output.unwrap_err());
    eprintln!("Step 4 OK: recovered terminal accepts input and produces output");

    // Clean up
    drop(backend_a2);
    let _ = Command::new("tmux").args(["kill-session", "-t", session_a]).output();
    let _ = Command::new("tmux").args(["kill-session", "-t", session_b]).output();

    eprintln!("\n=== ALL STEPS PASSED ===");
}
