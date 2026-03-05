// system_notifier.rs - Desktop notification via notify-rust
use crate::notification::NotificationType;
use notify_rust::Notification;

/// Sends a desktop system notification.
/// On macOS, the user may need to grant notification permission on first use.
pub fn notify(_app: &str, body: &str, notif_type: NotificationType) {
    let summary = match notif_type {
        NotificationType::Error => "pmux — Error",
        NotificationType::Waiting => "pmux — Waiting",
        NotificationType::WaitingConfirm => "pmux — Confirm",
        NotificationType::Info => "pmux — Done",
    };
    let _ = Notification::new()
        .summary(summary)
        .body(body)
        .appname("pmux")
        .timeout(5000)
        .show();
}
