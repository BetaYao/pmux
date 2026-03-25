import AppKit

protocol SplitContainerDelegate: AnyObject {
    func splitContainer(_ view: SplitContainerView, didChangeFocus leafId: String)
    func splitContainer(_ view: SplitContainerView, didRequestSplit axis: SplitAxis)
    func splitContainer(_ view: SplitContainerView, didRequestClosePane leafId: String)
    func splitContainerDidChangeLayout(_ view: SplitContainerView)
}

class SplitContainerView: NSView, DividerDelegate {
    var tree: SplitTree? { didSet { layoutTree() } }
    var surfaceViews: [String: NSView] = [:]
    weak var delegate: SplitContainerDelegate?

    private var dividers: [String: DividerView] = [:]
    private var leafFrames: [String: CGRect] = [:]

    override var isFlipped: Bool { true }

    override init(frame: NSRect) {
        super.init(frame: frame)
        translatesAutoresizingMaskIntoConstraints = true
        setAccessibilityIdentifier("splitPane.container")
    }

    required init?(coder: NSCoder) { fatalError() }

    override func resizeSubviews(withOldSize oldSize: NSSize) {
        super.resizeSubviews(withOldSize: oldSize)
        layoutTree()
    }

    func layoutTree() {
        guard let tree = tree else { return }
        leafFrames = Self.computeFrames(node: tree.root, in: bounds)
        for leaf in tree.allLeaves {
            guard let frame = leafFrames[leaf.id],
                  let view = surfaceViews[leaf.surfaceId] else { continue }
            if view.superview != self { addSubview(view) }
            view.frame = frame
            view.setAccessibilityIdentifier("splitPane.leaf.\(leaf.id)")
        }
        layoutDividers(node: tree.root, in: bounds)
        let activeSplitIds = collectSplitIds(tree.root)
        for (id, divider) in dividers where !activeSplitIds.contains(id) {
            divider.removeFromSuperview()
            dividers.removeValue(forKey: id)
        }
    }

    static func computeFrames(node: SplitNode, in rect: CGRect) -> [String: CGRect] {
        var result: [String: CGRect] = [:]
        computeFramesRecursive(node: node, in: rect, result: &result)
        return result
    }

