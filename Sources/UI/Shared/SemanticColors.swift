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
        appearance.isDark ? NSColor(hex: 0x0b0b0b) : NSColor(hex: 0xf3f4f7)
    }

    static let panel: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xffffff)
    }

    static let panel2: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xf7f8fb)
    }

    static let text: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0xf3f5f8) : NSColor(hex: 0x1f232b)
    }

    static let muted: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0xa8afbc) : NSColor(hex: 0x636b78)
    }

    static let line: NSColor = NSColor(name: nil) { appearance in
        appearance.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
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

    // MARK: - Pre-computed derived colors

    static let cardBgSelected: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        let p2 = a.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xf7f8fb)
        return acc.withAlphaComponent(0.12).blended(withFraction: 0.88, of: p2) ?? p2
    }
    static let cardBorderSelected: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return acc.withAlphaComponent(0.55).blended(withFraction: 0.45, of: ln) ?? ln
    }
    static let cardBgHover: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        let p2 = a.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xf7f8fb)
        return acc.withAlphaComponent(0.06).blended(withFraction: 0.94, of: p2) ?? p2
    }
    static let cardBorderHover: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        return acc.withAlphaComponent(0.35)
    }
    static let cardBorderDefault: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.78)
    }
    static let miniCardBorderSelected: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        return acc.withAlphaComponent(0.65)
    }
    static let miniCardShadowSelected: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        return acc.withAlphaComponent(0.25)
    }
    static let miniCardBorderHover: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        return acc.withAlphaComponent(0.45)
    }
    static let miniCardBorderDefault: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.58)
    }
    static let panelAlpha88: NSColor = NSColor(name: nil) { a in
        let p = a.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xffffff)
        return p.withAlphaComponent(0.88)
    }
    static let lineAlpha70: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.70)
    }
    static let lineAlpha75: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.75)
    }
    static let lineAlpha55: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.55)
    }
    static let lineAlpha45: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.45)
    }
    static let lineAlpha40: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.40)
    }
    static let lineAlpha38: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.38)
    }
    static let lineAlpha22: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.22)
    }
    static let lineAlpha18: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.18)
    }
    static let lineAlpha60: NSColor = NSColor(name: nil) { a in
        let ln = a.isDark ? NSColor(hex: 0x222222) : NSColor(hex: 0xd7dbe3)
        return ln.withAlphaComponent(0.60)
    }
    static let accentAlpha15: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        return acc.withAlphaComponent(0.15)
    }
    static let accentAlpha12: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        return acc.withAlphaComponent(0.12)
    }
    static let mutedAlpha50: NSColor = NSColor(name: nil) { a in
        let m = a.isDark ? NSColor(hex: 0xa8afbc) : NSColor(hex: 0x636b78)
        return m.withAlphaComponent(0.5)
    }
    static let aiBubbleUser: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        let p2 = a.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xf7f8fb)
        return acc.blended(withFraction: 0.82, of: p2) ?? p2
    }
    static let aiSendButtonBg: NSColor = NSColor(name: nil) { a in
        let acc = a.isDark ? NSColor(hex: 0x4f8cff) : NSColor(hex: 0x2563eb)
        let p2 = a.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xf7f8fb)
        return acc.blended(withFraction: 0.78, of: p2) ?? p2
    }
    static let backdropBlack: NSColor = NSColor.black.withAlphaComponent(0.4)
    static let threadRowBg: NSColor = NSColor(name: nil) { a in
        a.isDark
            ? NSColor(srgbRed: 0x1a / 255.0, green: 0x2a / 255.0, blue: 0x1a / 255.0, alpha: 1)
            : NSColor(hex: 0xe8f5e9)
    }
    static let threadRowBorder: NSColor = NSColor(name: nil) { a in
        a.isDark
            ? NSColor(srgbRed: 51 / 255.0, green: 193 / 255.0, blue: 123 / 255.0, alpha: 0.25)
            : NSColor(srgbRed: 51 / 255.0, green: 193 / 255.0, blue: 123 / 255.0, alpha: 0.35)
    }
    static let threadRowHoverBg: NSColor = NSColor(name: nil) { a in
        a.isDark
            ? NSColor(white: 1, alpha: 0.03)
            : NSColor(white: 0, alpha: 0.03)
    }
    static let threadRowHoverBorder: NSColor = NSColor(name: nil) { a in
        a.isDark
            ? NSColor(white: 1, alpha: 0.04)
            : NSColor(white: 0, alpha: 0.04)
    }

    // MARK: - Zoom-specific tokens

    static let arcBlockHover: NSColor = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x232323) : NSColor(hex: 0xf0f0f0)
    }
    static let arcBlockInactive: NSColor = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf5f5f5)
    }
    static let tileBg: NSColor = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x111111) : NSColor(hex: 0xfafafa)
    }
    static let tileBarBg: NSColor = NSColor(name: nil) { a in
        a.isDark ? NSColor(hex: 0x1a1a1a) : NSColor(hex: 0xf5f5f5)
    }
}
