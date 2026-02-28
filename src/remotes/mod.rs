//! remotes - Remote notification channels (Discord, KOOK, etc.)

pub mod channel;
pub mod publisher;
pub mod discord;

pub use channel::{RemoteChannel, RemoteMessage};
pub use publisher::RemoteChannelPublisher;
pub use discord::DiscordChannel;
