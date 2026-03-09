// Standalone test crate for status_detector — compiles without gpui.
//
// Uses #[path] to include source files from the main crate so we test the
// real code, not duplicated copies.

#[path = "../../../src/agent_status.rs"]
pub mod agent_status;

#[path = "../../../src/shell_integration.rs"]
pub mod shell_integration;

#[path = "../../../src/status_detector.rs"]
pub mod status_detector;
