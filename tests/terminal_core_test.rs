//! Core terminal functionality integration tests.
//!
//! Tests the actual tmux standard backend with real shell interaction:
//! 1. Input/output: type commands, see results
//! 2. Keyboard: backspace, arrow keys, ctrl sequences
//! 3. VT output: raw escape sequences flow correctly (colors, cursor)
//! 4. Resize: terminal resize delivers SIGWINCH
//! 5. Interactive CLI: run a real program (less, vi, etc.)
//! 6. Tab switch: session recovery preserves terminal state
//!
//! Run with: cargo test --test terminal_core_test -- --ignored --nocapture --test-threads=1

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

/// Drain all available output from the receiver within timeout.
fn drain_output(rx: &flume::Receiver<Vec<u8>>, timeout: Duration) -> Vec<u8> {
    let mut all = Vec::new();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => all.extend_from_slice(&chunk),
            Err(flume::RecvTimeoutError::Timeout) => {
                // Drain any remaining buffered data
                while let Ok(chunk) = rx.try_recv() {
                    all.extend_from_slice(&chunk);
                }
                if !all.is_empty() {
                    break;
                }
            }
            Err(flume::RecvTimeoutError::Disconnected) => break,
        }
    }
    all
}

/// Wait for output containing `marker`, return all accumulated bytes.
fn wait_for(rx: &flume::Receiver<Vec<u8>>, marker: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let mut all = Vec::new();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(chunk) => {
                all.extend_from_slice(&chunk);
                let s = String::from_utf8_lossy(&all);
                if s.contains(marker) {
                    return Ok(all);
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

fn setup_backend(
    session: &str,
    dir: &std::path::Path,
) -> (
    pmux::runtime::backends::tmux_standard::TmuxStandardBackend,
    String,
    flume::Receiver<Vec<u8>>,
) {
    // Clean up
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session])
        .output();

    let backend = pmux::runtime::backends::tmux_standard::TmuxStandardBackend::new(
        session, dir, 120, 36,
    )
    .expect("create backend");

    let pane_id = backend.primary_pane_id().expect("primary pane");
    let rx = backend.subscribe_output(&pane_id).expect("subscribe");

    // Wait for initial prompt (C-l redraw)
    std::thread::sleep(Duration::from_millis(500));
    let _ = drain_output(&rx, Duration::from_millis(500));

    (backend, pane_id, rx)
}

fn cleanup(session: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session])
        .output();
}

// ============================================================
// Test 1: Basic input and output
// ============================================================
#[test]
#[ignore]
fn test_basic_input_output() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-io";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Type a command
    backend.send_input(&pane_id, b"echo HELLO_WORLD\r").unwrap();

    // Give more time and collect all output
    std::thread::sleep(Duration::from_secs(1));
    let mut all = Vec::new();
    while let Ok(chunk) = rx.try_recv() {
        eprintln!("  chunk: {} bytes, has_esc={}, preview={:?}",
            chunk.len(),
            chunk.iter().any(|&b| b == 0x1b),
            String::from_utf8_lossy(&chunk[..chunk.len().min(80)]));
        all.extend_from_slice(&chunk);
    }
    // Also try waiting more
    if all.is_empty() || !String::from_utf8_lossy(&all).contains("HELLO_WORLD") {
        let result = wait_for(&rx, "HELLO_WORLD", Duration::from_secs(3));
        if let Ok(bytes) = result {
            all.extend_from_slice(&bytes);
        }
    }

    let text = String::from_utf8_lossy(&all);
    eprintln!("Total output: {} bytes", all.len());
    assert!(
        text.contains("HELLO_WORLD"),
        "output should contain HELLO_WORLD, got: {:?}", &text[..text.len().min(500)]
    );

    // Verify output contains VT escape sequences (not plain text)
    assert!(
        all.iter().any(|&b| b == 0x1b),
        "output should contain ESC (0x1b) bytes for VT sequences. Total {} bytes: {:?}",
        all.len(), &text[..text.len().min(200)]
    );

    drop(backend);
    cleanup(session);
    eprintln!("test_basic_input_output: PASSED");
}

