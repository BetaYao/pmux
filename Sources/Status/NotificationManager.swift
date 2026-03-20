import AppKit
import UserNotifications

extension Notification.Name {
    static let navigateToWorktree = Notification.Name("pmux.navigateToWorktree")
}

/// Sends macOS system notifications when agent status changes to actionable states.
class NotificationManager: NSObject {
    static let shared = NotificationManager()

    private var lastNotified: [String: Date] = [:]
    private let cooldown: TimeInterval = 30  // Don't spam same worktree within 30s

    private override init() {
        super.init()
        requestPermission()
    }

    private static let categoryIdentifier = "pmux.agentStatus"
    private static let openTerminalAction = "open_terminal"

    private func requestPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { granted, error in
            if let error {
                NSLog("Notification permission error: \(error)")
            }
        }
        UNUserNotificationCenter.current().delegate = self

        // Register notification category with action buttons
        let openAction = UNNotificationAction(
            identifier: Self.openTerminalAction,
            title: "Open Terminal",
            options: [.foreground]
        )
        let category = UNNotificationCategory(
            identifier: Self.categoryIdentifier,
            actions: [openAction],
            intentIdentifiers: []
        )
        UNUserNotificationCenter.current().setNotificationCategories([category])
    }

    /// Notify when agent transitions to a notable state
    func notify(worktreePath: String, branch: String, oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String = "") {
        // Only notify for transitions TO these states
        guard newStatus == .waiting || newStatus == .error || newStatus == .idle else { return }

        // Only notify if it was previously running (agent finished something)
        guard oldStatus == .running else { return }

        // Cooldown per worktree
        if let last = lastNotified[worktreePath], Date().timeIntervalSince(last) < cooldown {
            return
        }
        lastNotified[worktreePath] = Date()

        // Always add to in-app history
        let historyMessage: String
        let content = UNMutableNotificationContent()

        switch newStatus {
        case .waiting:
            content.title = "Agent needs input — \(branch)"
            let fallback = "\(branch) is waiting for your response"
            content.body = lastMessage.isEmpty ? fallback : lastMessage
            content.sound = .default
            historyMessage = content.body
        case .error:
            content.title = "Agent error — \(branch)"
            let fallback = "\(branch) encountered an error"
            content.body = lastMessage.isEmpty ? fallback : lastMessage
            content.sound = .defaultCritical
            historyMessage = content.body
        case .idle:
            content.title = "Agent finished — \(branch)"
            let fallback = "\(branch) completed its task"
            content.body = lastMessage.isEmpty ? fallback : lastMessage
            content.sound = .default
            historyMessage = content.body
        default:
            return
        }

        // Add to in-app history
        let entry = NotificationEntry(
            branch: branch,
            worktreePath: worktreePath,
            status: newStatus,
            message: historyMessage
        )
        NotificationHistory.shared.add(entry)

        // Don't send system notification if app is frontmost
        if NSApp.isActive { return }

        content.userInfo = ["worktreePath": worktreePath]
        content.categoryIdentifier = Self.categoryIdentifier

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

    /// Handle notification click or action button
    func userNotificationCenter(_ center: UNUserNotificationCenter,
                                didReceive response: UNNotificationResponse,
                                withCompletionHandler completionHandler: @escaping () -> Void) {
        let userInfo = response.notification.request.content.userInfo

        // Both default tap and "Open Terminal" action navigate to worktree
        let shouldNavigate = response.actionIdentifier == UNNotificationDefaultActionIdentifier
            || response.actionIdentifier == Self.openTerminalAction

        if shouldNavigate {
            DispatchQueue.main.async {
                NSApp.activate(ignoringOtherApps: true)
                NSApp.mainWindow?.deminiaturize(nil)

                if let path = userInfo["worktreePath"] as? String {
                    NotificationCenter.default.post(
                        name: .navigateToWorktree,
                        object: nil,
                        userInfo: ["worktreePath": path]
                    )
                }
            }
        }

        completionHandler()
    }
}
