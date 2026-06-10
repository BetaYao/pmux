import Foundation

protocol KeyboardModeDelegate: AnyObject {
    func keyboardModeDidChange(_ mode: KeyboardMode, substate: KeyboardSubstate)
    func keyboardHintDidChange(_ hint: String)
}

final class KeyboardModeController {
    weak var delegate: KeyboardModeDelegate?

    private(set) var mode: KeyboardMode = .normal
    private(set) var substate: KeyboardSubstate = .none

    func enterInsert() {
        setMode(.insert, substate: .none)
    }

    func enterNormal() {
        setMode(.normal, substate: .none)
    }

    private var lastEscTime: TimeInterval = -1
    static let doubleEscWindow: TimeInterval = 0.4

    /// Returns true if the controller consumed the Esc (caller must NOT pass it on).
    /// `now` is a monotonic timestamp in seconds (injected for tests; production uses
    /// ProcessInfo.processInfo.systemUptime).
    @discardableResult
    func handleEsc(hasCommand: Bool, now: TimeInterval) -> Bool {
        guard mode == .insert else { return false }
        if hasCommand {
            enterNormal()
            lastEscTime = -1
            return true
        }
        if lastEscTime >= 0 && (now - lastEscTime) <= Self.doubleEscWindow {
            enterNormal()
            lastEscTime = -1
            return true
        }
        lastEscTime = now
        return false   // first Esc passes through to the terminal
    }

    func beginDelete(agentId: String) {
        setMode(.normal, substate: .deletePending(agentId: agentId))
    }

    @discardableResult
    func confirmDelete() -> String? {
        guard case .deletePending(let agentId) = substate else { return nil }
        setMode(.normal, substate: .none)
        return agentId
    }

    func cancelDelete() {
        guard case .deletePending = substate else { return }
        setMode(.normal, substate: .none)
    }

    func beginCreateForm() { setMode(.normal, substate: .createForm) }
    func endCreateForm() {
        guard case .createForm = substate else { return }
        setMode(.normal, substate: .none)
    }

    private func setMode(_ newMode: KeyboardMode, substate newSub: KeyboardSubstate) {
        let changed = newMode != mode || newSub != substate
        mode = newMode
        substate = newSub
        if changed {
            delegate?.keyboardModeDidChange(mode, substate: substate)
            delegate?.keyboardHintDidChange(hintText)
        }
    }

    var hintText: String {
        switch substate {
        case .deletePending:
            return "DELETE?  ·  d / y confirm  ·  esc cancel"
        case .createForm:
            return "CREATE  ·  tab field  ·  \u{2190}\u{2192} change  ·  \u{2318}\u{23CE} create  ·  esc cancel"
        case .none:
            switch mode {
            case .insert:
                return "INSERT  ·  \u{2318}esc / esc\u{00B7}esc \u{2192} normal"
            case .normal:
                return "NORMAL  ·  hjkl move  ·  \u{23CE} enter  ·  d del  ·  c diff  ·  f files  ·  n new"
            }
        }
    }
}
