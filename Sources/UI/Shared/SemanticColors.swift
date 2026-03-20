import AppKit

extension NSColor {
    convenience init(hex: Int) {
        let r = CGFloat((hex >> 16) & 0xFF) / 255.0
        let g = CGFloat((hex >> 8) & 0xFF) / 255.0
        let b = CGFloat(hex & 0xFF) / 255.0
        self.init(srgbRed: r, green: g, blue: b, alpha: 1.0)
    }
}

extension NSAppearance {
    var isDark: Bool {
        bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }
}

enum SemanticColors {
    // MARK: - Backgrounds

    static let bg: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf3f4f7) }

    static let panel: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x202020) : NSColor(hex: 0xffffff) }

    static let panel2: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x282828) : NSColor(hex: 0xf7f8fb) }

    // MARK: - Text

    static let text: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0xe8e8e8) : NSColor(hex: 0x1f232b) }

    static let muted: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x999999) : NSColor(hex: 0x636b78) }

    // MARK: - Borders

    static let line: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x3a3a3a) : NSColor(hex: 0xd7dbe3) }

    /// Default card border — very subtle
    static let cardBorder: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x363636) : NSColor(hex: 0xe2e5eb) }

    /// Hovered card border — slightly brighter than default, subtle
    static let cardBorderHover: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x505050) : NSColor(hex: 0xbcc2cc) }

    /// Selected card border — accent blue
    static let cardBorderSelected: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x2d8cf0) : NSColor(hex: 0x2563eb) }

    // MARK: - Status

    static let running: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x33c17b) : NSColor(hex: 0x1f9d63) }

    static let waiting: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x3b82f6) : NSColor(hex: 0x2563eb) }

    static let idle: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x9ca3af) : NSColor(hex: 0x8a93a1) }

    static let accent: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0x0e72ed) : NSColor(hex: 0x2563eb) }

    static let danger: NSColor =
        NSColor(name: nil) { $0.isDark ? NSColor(hex: 0xff453a) : NSColor(hex: 0xdc2626) }
}
