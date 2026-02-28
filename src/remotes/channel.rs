use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct RemoteMessage {
    pub workspace: String,
    pub worktree: String,
    pub severity: RemoteSeverity,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteSeverity {
    Info,
    Warning,
    Error,
}

pub trait RemoteChannel: Send + Sync {
    fn name(&self) -> &str;
    fn send(&self, msg: &RemoteMessage) -> Result<(), RemoteSendError>;
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteSendError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error: {0}")]
    Api(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_message_serialize() {
        let msg = RemoteMessage {
            workspace: "repo-a".to_string(),
            worktree: "feat-x".to_string(),
            severity: RemoteSeverity::Error,
            title: "Agent Error".to_string(),
            body: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("repo-a"));
        assert!(json.contains("Error"));
    }
}
