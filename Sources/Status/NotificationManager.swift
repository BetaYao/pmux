import AppKit
import UserNotifications

/// Sends macOS system notifications when agent status changes to actionable states.
class NotificationManager: NSObject {
    static let shared = NotificationManager()

    private var lastNotified: [String: Date] = [:]
    private let cooldown: TimeInterval = 30  // Don't spam same worktree within 30s

    private override init() {
        super.init()
        requestPermission()
    }

    private func requestPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { granted, error in
            if let error {
                NSLog("Notification permission error: \(error)")
            }
        }
        UNUserNotificationCenter.current().delegate = self
    }

    /// Notify when agent transitions to a notable state
    func notify(worktreePath: String, branch: String, oldStatus: AgentStatus, newStatus: AgentStatus) {
        // Only notify for transitions TO these states
        guard newStatus == .waiting || newStatus == .error || newStatus == .idle else { return }

        // Only notify if it was previously running (agent finished something)
        guard oldStatus == .running else { return }

        // Cooldown per worktree
        if let last = lastNotified[worktreePath], Date().timeIntervalSince(last) < cooldown {
            return
        }
        lastNotified[worktreePath] = Date()

        // Don't notify if app is frontmost
        if NSApp.isActive { return }

        let content = UNMutableNotificationContent()

        switch newStatus {
        case .waiting:
            content.title = "Agent needs input"
            content.body = "\(branch) is waiting for your response"
            content.sound = .default
        case .error:
            content.title = "Agent error"
            content.body = "\(branch) encountered an error"
            content.sound = .defaultCritical
        case .idle:
            content.title = "Agent finished"
            content.body = "\(branch) completed its task"
            content.sound = .default
        default:
            return
        }

        content.userInfo = ["worktreePath": worktreePath]

        let request = UNNotificationRequest(
            identifier: "pmux-\(worktreePath.hashValue)",
            content: content,
            trigger: nil  // Deliver immediately
        )

        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                NSLog("Failed to send notification: \(error)")
            }
        }
    }
}

// MARK: - UNUserNotificationCenterDelegate

extension NotificationManager: UNUserNotificationCenterDelegate {
    /// Show notification even when app is in foreground (for secondary monitors)
    func userNotificationCenter(_ center: UNUserNotificationCenter,
                                willPresent notification: UNNotification,
                                withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        completionHandler([.banner, .sound])
    }

    /// Handle notification click — bring app to front
    func userNotificationCenter(_ center: UNUserNotificationCenter,
                                didReceive response: UNNotificationResponse,
                                withCompletionHandler completionHandler: @escaping () -> Void) {
        NSApp.activate(ignoringOtherApps: true)
        completionHandler()
    }
}
