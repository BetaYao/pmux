//! Subscribes to Event Bus, filters AgentStateChange + Notification, formats and sends to channels.

use std::sync::Arc;
use std::thread;

use crate::agent_status::AgentStatus;
use crate::remotes::channel::{RemoteChannel, RemoteMessage, RemoteSeverity};
use crate::remotes::discord::DiscordBot;
use crate::remotes::discord_gateway;
use crate::remotes::feishu::FeishuBot;
use crate::remotes::kook::KookBot;
use crate::remotes::kook_gateway;
use crate::remotes::secrets::Secrets;
use crate::runtime::{EventAgentStateChange, Notification, NotificationType, RuntimeEvent};
use flume::Receiver;

pub struct RemoteChannelPublisher {
    channels: Vec<Box<dyn RemoteChannel>>,
}

impl RemoteChannelPublisher {
    pub fn has_channels(&self) -> bool {
        !self.channels.is_empty()
    }

    pub fn from_config(config: &crate::config::Config, secrets: &Secrets) -> Self {
        let mut channels: Vec<Box<dyn RemoteChannel>> = Vec::new();
        if config.remote_channels.discord.enabled {
            if let (Some(ref token), Some(ref ch)) = (
                secrets.remote_channels.discord.bot_token.as_ref(),
                config.remote_channels.discord.channel_id.as_ref(),
            ) {
                if !token.is_empty() && !ch.is_empty() {
                    if let Ok(bot) = DiscordBot::new((*token).clone(), (*ch).clone()) {
                        channels.push(Box::new(bot));
                    }
                }
            }
        }
        if config.remote_channels.kook.enabled {
            if let (Some(ref token), Some(ref ch)) = (
                secrets.remote_channels.kook.bot_token.as_ref(),
                config.remote_channels.kook.channel_id.as_ref(),
            ) {
                if !token.is_empty() && !ch.is_empty() {
                    if let Ok(bot) = KookBot::new((*token).clone(), (*ch).clone()) {
                        channels.push(Box::new(bot));
                    }
                }
            }
        }
        if config.remote_channels.feishu.enabled {
            if let (Some(ref app_id), Some(ref app_secret), Some(ref chat_id)) = (
                secrets.remote_channels.feishu.app_id.as_ref(),
                secrets.remote_channels.feishu.app_secret.as_ref(),
                config.remote_channels.feishu.chat_id.as_ref(),
            ) {
                if !app_id.is_empty() && !app_secret.is_empty() && !chat_id.is_empty() {
                    if let Ok(bot) = FeishuBot::new(
                        (*app_id).clone(),
                        (*app_secret).clone(),
                        (*chat_id).clone(),
                    ) {
                        channels.push(Box::new(bot));
                    }
                }
            }
        }
        Self { channels }
    }

    pub fn run(self, rx: Receiver<RuntimeEvent>) {
        let channels = self.channels;
        thread::spawn(move || {
            for ev in rx {
                if channels.is_empty() {
                    continue;
                }
                let msg = match &ev {
                    RuntimeEvent::AgentStateChange(a) => Self::state_to_message(a),
                    RuntimeEvent::Notification(n) => Self::notification_to_message(n),
                    _ => continue,
                };
                for ch in &channels {
                    if let Err(e) = ch.send(&msg) {
                        eprintln!("remotes: {} send failed: {}", ch.name(), e);
                    }
                }
            }
        });
    }

    fn state_to_message(a: &EventAgentStateChange) -> RemoteMessage {
        let (workspace, worktree) = Self::parse_agent_id(&a.agent_id);
        let title = format!("Agent: {}", a.state.display_text());
        RemoteMessage {
            workspace,
            worktree,
            severity: Self::status_to_severity(&a.state),
            title,
            body: a.state.display_text().to_string(),
        }
    }

    fn notification_to_message(n: &Notification) -> RemoteMessage {
        let (workspace, worktree) = Self::parse_agent_id(&n.agent_id);
        let severity = match n.notif_type {
            NotificationType::Error => RemoteSeverity::Error,
            NotificationType::WaitingInput => RemoteSeverity::Warning,
            NotificationType::Info => RemoteSeverity::Info,
        };
        RemoteMessage {
            workspace,
            worktree,
            severity,
            title: format!("{:?}", n.notif_type),
            body: n.message.clone(),
        }
    }

    fn parse_agent_id(agent_id: &str) -> (String, String) {
        // agent_id can be "local:/path/to/worktree" or "session:window"
        if let Some(rest) = agent_id.strip_prefix("local:") {
            let path = std::path::Path::new(rest);
            let worktree = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| rest.to_string());
            let workspace = path
                .parent()
                .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_default();
            (workspace, worktree)
        } else {
            (agent_id.to_string(), agent_id.to_string())
        }
    }

    fn status_to_severity(s: &AgentStatus) -> RemoteSeverity {
        match s {
            AgentStatus::Error => RemoteSeverity::Error,
            AgentStatus::Waiting => RemoteSeverity::Warning,
            _ => RemoteSeverity::Info,
        }
    }
}

/// Spawn Gateway listeners for Discord and KOOK (command receiving).
pub fn spawn_remote_gateways(config: &crate::config::Config, secrets: &Secrets) {
    if config.remote_channels.discord.enabled {
        if let (Some(ref token), Some(ref ch)) = (
            secrets.remote_channels.discord.bot_token.as_ref(),
            config.remote_channels.discord.channel_id.as_ref(),
        ) {
            if !token.is_empty() && !ch.is_empty() {
                if let Ok(bot) = DiscordBot::new((*token).clone(), (*ch).clone()) {
                    discord_gateway::spawn_gateway(Arc::new(bot), (*ch).clone());
                }
            }
        }
    }
    if config.remote_channels.kook.enabled {
        if let (Some(ref token), Some(ref ch)) = (
            secrets.remote_channels.kook.bot_token.as_ref(),
            config.remote_channels.kook.channel_id.as_ref(),
        ) {
            if !token.is_empty() && !ch.is_empty() {
                if let Ok(bot) = KookBot::new((*token).clone(), (*ch).clone()) {
                    kook_gateway::spawn_gateway(Arc::new(bot), (*ch).clone());
                }
            }
        }
    }
}
