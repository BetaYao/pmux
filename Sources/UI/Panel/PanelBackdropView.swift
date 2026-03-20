import AppKit

protocol PanelBackdropDelegate: AnyObject {
    func backdropClicked()
}

final class PanelBackdropView: NSView {

    weak var delegate: PanelBackdropDelegate?

    // MARK: - Init

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    // MARK: - Public API

    func setVisible(_ visible: Bool) {
        isHidden = !visible
    }

    // MARK: - Setup

    private func setup() {
        identifier = NSUserInterfaceItemIdentifier("panel.backdrop")
        wantsLayer = true
        isHidden = true
        translatesAutoresizingMaskIntoConstraints = false
        layer?.backgroundColor = SemanticColors.backdropBlack.cgColor
    }

    // MARK: - Events

    override func mouseDown(with event: NSEvent) {
        delegate?.backdropClicked()
    }
}
