import AppKit

/// Encapsulates the "Dashboard Navigation" (D-state) focus ring.
///
/// Pure value logic — no AppKit view references. Consumed by `DashboardViewController`
/// which translates `focusedTarget` into first-responder and visual updates.
final class DashboardFocusController {

    enum Target: Equatable {
        case none
        case bigPanel          // only meaningful in focus layouts
        case card(String)      // worktree path (agent id)
    }

    enum Mode {
        case idle              // not in D state
        case grid              // grid layout: ring = cards only
        case focusLayout       // leftRight/topSmall/topLarge: ring = [bigPanel, cards...]
    }

    private(set) var mode: Mode = .idle
    private(set) var focusedTarget: Target = .none
    private(set) var cardIds: [String] = []

    /// Snapshot of state before entering D, used by Esc to restore.
    struct Snapshot {
        let firstResponder: NSResponder?
        let focusedWorktreePath: String?
        let layout: DashboardLayout
    }
    private(set) var snapshot: Snapshot?

    // MARK: - Entry

    func enterGrid(cardIds: [String], initialId: String?) {
        mode = .grid
        self.cardIds = cardIds
        if let initial = initialId, cardIds.contains(initial) {
            focusedTarget = .card(initial)
        } else if let first = cardIds.first {
            focusedTarget = .card(first)
        } else {
            focusedTarget = .none
        }
    }

    func enterFocusLayout(cardIds: [String]) {
        mode = .focusLayout
        self.cardIds = cardIds
        focusedTarget = .bigPanel
    }

    func exit() {
        mode = .idle
        focusedTarget = .none
        cardIds = []
        snapshot = nil
    }

    func captureSnapshot(_ snapshot: Snapshot) {
        self.snapshot = snapshot
    }

    // MARK: - Navigation

    func next() {
        switch mode {
        case .idle:
            return
        case .grid:
            guard !cardIds.isEmpty else { focusedTarget = .none; return }
            if case .card(let id) = focusedTarget, let idx = cardIds.firstIndex(of: id) {
                focusedTarget = .card(cardIds[(idx + 1) % cardIds.count])
            } else {
                focusedTarget = .card(cardIds[0])
            }
        case .focusLayout:
            // ring: [bigPanel, card0, card1, ...]
            switch focusedTarget {
            case .bigPanel:
                focusedTarget = cardIds.first.map { .card($0) } ?? .bigPanel
            case .card(let id):
                if let idx = cardIds.firstIndex(of: id) {
                    if idx + 1 < cardIds.count {
                        focusedTarget = .card(cardIds[idx + 1])
                    } else {
                        focusedTarget = .bigPanel
                    }
                } else {
                    focusedTarget = .bigPanel
                }
            case .none:
                focusedTarget = .bigPanel
            }
        }
    }

    func prev() {
        switch mode {
        case .idle:
            return
        case .grid:
            guard !cardIds.isEmpty else { focusedTarget = .none; return }
            if case .card(let id) = focusedTarget, let idx = cardIds.firstIndex(of: id) {
                let prevIdx = (idx - 1 + cardIds.count) % cardIds.count
                focusedTarget = .card(cardIds[prevIdx])
            } else {
                focusedTarget = .card(cardIds[cardIds.count - 1])
            }
        case .focusLayout:
            switch focusedTarget {
            case .bigPanel:
                focusedTarget = cardIds.last.map { .card($0) } ?? .bigPanel
            case .card(let id):
                if let idx = cardIds.firstIndex(of: id) {
                    if idx == 0 {
                        focusedTarget = .bigPanel
                    } else {
                        focusedTarget = .card(cardIds[idx - 1])
                    }
                } else {
                    focusedTarget = .bigPanel
                }
            case .none:
                focusedTarget = .bigPanel
            }
        }
    }

    func jump(toIndex index: Int) {
        guard mode != .idle, cardIds.indices.contains(index) else { return }
        focusedTarget = .card(cardIds[index])
    }

    /// Grid-aware directional move. `columns` is the number of cards per row in the
    /// current grid layout (callers pass 1 for focus layouts → up/down behave as prev/next,
    /// left/right are no-ops).
    func move(_ direction: FocusDirection, columns: Int) {
        guard mode != .idle else { return }
        guard case .card(let id) = focusedTarget, let idx = cardIds.firstIndex(of: id) else {
            // No card focused yet: any move selects the first card.
            if let first = cardIds.first { focusedTarget = .card(first) }
            return
        }
        let cols = max(1, columns)
        let col = idx % cols
        var target = idx
        switch direction {
        case .left:  if col > 0 { target = idx - 1 }
        case .right: if col < cols - 1 && idx + 1 < cardIds.count { target = idx + 1 }
        case .up:    if idx - cols >= 0 { target = idx - cols }
        case .down:  if idx + cols < cardIds.count { target = idx + cols }
        }
        focusedTarget = .card(cardIds[target])
    }

    // MARK: - Mutation

    /// Remove the currently focused card from the ring and advance focus.
    /// No-op if the focused target is not a card.
    func removeCurrentCard() {
        guard case .card(let id) = focusedTarget,
              let idx = cardIds.firstIndex(of: id) else { return }
        cardIds.remove(at: idx)
        if cardIds.isEmpty {
            focusedTarget = (mode == .focusLayout) ? .bigPanel : .none
            return
        }
        let nextIdx = idx % cardIds.count
        focusedTarget = .card(cardIds[nextIdx])
    }

    /// Replace the card list while preserving focus if possible.
    /// Called when the underlying agent list changes while D is active.
    func refreshCards(_ ids: [String]) {
        cardIds = ids
        if case .card(let id) = focusedTarget, !ids.contains(id) {
            focusedTarget = (mode == .focusLayout) ? .bigPanel : (ids.first.map { .card($0) } ?? .none)
        }
    }
}
