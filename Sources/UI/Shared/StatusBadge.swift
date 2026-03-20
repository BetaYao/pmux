import AppKit

/// Small status indicator dot with color
class StatusBadge: NSView {
    var status: AgentStatus = .unknown {
        didSet {
            if status != oldValue { needsDisplay = true }
        }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        let size = min(bounds.width, bounds.height)
        layer?.cornerRadius = size / 2
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = status.color.cgColor
        }
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: Theme.statusBadgeSize, height: Theme.statusBadgeSize)
    }
}
