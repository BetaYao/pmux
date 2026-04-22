import AppKit

protocol PanelCoordinatorDelegate: AnyObject {
    func panelCoordinator(_ coordinator: PanelCoordinator, navigateToWorktreePath path: String, paneIndex: Int?)
}

class PanelCoordinator: NSObject {
    weak var delegate: PanelCoordinatorDelegate?
    weak var titleBar: TitleBarView?

    let aiPanel = AIPanelView()
    let aiPopover = NSPopover()

    func setupPopovers() {
        aiPanel.delegate = self
        aiPanel.frame = NSRect(x: 0, y: 0, width: 440, height: 460)
        aiPopover.contentSize = aiPanel.frame.size
        aiPopover.behavior = .transient
        aiPopover.animates = true
        aiPopover.delegate = self
        aiPopover.contentViewController = ViewHostController(hostedView: aiPanel)
    }

    func closeBothPanels() {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
    }

    func toggleAIPanel() {
        if aiPopover.isShown {
            aiPopover.performClose(nil)
            aiPanel.setOpen(false, animated: false)
            return
        }

        // Feed real data from stores
        refreshAIPanelData()

        aiPanel.setOpen(true, animated: false)
        guard let titleBar else { return }
        let anchor = titleBar.aiAnchorView()
        aiPopover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .maxY)
    }

    private func refreshAIPanelData() {
        let formatter = DateFormatter()
        formatter.dateFormat = "HH:mm"

        let todoDisplayItems = TodoStore.shared.allItems().map { item in
            AIPanelView.TodoDisplayItem(
                id: item.id.hashValue,
                task: item.task,
                status: item.status,
                issue: item.issue,
                worktree: item.branch,
                progress: item.progress
            )
        }

        let ideaDisplayItems = IdeaStore.shared.allItems().map { item in
            AIPanelView.IdeaDisplayItem(
                id: item.id,
                timestamp: formatter.string(from: item.createdAt),
                text: item.text,
                source: item.source,
                tags: item.tags
            )
        }

        aiPanel.updateTodoItems(todoDisplayItems)
        aiPanel.updateIdeaItems(ideaDisplayItems)
    }

    func notificationPanelDidSelectItem(_ entry: NotificationEntry) {
        closeBothPanels()
        delegate?.panelCoordinator(self, navigateToWorktreePath: entry.worktreePath, paneIndex: entry.paneIndex)
    }
}

// MARK: - AIPanelDelegate

extension PanelCoordinator: AIPanelDelegate {
    func aiPanelDidRequestClose() {
        aiPopover.performClose(nil)
        aiPanel.setOpen(false, animated: false)
    }

    func aiPanelDidSubmitIdea(_ text: String) {
        IdeaStore.shared.add(text: text, project: "amux", source: "manual", tags: [])
        refreshAIPanelData()
    }

    func aiPanelDidRequestDeleteIdea(id: String) {
        IdeaStore.shared.remove(id: id)
        refreshAIPanelData()
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
        if popover === aiPopover {
            aiPanel.setOpen(false, animated: false)
        }
    }
}
