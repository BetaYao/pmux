import AppKit

protocol DraggableGridDelegate: AnyObject {
    func draggableGrid(_ grid: DraggableGridView, dropIndexFor point: NSPoint) -> Int
    func draggableGrid(_ grid: DraggableGridView, dropIndicatorFrameAt index: Int) -> NSRect
    func draggableGrid(_ grid: DraggableGridView, didDropItemWithID id: String, atIndex index: Int)
}

/// NSView subclass that acts as a drag-and-drop destination for reordering cards.
class DraggableGridView: NSView {
    weak var dragDelegate: DraggableGridDelegate?

    let dropIndicator = NSView()

    override func isAccessibilityElement() -> Bool { true }
    override func accessibilityRole() -> NSAccessibility.Role? { .group }

    override init(frame: NSRect) {
        super.init(frame: frame)
        registerForDraggedTypes([.terminalCard])

        dropIndicator.wantsLayer = true
        dropIndicator.layer?.backgroundColor = Theme.accent.cgColor
        dropIndicator.layer?.cornerRadius = 2
        dropIndicator.isHidden = true
        addSubview(dropIndicator)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) not supported")
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        return .move
    }

    override func draggingUpdated(_ sender: NSDraggingInfo) -> NSDragOperation {
        let point = convert(sender.draggingLocation, from: nil)
        guard let delegate = dragDelegate else { return [] }

        let index = delegate.draggableGrid(self, dropIndexFor: point)
        let indicatorFrame = delegate.draggableGrid(self, dropIndicatorFrameAt: index)

        dropIndicator.frame = indicatorFrame
        dropIndicator.isHidden = false

        return .move
    }

    override func draggingExited(_ sender: NSDraggingInfo?) {
        dropIndicator.isHidden = true
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        dropIndicator.isHidden = true

        guard let pasteboardItem = sender.draggingPasteboard.pasteboardItems?.first,
              let path = pasteboardItem.string(forType: .terminalCard),
              let delegate = dragDelegate else {
            return false
        }

        let point = convert(sender.draggingLocation, from: nil)
        let index = delegate.draggableGrid(self, dropIndexFor: point)
        delegate.draggableGrid(self, didDropItemWithID: path, atIndex: index)
        return true
    }
}
