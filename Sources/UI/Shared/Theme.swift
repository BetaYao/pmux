import AppKit

enum Theme {
    static let background = NSColor(red: 0x1e/255, green: 0x1e/255, blue: 0x1e/255, alpha: 1.0)
    static let surface = NSColor(red: 0x28/255, green: 0x28/255, blue: 0x28/255, alpha: 1.0)
    static let surfaceHover = NSColor(red: 0x33/255, green: 0x33/255, blue: 0x33/255, alpha: 1.0)
    static let border = NSColor(red: 0x3e/255, green: 0x3e/255, blue: 0x3e/255, alpha: 1.0)
    static let textPrimary = NSColor(white: 0.9, alpha: 1.0)
    static let textSecondary = NSColor(white: 0.6, alpha: 1.0)
    static let textDim = NSColor(white: 0.4, alpha: 1.0)
    static let accent = NSColor.systemBlue

    static let tabBarHeight: CGFloat = 36
    static let cardCornerRadius: CGFloat = 8
    static let cardPadding: CGFloat = 12
    static let statusBadgeSize: CGFloat = 8
}
