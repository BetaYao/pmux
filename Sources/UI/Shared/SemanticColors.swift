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
    // Use `static let` so each dynamic NSColor is created once and cached.
    // The NSColor(name:) block still resolves per-appearance at draw time,
    // but the NSColor wrapper object itself is allocated only once.
    static let bg: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x0f1115) : NSColor(hex: 0xf3f4f7)
    }

    static let panel: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x15171c) : NSColor(hex: 0xffffff)
    }

    static let panel2: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x1b1e25) : NSColor(hex: 0xf7f8fb)
    }

    static let text: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0xf3f5f8) : NSColor(hex: 0x1f232b)
    }

    static let muted: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0xa8afbc) : NSColor(hex: 0x636b78)
    }

    static let line: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x262a33) : NSColor(hex: 0xd7dbe3)
    }

    static let running: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x33c17b) : NSColor(hex: 0x1f9d63)
    }

    static let waiting: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x3b82f6) : NSColor(hex: 0x2563eb)
    }

    static let idle: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x9ca3af) : NSColor(hex: 0x8a93a1)
    }

    static let accent: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
    }

    static let danger: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0xff453a) : NSColor(hex: 0xdc2626)
    }
}