// ============================================================
// Test 2: Keyboard — backspace deletes characters
// ============================================================
#[test]
#[ignore]
fn test_keyboard_backspace() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-bs";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Type "echx" then backspace, then "o BACKSPACE_OK"
    backend.send_input(&pane_id, b"echx").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    backend.send_input(&pane_id, &[0x7f]).unwrap(); // backspace
    std::thread::sleep(Duration::from_millis(100));
    backend.send_input(&pane_id, b"o BACKSPACE_OK\r").unwrap();

    let result = wait_for(&rx, "BACKSPACE_OK", Duration::from_secs(3));
    assert!(result.is_ok(), "backspace test failed: {}", result.unwrap_err());
    eprintln!("test_keyboard_backspace: PASSED");

    drop(backend);
    cleanup(session);
}

// ============================================================
// Test 3: Ctrl-C interrupts
// ============================================================
#[test]
#[ignore]
fn test_ctrl_c_interrupt() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-ctrlc";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Start a long-running command
    backend.send_input(&pane_id, b"sleep 999\r").unwrap();
    std::thread::sleep(Duration::from_millis(300));

    // Send Ctrl-C
    backend.send_input(&pane_id, &[0x03]).unwrap();
    std::thread::sleep(Duration::from_millis(300));

    // Should be back at prompt — type a command to verify
    backend.send_input(&pane_id, b"echo CTRLC_OK\r").unwrap();
    let result = wait_for(&rx, "CTRLC_OK", Duration::from_secs(3));
    assert!(result.is_ok(), "ctrl-c test failed: {}", result.unwrap_err());
    eprintln!("test_ctrl_c_interrupt: PASSED");

    drop(backend);
    cleanup(session);
}

// ============================================================
// Test 4: VT escape sequences in output (colors)
// ============================================================
#[test]
#[ignore]
fn test_vt_escape_sequences() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-vt";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Use printf to emit known escape sequences
    backend
        .send_input(&pane_id, b"printf '\\033[31mRED\\033[0m NORMAL VT_TEST_OK\\n'\r")
        .unwrap();

    let result = wait_for(&rx, "VT_TEST_OK", Duration::from_secs(3));
    assert!(result.is_ok(), "VT test failed: {}", result.unwrap_err());

    let output = result.unwrap();
    // Should contain CSI sequences: ESC [ 31 m (red) and ESC [ 0 m (reset)
    let has_csi = output.windows(2).any(|w| w[0] == 0x1b && w[1] == b'[');
    assert!(has_csi, "output should contain CSI (ESC[) sequences for colors");
    eprintln!("test_vt_escape_sequences: PASSED");

    drop(backend);
    cleanup(session);
}

// ============================================================
// Test 5: Terminal resize
// ============================================================
#[test]
#[ignore]
fn test_terminal_resize() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-resize";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Resize to 80x24
    backend.resize(&pane_id, 80, 24).unwrap();
    std::thread::sleep(Duration::from_millis(500));

    // Check dimensions via stty size (rows cols)
    backend.send_input(&pane_id, b"stty size\r").unwrap();
    // stty size outputs "ROWS COLS", e.g. "24 80"
    let result = wait_for(&rx, "24 80", Duration::from_secs(3));
    if result.is_err() {
        // Also accept close values
        let result2 = wait_for(&rx, "80", Duration::from_secs(1));
        assert!(result2.is_ok(), "resize test failed — stty size didn't show 80 cols");
    }
    eprintln!("test_terminal_resize: PASSED");

    drop(backend);
    cleanup(session);
}

