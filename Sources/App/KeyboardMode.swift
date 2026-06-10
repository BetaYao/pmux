import Foundation

enum KeyboardMode: Equatable {
    case normal
    case insert
}

/// Transient state within Normal mode.
enum KeyboardSubstate: Equatable {
    case none
    case deletePending(agentId: String)   // first `d` pressed, awaiting confirm
    case createForm                        // inline worktree creator focused
}
