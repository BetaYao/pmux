import AppKit

protocol PanelCoordinatorDelegate: AnyObject {
    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String)
}

class PanelCoordinator: NSObject {
    weak var delegate: PanelCoordinatorDelegate?
    weak var titleBar: TitleBarView?

    let notificationPanel = NotificationPanelView()
    let aiPanel = AIPanelView()
    let notificationPopover = NSPopover()
    let aiPopover = NSPopover()

    func setupPopovers() {
        notificationPanel.delegate = self
        notificationPanel.frame = NSRect(x: 0, y: 0, width: 360, height: 460)
        notificationPopover.contentSize = notificationPanel.frame.size
        notificationPopover.behavior = .transient
        notificationPopover.animates = true
        notificationPopover.delegate = self
        notificationPopover.contentViewController = ViewHostController(hostedView: notificationPanel)

        aiPanel.delegate = self
        aiPanel.frame = NSRect(x: 0, y: 0, width: 360, height: 460)
        aiPopover.contentSize = aiPanel.frame.size
        aiPopover.behavior = .transient
        aiPopover.animates = true
        aiPopover.delegate = self
        aiPopover.contentViewController = ViewHostController(hostedView: aiPanel)
    }

    func closeBothPanels() {
        notificationPopover.performClose(nil)
        aiPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)
        aiPanel.setOpen(false, animated: false)
    }

    func toggleNotificationPanel() {
        if notificationPopover.isShown {
            notificationPopover.performClose(nil)
            notificationPanel.setOpen(false, animated: false)
            return
        }

        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)

        notificationPanel.updateNotifications(NotificationHistory.shared.entries.map {
            (
                title: "\($0.branch)  \($0.status.rawValue)",
                meta: $0.message,
                worktreePath: $0.worktreePath
            )
        })
        notificationPanel.setOpen(true, animated: false)

        guard let titleBar else { return }
        let anchor = titleBar.notificationsAnchorView()
        notificationPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }

    func toggleAIPanel() {
        if aiPopover.isShown {
            aiPopover.performClose(nil)
            aiPanel.setOpen(false, animated: false)
            return
        }

        notificationPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)

        aiPanel.setOpen(true, animated: false)
        guard let titleBar else { return }
        let anchor = titleBar.aiAnchorView()
        aiPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }
}

// MARK: - NotificationPanelDelegate

extension PanelCoordinator: NotificationPanelDelegate {
    func notificationPanelDidRequestClose() {
        notificationPopover.performClose(nil)
        notificationPanel.setOpen(false, animated: false)
    }

    func notificationPanelDidSelectItem(worktreePath: String) {
        closeBothPanels()
        NotificationCenter.default.post(
            name: .navigateToWorktree,
            object: nil,
            userInfo: ["worktreePath": worktreePath]
        )
    }
}

// MARK: - AIPanelDelegate

extension PanelCoordinator: AIPanelDelegate {
    func aiPanelDidRequestClose() {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
    }
}

// MARK: - NotificationHistoryDelegate

extension PanelCoordinator: NotificationHistoryDelegate {
    func notificationHistory(_ vc: NotificationHistoryViewController, didSelectWorktreePath path: String) {
        NotificationCenter.default.post(
            name: .navigateToWorktree,
            object: nil,
            userInfo: ["worktreePath": path]
        )
    }
}

// MARK: - NSPopoverDelegate

extension PanelCoordinator: NSPopoverDelegate {
    func popoverDidClose(_ notification: Notification) {
        guard let popover = notification.object as? NSPopover else { return }
        if popover === notificationPopover {
            notificationPanel.setOpen(false, animated: false)
        } else if popover === aiPopover {
            aiPanel.setOpen(false, animated: false)
        }
    }
}