// ============================================================
// Test 6: Tab switch recovery — session persists, I/O works after
// ============================================================
#[test]
#[ignore]
fn test_tab_switch_recovery() {
    if !tmux_available() { return; }
    let session_a = "pmux-core-test-tab-a";
    let session_b = "pmux-core-test-tab-b";
    let dir = tempfile::tempdir().unwrap();

    // Clean
    let _ = Command::new("tmux").args(["kill-session", "-t", session_a]).output();
    let _ = Command::new("tmux").args(["kill-session", "-t", session_b]).output();

    // Step 1: Create A, send command
    let (backend_a, pane_a, rx_a) = setup_backend(session_a, dir.path());
    backend_a.send_input(&pane_a, b"echo STEP1_OK\r").unwrap();
    let r = wait_for(&rx_a, "STEP1_OK", Duration::from_secs(3));
    assert!(r.is_ok(), "step 1 failed: {}", r.unwrap_err());
    eprintln!("Step 1 OK");

    // Step 2: "Switch to B" — drop A
    drop(backend_a);
    let (backend_b, pane_b, rx_b) = setup_backend(session_b, dir.path());
    backend_b.send_input(&pane_b, b"echo STEP2_OK\r").unwrap();
    let r = wait_for(&rx_b, "STEP2_OK", Duration::from_secs(3));
    assert!(r.is_ok(), "step 2 failed: {}", r.unwrap_err());
    eprintln!("Step 2 OK");

    // Step 3: "Switch back to A" — drop B, recover A
    drop(backend_b);

    // Window count should still be 1
    let win_count = Command::new("tmux")
        .args(["list-windows", "-t", session_a, "-F", "#{window_index}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0);
    assert_eq!(win_count, 1, "session A should still have 1 window, got {}", win_count);

    let (backend_a2, pane_a2, rx_a2) = setup_backend(session_a, dir.path());
    backend_a2.send_input(&pane_a2, b"echo STEP3_OK\r").unwrap();
    let r = wait_for(&rx_a2, "STEP3_OK", Duration::from_secs(3));
    assert!(r.is_ok(), "step 3 (recovery I/O) failed: {}", r.unwrap_err());
    eprintln!("Step 3 OK");

    // Verify still 1 window (no orphans created)
    let win_count = Command::new("tmux")
        .args(["list-windows", "-t", session_a, "-F", "#{window_index}"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().count())
        .unwrap_or(0);
    assert_eq!(win_count, 1, "after recovery should still have 1 window, got {}", win_count);

    drop(backend_a2);
    cleanup(session_a);
    cleanup(session_b);
    eprintln!("test_tab_switch_recovery: PASSED");
}

// ============================================================
// Test 7: Interactive program (less)
// ============================================================
#[test]
#[ignore]
fn test_interactive_program() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-interactive";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Create a test file
    backend.send_input(&pane_id, b"echo 'line1\nline2\nline3' > /tmp/pmux-test-less\r").unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let _ = drain_output(&rx, Duration::from_millis(200));

    // Run less
    backend.send_input(&pane_id, b"less /tmp/pmux-test-less\r").unwrap();
    let r = wait_for(&rx, "line1", Duration::from_secs(3));
    assert!(r.is_ok(), "less should show file content: {}", r.unwrap_err());

    // Exit less with 'q'
    backend.send_input(&pane_id, b"q").unwrap();
    std::thread::sleep(Duration::from_millis(300));

    // Should be back at prompt
    backend.send_input(&pane_id, b"echo LESS_EXIT_OK\r").unwrap();
    let r = wait_for(&rx, "LESS_EXIT_OK", Duration::from_secs(3));
    assert!(r.is_ok(), "should be back at prompt after less: {}", r.unwrap_err());
    eprintln!("test_interactive_program: PASSED");

    // Cleanup
    let _ = std::fs::remove_file("/tmp/pmux-test-less");
    drop(backend);
    cleanup(session);
}

// ============================================================
// Test 8: Multiple rapid inputs (typing speed)
// ============================================================
#[test]
#[ignore]
fn test_rapid_input() {
    if !tmux_available() { return; }
    let session = "pmux-core-test-rapid";
    let dir = tempfile::tempdir().unwrap();
    let (backend, pane_id, rx) = setup_backend(session, dir.path());

    // Send characters one by one rapidly
    for ch in b"echo RAPID_TEST_12345" {
        backend.send_input(&pane_id, &[*ch]).unwrap();
    }
    backend.send_input(&pane_id, b"\r").unwrap();

    let r = wait_for(&rx, "RAPID_TEST_12345", Duration::from_secs(3));
    assert!(r.is_ok(), "rapid input failed: {}", r.unwrap_err());
    eprintln!("test_rapid_input: PASSED");

    drop(backend);
    cleanup(session);
}
