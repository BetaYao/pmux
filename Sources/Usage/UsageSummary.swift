import Foundation

enum UsageProvider: String, Codable, Equatable {
    case claude
    case codex

    var displayName: String {
        switch self {
        case .claude: return "Claude"
        case .codex: return "Codex"
        }
    }
}

struct UsageRateLimitWindow: Codable, Equatable {
    let usedPercent: Int
    let resetsAt: Date?

    var remainingPercent: Int {
        max(0, min(100, 100 - usedPercent))
    }

    var progress: Double {
        Double(max(0, min(100, usedPercent))) / 100.0
    }
}

struct UsageSnapshot: Equatable {
    let provider: UsageProvider
    let rateLimit: UsageRateLimitWindow?
    let todayTokens: Int?
    let updatedAt: Date?
    let isStale: Bool
}

enum PrimaryCapsuleFrameKind: Equatable {
    case shortcut
    case usage
}

struct PrimaryCapsuleFrame: Equatable {
    let kind: PrimaryCapsuleFrameKind
    let iconName: String
    let leadingText: String
    let bodyText: String
    let trailingText: String
    let usageProgress: Double?
    let resetText: String?

    static func shortcut(leading: String, body: String) -> PrimaryCapsuleFrame {
        PrimaryCapsuleFrame(
            kind: .shortcut,
            iconName: "command",
            leadingText: leading,
            bodyText: body,
            trailingText: "Shortcuts",
            usageProgress: nil,
            resetText: nil
        )
    }
}

enum UsageSummaryFormatter {
    static func formatUsageFrame(_ snapshot: UsageSnapshot, now: Date = Date()) -> PrimaryCapsuleFrame {
        let remaining = snapshot.rateLimit.map { "\($0.remainingPercent)%" } ?? "--"
        let today = snapshot.todayTokens.map(compactTokenCount) ?? "--"
        let resetText = snapshot.rateLimit?.resetsAt.flatMap { compactResetText(until: $0, now: now) }
        return PrimaryCapsuleFrame(
            kind: .usage,
            iconName: snapshot.provider == .claude ? "sparkles" : "terminal",
            leadingText: snapshot.provider.displayName,
            bodyText: "剩余 \(remaining)",
            trailingText: "Today \(today)",
            usageProgress: snapshot.rateLimit?.progress,
            resetText: resetText
        )
    }

    static func compactTokenCount(_ count: Int) -> String {
        if count >= 1_000_000 {
            return String(format: "%.1fM", Double(count) / 1_000_000.0)
        }
        if count >= 1_000 {
            return String(format: "%.1fK", Double(count) / 1_000.0)
        }
        return "\(count)"
    }

    private static func compactResetText(until resetsAt: Date, now: Date) -> String? {
        let seconds = max(0, Int(resetsAt.timeIntervalSince(now)))
        guard seconds > 0 else { return nil }
        let hours = seconds / 3600
        let minutes = (seconds % 3600) / 60
        if hours > 0 {
            return "\(hours)h \(minutes)m"
        }
        return "\(max(1, minutes))m"
    }
}
