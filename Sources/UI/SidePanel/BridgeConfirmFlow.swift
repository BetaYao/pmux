import Foundation

enum ConfirmDecision: Equatable { case expand, execute }

/// Graded confirm flow: irreversible actions (returnToPort) require expand then execute;
/// all other kinds execute immediately.
enum BridgeConfirmFlow {
    static func onEnter(kind: FirstMateActionKind, expanded: Bool) -> ConfirmDecision {
        switch kind {
        case .returnToPort:
            return expanded ? .execute : .expand
        default:
            return .execute
        }
    }
}
