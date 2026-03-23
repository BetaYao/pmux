import Foundation

/// Manages the lifecycle of TerminalSurface instances, keyed by worktree path.
class TerminalSurfaceManager {
    private var surfaces: [String: TerminalSurface] = [:]

    /// Get or create a surface for the given worktree info.
    func surface(for info: WorktreeInfo, backend: String) -> TerminalSurface {
        if let existing = surfaces[info.path] {
            return existing
        }
        let surface = TerminalSurface()
        if backend != "local" {
            surface.sessionName = SessionManager.persistentSessionName(for: info.path)
            surface.backend = backend
        }
        surfaces[info.path] = surface
        return surface
    }

    /// Look up an existing surface by worktree path.
    func surface(forPath path: String) -> TerminalSurface? {
        surfaces[path]
    }

    /// Remove and destroy a surface for the given path.
    @discardableResult
    func removeSurface(forPath path: String) -> TerminalSurface? {
        guard let surface = surfaces.removeValue(forKey: path) else { return nil }
        surface.destroy()
        return surface
    }

    /// Remove all surfaces, destroying each.
    func removeAll() {
        for (_, surface) in surfaces {
            surface.destroy()
        }
        surfaces.removeAll()
    }

    /// All current surface entries.
    var all: [String: TerminalSurface] {
        surfaces
    }

    /// Number of managed surfaces.
    var count: Int {
        surfaces.count
    }
}
