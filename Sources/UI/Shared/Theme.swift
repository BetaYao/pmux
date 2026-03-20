import AppKit

enum ThemeMode: String {
    case dark
    case light
    case system

    static func applyAppearance(_ mode: ThemeMode) {
        let appearance: NSAppearance?
        switch mode {
        case .dark:
            appearance = NSAppearance(named: .darkAqua)
        case .light:
            appearance = NSAppearance(named: .aqua)
        case .system:
            appearance = nil
        }

        // Set on app AND each window so the change is immediate
        NSApp.appearance = appearance
        for window in NSApp.windows {
            window.appearance = appearance
            window.invalidateShadow()
            window.displayIfNeeded()
        }

        // Post notification AFTER appearance is applied so views
        // resolve dynamic colors under the new appearance
        DispatchQueue.main.async {
            NotificationCenter.default.post(name: .themeDidChange, object: nil)
        }
    }
}

extension Notification.Name {
    static let themeDidChange = Notification.Name("pmux.themeDidChange")
}

enum Theme {
    static var background: NSColor { SemanticColors.bg }
    static var surface: NSColor { SemanticColors.panel }
    static var surfaceHover: NSColor { SemanticColors.panel2 }
    static var border: NSColor { SemanticColors.line }
    static var textPrimary: NSColor { SemanticColors.text }
    static var textSecondary: NSColor { SemanticColors.muted }
    static var textDim: NSColor { SemanticColors.muted }
    static var accent: NSColor { SemanticColors.accent }

    static let tabBarHeight: CGFloat = 36
    static let cardCornerRadius: CGFloat = 8
    static let cardPadding: CGFloat = 12
    static let statusBadgeSize: CGFloat = 8
}
