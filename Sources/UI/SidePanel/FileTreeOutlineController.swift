import AppKit

// MARK: - FileTreeNode

final class FileTreeNode {
    let url: URL
    let isDirectory: Bool
    var children: [FileTreeNode]?

    init(url: URL, isDirectory: Bool) {
        self.url = url
        self.isDirectory = isDirectory
    }
}

// MARK: - FileTreeOutlineController

final class FileTreeOutlineController: NSObject, NSOutlineViewDataSource, NSOutlineViewDelegate {
    let outlineView: NSOutlineView
    var onSelectFile: ((String) -> Void)?

    private var rootNodes: [FileTreeNode] = []

    init(rootPath: String) {
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("FileColumn"))
        column.title = "Name"
        let ov = NSOutlineView()
        ov.addTableColumn(column)
        ov.outlineTableColumn = column
        ov.headerView = nil
        ov.rowHeight = 20
        self.outlineView = ov
        super.init()
        ov.dataSource = self
        ov.delegate = self
        setRoot(rootPath)
    }

    func setRoot(_ path: String?) {
        guard let path = path else {
            rootNodes = []
            outlineView.reloadData()
            return
        }
        let url = URL(fileURLWithPath: path)
        rootNodes = FileTreeOutlineController.childNodes(of: url)
        outlineView.reloadData()
    }

    // MARK: - Pure seam (unit-tested)

    /// Lists the contents of `directory`, hiding dotfiles, with directories
    /// first and each group sorted alphabetically by lastPathComponent.
    static func childNodes(of directory: URL) -> [FileTreeNode] {
        let fm = FileManager.default
        guard let items = try? fm.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: []
        ) else { return [] }

        var dirs: [FileTreeNode] = []
        var files: [FileTreeNode] = []

        for item in items {
            let name = item.lastPathComponent
            guard !name.hasPrefix(".") else { continue }
            let isDir = (try? item.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
            let node = FileTreeNode(url: item, isDirectory: isDir)
            if isDir {
                dirs.append(node)
            } else {
                files.append(node)
            }
        }

        dirs.sort { $0.url.lastPathComponent < $1.url.lastPathComponent }
        files.sort { $0.url.lastPathComponent < $1.url.lastPathComponent }
        return dirs + files
    }

    // MARK: - NSOutlineViewDataSource

    func outlineView(_ outlineView: NSOutlineView, numberOfChildrenOfItem item: Any?) -> Int {
        if item == nil {
            return rootNodes.count
        }
        guard let node = item as? FileTreeNode, node.isDirectory else { return 0 }
        if node.children == nil {
            node.children = FileTreeOutlineController.childNodes(of: node.url)
        }
        return node.children?.count ?? 0
    }

    func outlineView(_ outlineView: NSOutlineView, child index: Int, ofItem item: Any?) -> Any {
        if item == nil {
            return rootNodes[index]
        }
        let node = item as! FileTreeNode
        if node.children == nil {
            node.children = FileTreeOutlineController.childNodes(of: node.url)
        }
        return node.children![index]
    }

    func outlineView(_ outlineView: NSOutlineView, isItemExpandable item: Any) -> Bool {
        (item as? FileTreeNode)?.isDirectory ?? false
    }

    // MARK: - NSOutlineViewDelegate

    func outlineView(_ outlineView: NSOutlineView, viewFor tableColumn: NSTableColumn?, item: Any) -> NSView? {
        guard let node = item as? FileTreeNode else { return nil }
        let id = NSUserInterfaceItemIdentifier("FileCell")
        let cellView: NSTableCellView
        if let reused = outlineView.makeView(withIdentifier: id, owner: self) as? NSTableCellView {
            cellView = reused
        } else {
            cellView = NSTableCellView()
            cellView.identifier = id

            let imageView = NSImageView()
            imageView.translatesAutoresizingMaskIntoConstraints = false
            imageView.imageScaling = .scaleProportionallyDown
            cellView.addSubview(imageView)
            cellView.imageView = imageView

            let textField = NSTextField(labelWithString: "")
            textField.translatesAutoresizingMaskIntoConstraints = false
            cellView.addSubview(textField)
            cellView.textField = textField

            NSLayoutConstraint.activate([
                imageView.leadingAnchor.constraint(equalTo: cellView.leadingAnchor, constant: 2),
                imageView.centerYAnchor.constraint(equalTo: cellView.centerYAnchor),
                imageView.widthAnchor.constraint(equalToConstant: 14),
                imageView.heightAnchor.constraint(equalToConstant: 14),
                textField.leadingAnchor.constraint(equalTo: imageView.trailingAnchor, constant: 4),
                textField.trailingAnchor.constraint(equalTo: cellView.trailingAnchor, constant: -2),
                textField.centerYAnchor.constraint(equalTo: cellView.centerYAnchor),
            ])
        }

        let symbolName = node.isDirectory ? "folder" : "doc"
        cellView.imageView?.image = NSImage(systemSymbolName: symbolName, accessibilityDescription: nil)
        cellView.textField?.stringValue = node.url.lastPathComponent
        return cellView
    }

    func outlineViewSelectionDidChange(_ notification: Notification) {
        let row = outlineView.selectedRow
        guard row >= 0,
              let node = outlineView.item(atRow: row) as? FileTreeNode,
              !node.isDirectory else { return }
        onSelectFile?(node.url.path)
    }
}
