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

    private func setMode(_ newMode: KeyboardMode, substate newSub: KeyboardSubstate) {
        let changed = newMode != mode || newSub != substate
        mode = newMode
        substate = newSub
        if changed {
            delegate?.keyboardModeDidChange(mode, substate: substate)
            delegate?.keyboardHintDidChange(hintText)
        }
    }

    // Placeholder; replaced with real hints in Task 4.
    var hintText: String { mode == .insert ? "INSERT" : "NORMAL" }
}
