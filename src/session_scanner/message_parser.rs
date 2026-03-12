//! message_parser.rs — Parse JSONL lines from Claude Code session files.
//!
//! Extracts structured SessionEvent from each JSONL line, focusing on
//! status-relevant messages (user input, assistant thinking/tool_use/text, turn end).

use serde::Deserialize;

/// Events extracted from JSONL messages that are relevant for agent status detection.
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// User sent a new message (enqueue)
    UserInput {
        session_id: String,
        timestamp: String,
    },
    /// Assistant is thinking (has thinking block in content)
    Thinking { session_id: String },
    /// Assistant is using a tool
    ToolUse {
        session_id: String,
        tool_name: String,
        tool_id: String,
    },
    /// Tool result received
    ToolResult {
        session_id: String,
        tool_id: String,
        is_error: bool,
    },
    /// Assistant produced text output
    TextOutput { session_id: String },
    /// Turn ended (system stop message)
    TurnEnd {
        session_id: String,
        timestamp: String,
    },
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    operation: Option<String>,
    timestamp: Option<String>,
    subtype: Option<String>,
    message: Option<RawInnerMessage>,
}

#[derive(Deserialize)]
struct RawInnerMessage {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

/// Parse a single JSONL line and extract a session event.
/// Returns (session_id, event) if the line is relevant, None otherwise.
pub fn parse_jsonl_line(line: &str) -> Option<(String, SessionEvent)> {
    let raw: RawMessage = serde_json::from_str(line).ok()?;
    let session_id = raw.session_id?;

    match raw.msg_type.as_deref()? {
        "queue-operation" if raw.operation.as_deref() == Some("enqueue") => {
            Some((
                session_id.clone(),
                SessionEvent::UserInput {
                    session_id,
                    timestamp: raw.timestamp.unwrap_or_default(),
                },
            ))
        }
        "system" if raw.subtype.as_deref() == Some("stop_hook_summary") => {
            Some((
                session_id.clone(),
                SessionEvent::TurnEnd {
                    session_id,
                    timestamp: raw.timestamp.unwrap_or_default(),
                },
            ))
        }
        "assistant" => {
            let msg = raw.message?;
            if msg.role.as_deref() != Some("assistant") {
                return None;
            }
            let content = msg.content.as_ref()?.as_array()?;

            // Check content blocks — return the first relevant event found.
            // Priority: thinking > tool_use > text
            for block in content {
                let block_type = block.get("type")?.as_str()?;
                match block_type {
                    "thinking" => {
                        return Some((
                            session_id.clone(),
                            SessionEvent::Thinking { session_id },
                        ));
                    }
                    "tool_use" => {
                        let name = block
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let id = block
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        return Some((
                            session_id.clone(),
                            SessionEvent::ToolUse {
                                session_id,
                                tool_name: name,
                                tool_id: id,
                            },
                        ));
                    }
                    "text" => {
                        return Some((
                            session_id.clone(),
                            SessionEvent::TextOutput { session_id },
                        ));
                    }
                    _ => {}
                }
            }
            None
        }
        "user" => {
            // tool_result is inside user messages
            let msg = raw.message?;
            if msg.role.as_deref() != Some("user") {
                return None;
            }
            let content = msg.content.as_ref()?.as_array()?;
            for block in content {
                if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                    let tool_id = block
                        .get("tool_use_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let is_error = block
                        .get("is_error")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    return Some((
                        session_id.clone(),
                        SessionEvent::ToolResult {
                            session_id,
                            tool_id,
                            is_error,
                        },
                    ));
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_enqueue() {
        let line = r#"{"type":"queue-operation","operation":"enqueue","sessionId":"abc-123","timestamp":"2026-03-11T10:00:00Z"}"#;
        let (sid, event) = parse_jsonl_line(line).unwrap();
        assert_eq!(sid, "abc-123");
        assert!(matches!(event, SessionEvent::UserInput { .. }));
    }

    #[test]
    fn test_parse_turn_end() {
        let line = r#"{"type":"system","subtype":"stop_hook_summary","sessionId":"abc-123","timestamp":"2026-03-11T10:01:00Z"}"#;
        let (sid, event) = parse_jsonl_line(line).unwrap();
        assert_eq!(sid, "abc-123");
        assert!(matches!(event, SessionEvent::TurnEnd { .. }));
    }

    #[test]
    fn test_parse_assistant_thinking() {
        let line = r#"{"type":"assistant","sessionId":"abc-123","message":{"role":"assistant","content":[{"type":"thinking","thinking":"analyzing..."}]}}"#;
        let (sid, event) = parse_jsonl_line(line).unwrap();
        assert_eq!(sid, "abc-123");
        assert!(matches!(event, SessionEvent::Thinking { .. }));
    }

    #[test]
    fn test_parse_assistant_tool_use() {
        let line = r#"{"type":"assistant","sessionId":"abc-123","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","id":"toolu_01"}]}}"#;
        let (sid, event) = parse_jsonl_line(line).unwrap();
        assert_eq!(sid, "abc-123");
        if let SessionEvent::ToolUse {
            tool_name, tool_id, ..
        } = event
        {
            assert_eq!(tool_name, "Bash");
            assert_eq!(tool_id, "toolu_01");
        } else {
            panic!("Expected ToolUse");
        }
    }

    #[test]
    fn test_parse_assistant_text() {
        let line = r#"{"type":"assistant","sessionId":"abc-123","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#;
        let (sid, event) = parse_jsonl_line(line).unwrap();
        assert_eq!(sid, "abc-123");
        assert!(matches!(event, SessionEvent::TextOutput { .. }));
    }

    #[test]
    fn test_parse_tool_result() {
        let line = r#"{"type":"user","sessionId":"abc-123","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01","is_error":false}]}}"#;
        let (sid, event) = parse_jsonl_line(line).unwrap();
        assert_eq!(sid, "abc-123");
        if let SessionEvent::ToolResult {
            tool_id, is_error, ..
        } = event
        {
            assert_eq!(tool_id, "toolu_01");
            assert!(!is_error);
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_parse_tool_result_error() {
        let line = r#"{"type":"user","sessionId":"abc-123","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","is_error":true}]}}"#;
        let (_, event) = parse_jsonl_line(line).unwrap();
        if let SessionEvent::ToolResult { is_error, .. } = event {
            assert!(is_error);
        } else {
            panic!("Expected ToolResult");
        }
    }

    #[test]
    fn test_parse_unknown_type() {
        let line = r#"{"type":"unknown","sessionId":"abc-123"}"#;
        assert!(parse_jsonl_line(line).is_none());
    }

    #[test]
    fn test_parse_invalid_json() {
        assert!(parse_jsonl_line("not json").is_none());
    }

    #[test]
    fn test_parse_missing_session_id() {
        let line = r#"{"type":"queue-operation","operation":"enqueue"}"#;
        assert!(parse_jsonl_line(line).is_none());
    }
}
