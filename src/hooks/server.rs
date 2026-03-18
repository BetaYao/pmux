//! hooks/server.rs - Local HTTP webhook server for receiving AI tool hook events

use std::io::Read;
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use serde::Deserialize;

use crate::runtime::{HookEvent, RuntimeEvent, SharedEventBus};

/// Unified hook payload accepted from all tools.
/// Superset of Claude Code, Gemini CLI, Codex, and Aider payloads.
#[derive(Debug, Default, Deserialize)]
pub struct HookPayload {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub tool_name: Option<String>,
    /// Injected by pmux curl commands to identify the source tool
    #[serde(default)]
    pub pmux_source: Option<String>,
}

impl HookPayload {
    /// Infer which tool sent this event
    pub fn infer_source(&self) -> String {
        if let Some(ref src) = self.pmux_source {
            return src.clone();
        }
        if self.hook_event_name == "aider_waiting" {
            return "aider".to_string();
        }
        "unknown".to_string()
    }

    /// Map hook_event_name to AgentStatus string
    pub fn to_status(&self) -> Option<&'static str> {
        match self.hook_event_name.as_str() {
            "PreToolUse" | "BeforeTool" | "SessionStart" => Some("Running"),
            "Stop" | "AfterAgent" | "SessionEnd"         => Some("Idle"),
            "Notification"                               => Some("Waiting"),
            "aider_waiting"                              => Some("Waiting"),
            _ => None,
        }
    }
}

pub struct WebhookServer {
    port: u16,
    event_bus: SharedEventBus,
}

impl WebhookServer {
    pub fn new(port: u16, event_bus: SharedEventBus) -> Self {
        Self { port, event_bus }
    }

    /// Start the HTTP server in a background thread. Returns immediately.
    pub fn start(self) -> Result<(), String> {
        let addr = format!("127.0.0.1:{}", self.port);
        let listener = TcpListener::bind(&addr)
            .map_err(|e| format!("webhook server bind {} failed: {}", addr, e))?;

        let event_bus = Arc::clone(&self.event_bus);
        thread::Builder::new()
            .name("pmux-webhook".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let bus = Arc::clone(&event_bus);
                    thread::spawn(move || {
                        handle_connection(&mut stream, &bus);
                    });
                }
            })
            .map_err(|e| e.to_string())?;

        Ok(())
    }
}

fn handle_connection(stream: &mut std::net::TcpStream, event_bus: &SharedEventBus) {
    use std::io::Write;

    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return,
    };
    let raw = match std::str::from_utf8(&buf[..n]) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Only accept POST /webhook
    if !raw.starts_with("POST /webhook") {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        return;
    }

    // Extract JSON body (after the blank line separating headers from body)
    let body = match raw.find("\r\n\r\n") {
        Some(pos) => &raw[pos + 4..],
        None => return,
    };

    let payload: HookPayload = match serde_json::from_str(body) {
        Ok(p) => p,
        Err(_) => {
            let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n");
            return;
        }
    };

    if !payload.hook_event_name.is_empty() || payload.pmux_source.is_some() {
        event_bus.publish(RuntimeEvent::HookEvent(HookEvent {
            session_id: payload.session_id.clone(),
            cwd: payload.cwd.clone(),
            hook_event_name: payload.hook_event_name.clone(),
            tool_name: payload.tool_name.clone(),
            source_tool: payload.infer_source(),
        }));
    }

    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_to_status_stop() {
        let p = HookPayload {
            hook_event_name: "Stop".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), Some("Idle"));
    }

    #[test]
    fn test_payload_to_status_pre_tool_use() {
        let p = HookPayload {
            hook_event_name: "PreToolUse".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), Some("Running"));
    }

    #[test]
    fn test_payload_to_status_aider_waiting() {
        let p = HookPayload {
            hook_event_name: "aider_waiting".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), Some("Waiting"));
        assert_eq!(p.infer_source(), "aider");
    }

    #[test]
    fn test_payload_infer_source_from_field() {
        let p = HookPayload {
            pmux_source: Some("gemini_cli".to_string()),
            ..Default::default()
        };
        assert_eq!(p.infer_source(), "gemini_cli");
    }

    #[test]
    fn test_payload_unknown_event_has_no_status() {
        let p = HookPayload {
            hook_event_name: "SomeUnknownEvent".to_string(),
            ..Default::default()
        };
        assert_eq!(p.to_status(), None);
    }

    #[test]
    fn test_webhook_server_receives_event() {
        use std::io::Write;
        use std::net::TcpStream;
        use std::time::Duration;

        // Bind on port 0 to get a free port
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);

        let bus = Arc::new(crate::runtime::EventBus::new(16));
        let rx = bus.subscribe();

        WebhookServer::new(port, Arc::clone(&bus)).start().unwrap();
        std::thread::sleep(Duration::from_millis(50));

        let body = r#"{"session_id":"s1","cwd":"/repo","hook_event_name":"Stop"}"#;
        let request = format!(
            "POST /webhook HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let mut conn = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
        conn.write_all(request.as_bytes()).unwrap();
        drop(conn);

        let ev = rx.recv_timeout(Duration::from_millis(500)).expect("expected event");
        match ev {
            RuntimeEvent::HookEvent(h) => {
                assert_eq!(h.session_id, "s1");
                assert_eq!(h.hook_event_name, "Stop");
                assert_eq!(h.cwd, "/repo");
            }
            _ => panic!("expected HookEvent"),
        }
    }
}
