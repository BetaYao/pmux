import Foundation

/// Manages the lifecycle of SplitTree instances, keyed by worktree path.
class TerminalSurfaceManager {
    private var trees: [String: SplitTree] = [:]

    /// Get or create a SplitTree for the given worktree info.
    /// Creates a single-leaf tree and registers the surface in SurfaceRegistry.
    func tree(for info: WorktreeInfo, backend: String) -> SplitTree {
        if let existing = trees[info.path] {
            return existing
        }
        let surface = TerminalSurface()
        let sessionName = backend != "local" ? SessionManager.persistentSessionName(for: info.path) : ""
        if backend != "local" {
            surface.sessionName = sessionName
            surface.backend = backend
        }
        SurfaceRegistry.shared.register(surface)
        let leafId = UUID().uuidString
        let splitTree = SplitTree(
            worktreePath: info.path,
            rootLeafId: leafId,
            surfaceId: surface.id,
            sessionName: sessionName
        )
        trees[info.path] = splitTree
        return splitTree
    }

    /// Look up an existing tree by worktree path.
    func tree(forPath path: String) -> SplitTree? {
        trees[path]
    }

    /// Remove and destroy a tree for the given path.
    @discardableResult
    func removeTree(forPath path: String) -> SplitTree? {
        guard let tree = trees.removeValue(forKey: path) else { return nil }
        for leaf in tree.allLeaves {
            if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                surface.destroy()
            }
            SurfaceRegistry.shared.unregister(leaf.surfaceId)
        }
        return tree
    }

    /// Remove all trees, destroying each surface.
    func removeAll() {
        for (_, tree) in trees {
            for leaf in tree.allLeaves {
                if let surface = SurfaceRegistry.shared.surface(forId: leaf.surfaceId) {
                    surface.destroy()
                }
                SurfaceRegistry.shared.unregister(leaf.surfaceId)
            }
        }
        trees.removeAll()
    }

    /// All current tree entries.
    var all: [String: SplitTree] {
        trees
    }

    /// Number of managed trees.
    var count: Int {
        trees.count
    }

    // MARK: - Legacy surface accessors (for AgentHead / backward compat)

    /// Returns the primary (first) surface for the given worktree path, if any.
    func primarySurface(forPath path: String) -> TerminalSurface? {
        guard let tree = trees[path],
              let firstLeaf = tree.allLeaves.first else { return nil }
        return SurfaceRegistry.shared.surface(forId: firstLeaf.surfaceId)
    }
}