    private static func computeFramesRecursive(node: SplitNode, in rect: CGRect, result: inout [String: CGRect]) {
        switch node {
        case .leaf(let id, _, _):
            result[id] = rect
        case .split(_, let axis, let ratio, let first, let second):
            let dividerSize = DividerView.thickness
            switch axis {
            case .horizontal:
                let firstWidth = floor((rect.width - dividerSize) * ratio)
                let secondX = rect.origin.x + firstWidth + dividerSize
                let secondWidth = rect.width - firstWidth - dividerSize
                let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: firstWidth, height: rect.height)
                let secondRect = CGRect(x: secondX, y: rect.origin.y, width: secondWidth, height: rect.height)
                computeFramesRecursive(node: first, in: firstRect, result: &result)
                computeFramesRecursive(node: second, in: secondRect, result: &result)
            case .vertical:
                let firstHeight = floor((rect.height - dividerSize) * ratio)
                let secondY = rect.origin.y + firstHeight + dividerSize
                let secondHeight = rect.height - firstHeight - dividerSize
                let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: rect.width, height: firstHeight)
                let secondRect = CGRect(x: rect.origin.x, y: secondY, width: rect.width, height: secondHeight)
                computeFramesRecursive(node: first, in: firstRect, result: &result)
                computeFramesRecursive(node: second, in: secondRect, result: &result)
            }
        }
    }

    private func layoutDividers(node: SplitNode, in rect: CGRect) {
        guard case .split(let id, let axis, let ratio, let first, let second) = node else { return }
        let dividerSize = DividerView.thickness

        let divider: DividerView
        if let existing = dividers[id] {
            divider = existing
        } else {
            divider = DividerView(splitNodeId: id, axis: axis)
            divider.delegate = self
            divider.setAccessibilityIdentifier("splitPane.divider.\(id)")
            addSubview(divider)
            dividers[id] = divider
        }

        switch axis {
        case .horizontal:
            let firstWidth = floor((rect.width - dividerSize) * ratio)
            divider.frame = CGRect(x: rect.origin.x + firstWidth, y: rect.origin.y, width: dividerSize, height: rect.height)
            divider.parentSplitSize = rect.width
            divider.currentRatio = ratio
            let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: firstWidth, height: rect.height)
            let secondRect = CGRect(x: rect.origin.x + firstWidth + dividerSize, y: rect.origin.y, width: rect.width - firstWidth - dividerSize, height: rect.height)
            layoutDividers(node: first, in: firstRect)
            layoutDividers(node: second, in: secondRect)
        case .vertical:
            let firstHeight = floor((rect.height - dividerSize) * ratio)
            divider.frame = CGRect(x: rect.origin.x, y: rect.origin.y + firstHeight, width: rect.width, height: dividerSize)
            divider.parentSplitSize = rect.height
            divider.currentRatio = ratio
            let firstRect = CGRect(x: rect.origin.x, y: rect.origin.y, width: rect.width, height: firstHeight)
            let secondRect = CGRect(x: rect.origin.x, y: rect.origin.y + firstHeight + dividerSize, width: rect.width, height: rect.height - firstHeight - dividerSize)
            layoutDividers(node: first, in: firstRect)
            layoutDividers(node: second, in: secondRect)
        }
    }

    private func collectSplitIds(_ node: SplitNode) -> Set<String> {
        switch node {
        case .leaf: return []
        case .split(let id, _, _, let first, let second):
            return Set([id]).union(collectSplitIds(first)).union(collectSplitIds(second))
        }
    }

    func focusLeaf(direction: SplitAxis, positive: Bool) -> String? {
        guard let tree = tree else { return nil }
        guard let currentFrame = leafFrames[tree.focusedId] else { return nil }
        let center = CGPoint(x: currentFrame.midX, y: currentFrame.midY)

        var bestLeaf: String?
        var bestDistance: CGFloat = .greatestFiniteMagnitude

        for leaf in tree.allLeaves where leaf.id != tree.focusedId {
            guard let frame = leafFrames[leaf.id] else { continue }
            let leafCenter = CGPoint(x: frame.midX, y: frame.midY)

            let inDirection: Bool
            switch (direction, positive) {
            case (.horizontal, true):  inDirection = leafCenter.x > center.x
            case (.horizontal, false): inDirection = leafCenter.x < center.x
            case (.vertical, true):    inDirection = leafCenter.y > center.y
            case (.vertical, false):   inDirection = leafCenter.y < center.y
            }
            guard inDirection else { continue }

            let overlaps: Bool
            if direction == .horizontal {
                overlaps = frame.minY < currentFrame.maxY && frame.maxY > currentFrame.minY
            } else {
                overlaps = frame.minX < currentFrame.maxX && frame.maxX > currentFrame.minX
            }
            guard overlaps else { continue }

            let dist = hypot(leafCenter.x - center.x, leafCenter.y - center.y)
            if dist < bestDistance {
                bestDistance = dist
                bestLeaf = leaf.id
            }
        }

        if let best = bestLeaf {
            tree.focusedId = best
            delegate?.splitContainer(self, didChangeFocus: best)
        }
        return bestLeaf
    }

    func dividerDidMove(_ splitNodeId: String, newRatio: CGFloat) {
        tree?.updateRatio(splitId: splitNodeId, newRatio: newRatio)
        layoutTree()
        delegate?.splitContainerDidChangeLayout(self)
    }

    func dividerDidDoubleClick(_ splitNodeId: String) {
        tree?.updateRatio(splitId: splitNodeId, newRatio: 0.5)
        layoutTree()
        delegate?.splitContainerDidChangeLayout(self)
    }
}
