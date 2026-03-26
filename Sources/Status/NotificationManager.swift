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

    func shouldNotify(terminalID: String, oldStatus: AgentStatus, newStatus: AgentStatus) -> Bool {
        guard oldStatus == .running else { return false }
        guard newStatus == .waiting || newStatus == .error || newStatus == .idle else { return false }
        if let last = lastNotified[terminalID], Date().timeIntervalSince(last) < cooldown {
            return false
        }
        lastNotified[terminalID] = Date()
        return true
    }

    static func formatTitle(status: AgentStatus, branch: String, paneIndex: Int, paneCount: Int) -> String {
        let base: String
        switch status {
        case .idle: base = "Agent finished — \(branch)"
        case .waiting: base = "Agent needs input — \(branch)"
        case .error: base = "Agent error — \(branch)"
        default: base = "Agent status — \(branch)"
        }
        if paneCount > 1 {
            return "\(base) [Pane \(paneIndex)]"
        }
        return base
    }

    /// Per-pane notification with terminalID-based cooldown.
    /// `isFocusedPane`: true when this pane is the currently focused pane — suppresses system notification.
    func notify(terminalID: String, worktreePath: String, branch: String,
                paneIndex: Int, paneCount: Int,
                oldStatus: AgentStatus, newStatus: AgentStatus, lastMessage: String,
                isFocusedPane: Bool) {
        guard shouldNotify(terminalID: terminalID, oldStatus: oldStatus, newStatus: newStatus) else { return }

        let title = Self.formatTitle(status: newStatus, branch: branch, paneIndex: paneIndex, paneCount: paneCount)
        let content = UNMutableNotificationContent()
        content.title = title

        let fallback: String
        switch newStatus {
        case .waiting: fallback = "\(branch) is waiting for your response"
        case .error: fallback = "\(branch) encountered an error"
        case .idle: fallback = "\(branch) completed its task"
        default: fallback = ""
        }
        content.body = lastMessage.isEmpty ? fallback : lastMessage
        content.sound = newStatus == .error ? .defaultCritical : .default

        let historyPaneIndex: Int? = paneCount > 1 ? paneIndex : nil
        let entry = NotificationEntry(
            branch: branch,
            worktreePath: worktreePath,
            status: newStatus,
            message: content.body,
            paneIndex: historyPaneIndex
        )
        NotificationHistory.shared.add(entry)

        // Only suppress system notification for the currently focused pane
        if isFocusedPane { return }

        content.userInfo = ["worktreePath": worktreePath, "paneIndex": paneIndex]
        content.categoryIdentifier = Self.categoryIdentifier

        let request = UNNotificationRequest(
            identifier: "pmux-\(worktreePath.hashValue)-\(paneIndex)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request) { error in
            if let error { NSLog("Failed to send notification: \(error)") }
        }
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

        if shouldNavigate, let path = userInfo["worktreePath"] as? String {
            let paneIndex = userInfo["paneIndex"] as? Int
            DispatchQueue.main.async {
                guard let appDelegate = NSApp.delegate as? AppDelegate,
                      let mwc = appDelegate.mainWindowController else { return }

                // Bring existing window to front without creating a new one
                mwc.window?.deminiaturize(nil)
                mwc.window?.makeKeyAndOrderFront(nil)
                NSApp.activate(ignoringOtherApps: true)

                // Navigate directly — no NotificationCenter broadcast
                mwc.tabCoordinator.handleNavigateToWorktree(worktreePath: path, paneIndex: paneIndex)
            }
        }

        completionHandler()
    }
}
