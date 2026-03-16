//! hooks/ - AI tool hook detection, installation, and webhook server

pub mod detector;
pub mod installer;
pub mod server;
pub mod handler;
pub mod setup_check;

pub static WEBHOOK_PORT: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(7070);
