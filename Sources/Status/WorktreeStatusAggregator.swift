import Foundation

protocol WorktreeStatusDelegate: AnyObject {
    func worktreeStatusDidUpdate(_ status: WorktreeStatus)
    func paneStatusDidChange(worktreePath: String, paneIndex: Int,
                             oldStatus: AgentStatus, newStatus: AgentStatus,
                             lastMessage: String)
}

/// Thread safety: All methods must be called on the main queue.
/// StatusPublisher dispatches to main before calling agentDidUpdate.
class WorktreeStatusAggregator {
    weak var delegate: WorktreeStatusDelegate?

    private var worktreeStatuses: [String: WorktreeStatus] = [:]
    private var paneStates: [String: PaneStatus] = [:]
    private var terminalToWorktree: [String: String] = [:]
    private var worktreeTerminals: [String: [String]] = [:]

    func registerTerminal(_ terminalID: String, worktreePath: String, leafIndex: Int) {
        terminalToWorktree[terminalID] = worktreePath
        var ids = worktreeTerminals[worktreePath] ?? []
        if !ids.contains(terminalID) {
            if leafIndex < ids.count {
                ids.insert(terminalID, at: leafIndex)
            } else {
                ids.append(terminalID)
            }
        }
        worktreeTerminals[worktreePath] = ids
    }

    func unregisterTerminal(_ terminalID: String, worktreePath: String) {
        terminalToWorktree.removeValue(forKey: terminalID)
        worktreeTerminals[worktreePath]?.removeAll { $0 == terminalID }
        paneStates.removeValue(forKey: terminalID)
        if worktreeTerminals[worktreePath]?.isEmpty == true {
            worktreeTerminals.removeValue(forKey: worktreePath)
            worktreeStatuses.removeValue(forKey: worktreePath)
        }
    }

    func updateLeafOrder(worktreePath: String, terminalIDs: [String]) {
        worktreeTerminals[worktreePath] = terminalIDs
        rebuildWorktreeStatus(worktreePath: worktreePath)
    }

    func agentDidUpdate(terminalID: String, status: AgentStatus, lastMessage: String) {
        guard let worktreePath = terminalToWorktree[terminalID] else { return }

        let now = Date()
        let oldPaneState = paneStates[terminalID]
        let statusChanged = oldPaneState?.status != status
        let messageChanged = oldPaneState?.lastMessage != lastMessage

        guard statusChanged || messageChanged else { return }

        let paneIndex = paneIndexForTerminal(terminalID, worktreePath: worktreePath)
        let newPaneState = PaneStatus(
            paneIndex: paneIndex,
            terminalID: terminalID,
            status: status,
            lastMessage: lastMessage,
            lastUpdated: now
        )
        paneStates[terminalID] = newPaneState

        if statusChanged, let oldStatus = oldPaneState?.status {
            delegate?.paneStatusDidChange(
                worktreePath: worktreePath,
                paneIndex: paneIndex,
                oldStatus: oldStatus,
                newStatus: status,
                lastMessage: lastMessage
            )
        }

        rebuildWorktreeStatus(worktreePath: worktreePath)
    }

    func status(for worktreePath: String) -> WorktreeStatus? {
        worktreeStatuses[worktreePath]
    }

    private func paneIndexForTerminal(_ terminalID: String, worktreePath: String) -> Int {
        let ids = worktreeTerminals[worktreePath] ?? []
        let index = ids.firstIndex(of: terminalID) ?? 0
        return index + 1
    }

    private func rebuildWorktreeStatus(worktreePath: String) {
        guard let terminalIDs = worktreeTerminals[worktreePath], !terminalIDs.isEmpty else { return }

        var panes: [PaneStatus] = []
        for (index, tid) in terminalIDs.enumerated() {
            if var pane = paneStates[tid] {
                pane = PaneStatus(
                    paneIndex: index + 1,
                    terminalID: pane.terminalID,
                    status: pane.status,
                    lastMessage: pane.lastMessage,
                    lastUpdated: pane.lastUpdated
                )
                paneStates[tid] = pane
                panes.append(pane)
            }
        }

        guard !panes.isEmpty else { return }

        let mostRecent = panes.max(by: { $0.lastUpdated < $1.lastUpdated }) ?? panes[0]

        let ws = WorktreeStatus(
            worktreePath: worktreePath,
            panes: panes,
            mostRecentPaneIndex: mostRecent.paneIndex,
            mostRecentMessage: mostRecent.lastMessage
        )
        worktreeStatuses[worktreePath] = ws
        delegate?.worktreeStatusDidUpdate(ws)
    }
}
