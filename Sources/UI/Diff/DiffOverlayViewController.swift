import AppKit

/// Overlay panel showing git diff for a worktree.
class DiffOverlayViewController: NSViewController {
    private let reviewView: DiffReviewView
    private let closeButton = NSButton()

    init(worktreePath: String) {
        self.reviewView = DiffReviewView(worktreePath: worktreePath)
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func loadView() {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 1000, height: 660))
        container.wantsLayer = true
        container.layer?.backgroundColor = Theme.background.cgColor
        container.setAccessibilityIdentifier("repo.diffOverlay")
        container.setAccessibilityElement(true)
        container.setAccessibilityRole(.group)

        reviewView.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(reviewView)
        reviewView.loadDiff()

        closeButton.title = "Close"
        closeButton.bezelStyle = .rounded
        closeButton.keyEquivalent = "\u{1b}"
        closeButton.target = self
        closeButton.action = #selector(closeClicked)
        closeButton.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(closeButton)

        NSLayoutConstraint.activate([
            closeButton.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            closeButton.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),

            reviewView.topAnchor.constraint(equalTo: container.topAnchor, constant: 8),
            reviewView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            reviewView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            reviewView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        view = container
    }

    @objc private func closeClicked() {
        dismiss(nil)
    }
}
