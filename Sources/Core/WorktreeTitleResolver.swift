import Foundation

/// Resolves the human-facing title for a worktree, shared by the top capsule and
/// the mini cards. Order: Claude session summary → last user prompt → branch.
enum WorktreeTitleResolver {
    static func resolve(
        worktreePath: String,
        lastUserPrompt: String,
        branch: String,
        sessionTitle: (String) -> String? = { SessionTitleLookup.title(worktreePath: $0) }
    ) -> String {
        if let summary = sessionTitle(worktreePath)?.trimmingCharacters(in: .whitespacesAndNewlines),
           !summary.isEmpty {
            return summary
        }
        let prompt = lastUserPrompt.trimmingCharacters(in: .whitespacesAndNewlines)
        if !prompt.isEmpty { return prompt }
        return branch
    }
}
