/// Input handler for forwarding keyboard events to tmux sessions
pub struct InputHandler {
    /// The tmux session name (e.g., "sdlc-myproject")
    session_name: String,
}

impl InputHandler {
    /// Create a new InputHandler for the given session
    pub fn new(session_name: String) -> Self {
        Self { session_name }
    }

    /// Get the session name
    pub fn session_name(&self) -> &str {
        &self.session_name
    }

    /// Send a key to the active pane in the tmux session
    /// Returns Ok(()) if the key was sent, or an error message if it failed
    /// Errors are logged but do not crash the application
    pub fn send_key(&self, key: &str) -> Result<(), String> {
        self.send_key_to_target(&self.session_name, key)
    }

    /// Send a key to a specific tmux target (e.g. "session:window" or "session:window.pane")
    /// When use_literal is true, uses -l for literal character sending (avoids tmux key interpretation).
    pub fn send_key_to_target(&self, target: &str, key: &str) -> Result<(), String> {
        self.send_key_to_target_with_literal(target, key, false)
    }

    /// Send a key with explicit literal flag (for regular chars, use true to avoid tmux interpretation).
    pub fn send_key_to_target_with_literal(
        &self,
        target: &str,
        key: &str,
        use_literal: bool,
    ) -> Result<(), String> {
        use std::process::Command;

        let mut args = vec!["send-keys", "-t", target];
        if use_literal {
            args.push("-l");
        }
        args.push("--"); // end flags so key is never parsed as option
        args.push(key);

        let output = Command::new("tmux").args(args).output();

        match output {
            Ok(result) => {
                if result.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&result.stderr);
                    eprintln!("InputHandler: tmux send-keys failed: {}", stderr);
                    Err(stderr.to_string())
                }
            }
            Err(e) => {
                eprintln!("InputHandler: Failed to execute tmux send-keys: {}", e);
                Err(format!("Failed to execute tmux send-keys: {}", e))
            }
        }
    }
}

/// Convert a GPUI key name to a tmux send-keys string.
/// Returns None if the key should be handled by pmux (app shortcut).
/// Returns Some((tmux_key, use_literal)) - use_literal=true means use tmux -l for literal sending.
pub fn key_to_tmux(key: &str, modifiers_cmd: bool) -> Option<(String, bool)> {
    // pmux shortcuts: Cmd+B, Cmd+N, Cmd+W — intercept
    if modifiers_cmd {
        return None;
    }
    let (tmux_key, use_literal) = match key {
        "enter" | "return" => ("Enter".to_string(), false),
        "backspace" => ("BSpace".to_string(), false),
        "escape" => ("Escape".to_string(), false),
        "tab" => ("Tab".to_string(), false),
        "up" => ("Up".to_string(), false),
        "down" => ("Down".to_string(), false),
        "left" => ("Left".to_string(), false),
        "right" => ("Right".to_string(), false),
        "home" => ("Home".to_string(), false),
        "end" => ("End".to_string(), false),
        "pageup" => ("PPage".to_string(), false),
        "pagedown" => ("NPage".to_string(), false),
        other => (other.to_string(), true),
    };
    Some((tmux_key, use_literal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_handler_creation() {
        let handler = InputHandler::new("sdlc-myproject".to_string());
        assert_eq!(handler.session_name(), "sdlc-myproject");
    }

    #[test]
    fn test_send_key_api_exists() {
        // Verify the send_key method has the correct signature
        let handler = InputHandler::new("test-session".to_string());
        let _fn_ptr: fn(&InputHandler, &str) -> Result<(), String> = InputHandler::send_key;
        // We can't test actual tmux execution without tmux running,
        // but we verify the API exists by checking the method signature
        let _ = handler.send_key("test-key");
    }

    #[test]
    fn test_enter_key() {
        assert_eq!(key_to_tmux("enter", false), Some(("Enter".to_string(), false)));
    }

    #[test]
    fn test_backspace_key() {
        assert_eq!(key_to_tmux("backspace", false), Some(("BSpace".to_string(), false)));
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(key_to_tmux("up", false), Some(("Up".to_string(), false)));
        assert_eq!(key_to_tmux("down", false), Some(("Down".to_string(), false)));
        assert_eq!(key_to_tmux("left", false), Some(("Left".to_string(), false)));
        assert_eq!(key_to_tmux("right", false), Some(("Right".to_string(), false)));
    }

    #[test]
    fn test_escape_tab() {
        assert_eq!(key_to_tmux("escape", false), Some(("Escape".to_string(), false)));
        assert_eq!(key_to_tmux("tab", false), Some(("Tab".to_string(), false)));
    }

    #[test]
    fn test_cmd_key_intercepted() {
        assert_eq!(key_to_tmux("b", true), None);
        assert_eq!(key_to_tmux("n", true), None);
    }

    #[test]
    fn test_regular_char_passthrough() {
        assert_eq!(key_to_tmux("a", false), Some(("a".to_string(), true)));
        assert_eq!(key_to_tmux("z", false), Some(("z".to_string(), true)));
    }
}
