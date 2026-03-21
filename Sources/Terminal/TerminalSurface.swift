import AppKit

/// Manages a single Ghostty terminal surface (NSView + PTY + Metal renderer).
/// Each worktree gets one TerminalSurface instance.
class TerminalSurface {
    /// Unique identifier for this terminal instance (used as primary key in AgentHead)
    let id: String = UUID().uuidString

    /// The NSView that Ghostty renders into (layer-backed, Metal)
    private(set) var view: GhosttyNSView!
    private(set) var surface: ghostty_surface_t?
    private weak var containerView: NSView?

    /// tmux session name (nil = no tmux, direct shell)
    var sessionName: String?

    /// Create the terminal surface and add it to the given container view.
    /// If useTmux is true, the surface runs inside a tmux session for persistence.
    /// When a sessionName is provided, tmux session existence is checked asynchronously
    /// and the surface is created once the check completes.
    func create(in container: NSView, workingDirectory: String? = nil, sessionName: String? = nil) -> Bool {
        guard let app = GhosttyBridge.shared.app else {
            NSLog("GhosttyBridge not initialized")
            return false
        }

        if let sessionName {
            // Check tmux session existence on background thread to avoid blocking UI
            Self.tmuxSessionExistsAsync(sessionName) { [weak self] exists in
                guard let self else { return }
                let tmuxCommand: String
                if exists {
                    tmuxCommand = "tmux attach-session -t \(sessionName) \\; set-option status off"
                } else {
                    tmuxCommand = "tmux new-session -s \(sessionName) \\; set-option status off"
                }
                self._createWithCommand(app: app, container: container, workingDirectory: workingDirectory, command: tmuxCommand)
            }
            return true  // Surface creation is deferred
        }

        _createWithCommand(app: app, container: container, workingDirectory: workingDirectory, command: nil)
        return surface != nil
    }

    private func _createWithCommand(app: ghostty_app_t, container: NSView, workingDirectory: String?, command: String?) {
        let termView = GhosttyNSView(frame: container.bounds)
        termView.wantsLayer = true

        var config = ghostty_surface_config_new()
        config.platform_tag = GHOSTTY_PLATFORM_MACOS
        config.platform.macos.nsview = Unmanaged.passUnretained(termView).toOpaque()
        config.scale_factor = Double(container.window?.backingScaleFactor ?? 2.0)

        let createBlock: () -> Void = {
            self._createSurface(app: app, config: &config, view: termView, container: container)
        }

        if let workingDirectory, let command {
            workingDirectory.withCString { wdPtr in
                command.withCString { cmdPtr in
                    config.working_directory = wdPtr
                    config.command = cmdPtr
                    createBlock()
                }
            }
        } else if let workingDirectory {
            workingDirectory.withCString { wdPtr in
                config.working_directory = wdPtr
                createBlock()
            }
        } else if let command {
            command.withCString { cmdPtr in
                config.command = cmdPtr
                createBlock()
            }
        } else {
            createBlock()
        }
    }

    private func _createSurface(app: ghostty_app_t, config: inout ghostty_surface_config_s, view: GhosttyNSView, container: NSView) {
        guard let s = ghostty_surface_new(app, &config) else {
            NSLog("Failed to create Ghostty surface")
            return
        }
        self.surface = s
        self.view = view
        self.containerView = container
        view.surface = s
        view.terminalSurface = self

        view.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        // Set initial size — Ghostty expects pixel (framebuffer) dimensions, not points
        let size = container.bounds.size
        let scale = container.window?.backingScaleFactor ?? 2.0
        ghostty_surface_set_content_scale(s, Double(scale), Double(scale))
        ghostty_surface_set_size(s, UInt32(size.width * scale), UInt32(size.height * scale))
        ghostty_surface_set_focus(s, true)
    }

    /// Check if a tmux session with given name exists (async, avoids blocking main thread)
    private static func tmuxSessionExistsAsync(_ name: String, completion: @escaping (Bool) -> Void) {
        DispatchQueue.global(qos: .userInitiated).async {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["tmux", "has-session", "-t", name]
            process.standardOutput = Pipe()
            process.standardError = Pipe()
            let exists: Bool
            do {
                try process.run()
                process.waitUntilExit()
                exists = process.terminationStatus == 0
            } catch {
                exists = false
            }
            DispatchQueue.main.async {
                completion(exists)
            }
        }
    }

    /// Reparent this terminal's view to a different container
    func reparent(to container: NSView) {
        guard let view, let surface else { return }

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        view.removeFromSuperview()
        view.translatesAutoresizingMaskIntoConstraints = false
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            view.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        CATransaction.commit()

        self.containerView = container

        // Force size sync after constraints resolve — need TWO deferred
        // passes because the first run loop resolves constraints (setting frame)
        // and the second one is needed for Ghostty to recalculate the grid
        DispatchQueue.main.async { [weak self] in
            guard let self, let view = self.view, let surface = self.surface else { return }
            self.syncContentScale()
            self.syncSize()
            ghostty_surface_set_focus(surface, true)
            view.needsDisplay = true
            // Third pass: read the grid size AFTER Ghostty has processed the resize
            DispatchQueue.main.async { [weak self] in
                self?.refreshTmuxLayout()
            }
        }
    }

    /// Tell tmux to resize its window to match the terminal's actual grid size
    func refreshTmuxLayout() {
        guard let sessionName, let surface else { return }
        let gridSize = ghostty_surface_size(surface)
        guard gridSize.columns > 0, gridSize.rows > 0 else { return }
        let cols = Int(gridSize.columns)
        let rows = Int(gridSize.rows)
        DispatchQueue.global().async {
            // Explicitly resize tmux window to match the Ghostty grid
            let resize = Process()
            resize.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            resize.arguments = ["tmux", "resize-window", "-t", sessionName, "-x", "\(cols)", "-y", "\(rows)"]
            resize.standardOutput = Pipe()
            resize.standardError = Pipe()
            try? resize.run()
            resize.waitUntilExit()

            // Refresh client display
            let refresh = Process()
            refresh.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            refresh.arguments = ["tmux", "refresh-client", "-t", sessionName, "-S"]
            refresh.standardOutput = Pipe()
            refresh.standardError = Pipe()
            try? refresh.run()
        }
    }

    /// Static version for when we don't have the surface (fallback)
    static func refreshTmuxClient(_ sessionName: String) {
        DispatchQueue.global().async {
            let resize = Process()
            resize.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            resize.arguments = ["tmux", "resize-window", "-t", sessionName, "-A"]
            resize.standardOutput = Pipe()
            resize.standardError = Pipe()
            try? resize.run()
            resize.waitUntilExit()

            let refresh = Process()
            refresh.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            refresh.arguments = ["tmux", "refresh-client", "-t", sessionName, "-S"]
            refresh.standardOutput = Pipe()
            refresh.standardError = Pipe()
            try? refresh.run()
        }
    }

    /// Sync the surface size with the current container bounds
    func syncSize() {
        guard let surface, let view else { return }
        let size = view.bounds.size
        guard size.width > 0, size.height > 0 else { return }
        let scale = view.window?.backingScaleFactor ?? 2.0
        ghostty_surface_set_size(surface, UInt32(size.width * scale), UInt32(size.height * scale))
        ghostty_surface_refresh(surface)
    }

    /// Sync the content scale (Retina vs non-Retina)
    func syncContentScale() {
        guard let surface, let view, let window = view.window else { return }
        let scale = Double(window.backingScaleFactor)
        ghostty_surface_set_content_scale(surface, scale, scale)
    }

    /// Set keyboard focus
    func setFocus(_ focused: Bool) {
        guard let surface else { return }
        ghostty_surface_set_focus(surface, focused)
    }

    /// Check if the process has exited
    var processExited: Bool {
        guard let surface else { return true }
        return ghostty_surface_process_exited(surface)
    }

    /// Read visible terminal text from the viewport
    func readViewportText() -> String? {
        guard let surface else { return nil }

        let size = ghostty_surface_size(surface)
        guard size.rows > 0, size.columns > 0 else { return nil }

        var selection = ghostty_selection_s()
        selection.top_left = ghostty_point_s(
            tag: GHOSTTY_POINT_VIEWPORT,
            coord: GHOSTTY_POINT_COORD_TOP_LEFT,
            x: 0,
            y: 0
        )
        selection.bottom_right = ghostty_point_s(
            tag: GHOSTTY_POINT_VIEWPORT,
            coord: GHOSTTY_POINT_COORD_BOTTOM_RIGHT,
            x: UInt32(size.columns - 1),
            y: UInt32(size.rows - 1)
        )
        selection.rectangle = false

        var text = ghostty_text_s()
        guard ghostty_surface_read_text(surface, selection, &text) else {
            return nil
        }
        defer { ghostty_surface_free_text(surface, &text) }

        guard let ptr = text.text, text.text_len > 0 else { return nil }
        return String(cString: ptr)
    }

    /// Get the process status for status detection
    var processStatus: ProcessStatus {
        guard let surface else { return .unknown }
        if ghostty_surface_process_exited(surface) {
            // We don't have the exit code from ghostty, so assume exited
            return .exited
        }
        return .running
    }

    // MARK: - Search

    /// Start a search in the terminal scrollback using Ghostty's binding action system.
    func startSearch(_ query: String) {
        guard let surface else { return }
        // Trigger Ghostty's built-in search with the query as parameter
        let action = "search:\(query)"
        action.withCString { cstr in
            _ = ghostty_surface_binding_action(surface, cstr, UInt(strlen(cstr)))
        }
    }

    /// End the current search.
    func endSearch() {
        guard let surface else { return }
        let action = "close_surface_overlay"
        action.withCString { cstr in
            _ = ghostty_surface_binding_action(surface, cstr, UInt(strlen(cstr)))
        }
    }

    /// Navigate to the next search match.
    func searchNext() {
        guard let surface else { return }
        let action = "search_forward"
        action.withCString { cstr in
            _ = ghostty_surface_binding_action(surface, cstr, UInt(strlen(cstr)))
        }
    }

    /// Navigate to the previous search match.
    func searchPrev() {
        guard let surface else { return }
        let action = "search_backward"
        action.withCString { cstr in
            _ = ghostty_surface_binding_action(surface, cstr, UInt(strlen(cstr)))
        }
    }

    /// Destroy the surface and clean up
    func destroy() {
        if let surface {
            ghostty_surface_request_close(surface)
        }
        view?.removeFromSuperview()
        surface = nil
        view = nil
        containerView = nil
    }

    deinit {
        destroy()
    }
}

// MARK: - GhosttyNSView

/// The NSView subclass that hosts a Ghostty Metal surface.
/// Forwards keyboard and mouse events to the Ghostty C API.
class GhosttyNSView: NSView {
    var surface: ghostty_surface_t?
    weak var terminalSurface: TerminalSurface?

    override var acceptsFirstResponder: Bool { true }

    override var canBecomeKeyView: Bool { true }

    private(set) var lastSyncedSize: NSSize = .zero

    /// Test accessor for lastSyncedSize
    var lastSyncedSizeForTesting: NSSize { lastSyncedSize }

    /// Test helper: set lastSyncedSize to simulate a previous sync
    func resetLastSyncedSizeForTesting(to size: NSSize) {
        lastSyncedSize = size
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        syncSurfaceSize()
    }

    override func removeFromSuperview() {
        super.removeFromSuperview()
        // Reset debounce so the next container gets a fresh sync
        lastSyncedSize = .zero
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let surface, let window else { return }
        let scale = Double(window.backingScaleFactor)
        ghostty_surface_set_content_scale(surface, scale, scale)
        syncSurfaceSize()
    }

    private func syncSurfaceSize() {
        let size = bounds.size
        guard size.width > 0, size.height > 0 else { return }
        guard size != lastSyncedSize else { return }
        lastSyncedSize = size

        guard let surface else { return }

        // Update content scale in case we moved to a different window/screen
        if let window {
            let scale = Double(window.backingScaleFactor)
            ghostty_surface_set_content_scale(surface, scale, scale)
        }

        let scale = window?.backingScaleFactor ?? 2.0
        ghostty_surface_set_size(surface, UInt32(size.width * scale), UInt32(size.height * scale))
        ghostty_surface_refresh(surface)
        needsDisplay = true

        // Resize tmux to match the new terminal grid dimensions
        terminalSurface?.refreshTmuxLayout()
    }

    override func becomeFirstResponder() -> Bool {
        if let surface {
            ghostty_surface_set_focus(surface, true)
        }
        return super.becomeFirstResponder()
    }

    override func resignFirstResponder() -> Bool {
        if let surface {
            ghostty_surface_set_focus(surface, false)
        }
        return super.resignFirstResponder()
    }

    // MARK: - Keyboard

    override func keyDown(with event: NSEvent) {
        guard let surface else { return }

        var keyInput = ghostty_input_key_s()
        keyInput.action = GHOSTTY_ACTION_PRESS
        keyInput.keycode = UInt32(event.keyCode)
        keyInput.mods = modsFromEvent(event)

        // Send text if available
        if let chars = event.characters, !chars.isEmpty {
            chars.withCString { cStr in
                keyInput.text = cStr
                _ = ghostty_surface_key(surface, keyInput)
            }
        } else {
            _ = ghostty_surface_key(surface, keyInput)
        }
    }

    override func keyUp(with event: NSEvent) {
        guard let surface else { return }
        var keyInput = ghostty_input_key_s()
        keyInput.action = GHOSTTY_ACTION_RELEASE
        keyInput.keycode = UInt32(event.keyCode)
        keyInput.mods = modsFromEvent(event)
        _ = ghostty_surface_key(surface, keyInput)
    }

    override func flagsChanged(with event: NSEvent) {
        guard let surface else { return }
        var keyInput = ghostty_input_key_s()
        keyInput.action = GHOSTTY_ACTION_PRESS  // Ghostty handles press/release internally for modifiers
        keyInput.keycode = UInt32(event.keyCode)
        keyInput.mods = modsFromEvent(event)
        _ = ghostty_surface_key(surface, keyInput)
    }

    // MARK: - Mouse

    override func mouseDown(with event: NSEvent) {
        guard let surface else { return }
        let pos = convert(event.locationInWindow, from: nil)
        ghostty_surface_mouse_pos(surface, pos.x, Double(bounds.height) - pos.y, modsFromEvent(event))
        _ = ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_LEFT, modsFromEvent(event))
    }

    override func mouseUp(with event: NSEvent) {
        guard let surface else { return }
        let pos = convert(event.locationInWindow, from: nil)
        ghostty_surface_mouse_pos(surface, pos.x, Double(bounds.height) - pos.y, modsFromEvent(event))
        _ = ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_LEFT, modsFromEvent(event))
    }

    override func mouseDragged(with event: NSEvent) {
        guard let surface else { return }
        let pos = convert(event.locationInWindow, from: nil)
        ghostty_surface_mouse_pos(surface, pos.x, Double(bounds.height) - pos.y, modsFromEvent(event))
    }

    override func mouseMoved(with event: NSEvent) {
        guard let surface else { return }
        let pos = convert(event.locationInWindow, from: nil)
        ghostty_surface_mouse_pos(surface, pos.x, Double(bounds.height) - pos.y, modsFromEvent(event))
    }

    override func scrollWheel(with event: NSEvent) {
        guard let surface else {
            super.scrollWheel(with: event)
            return
        }
        var scrollMods: ghostty_input_scroll_mods_t = 0
        if event.hasPreciseScrollingDeltas {
            scrollMods |= 1  // precision bit
        }
        // Send mouse position before scroll so Ghostty knows where the cursor is
        let pos = convert(event.locationInWindow, from: nil)
        ghostty_surface_mouse_pos(surface, pos.x, Double(bounds.height) - pos.y, modsFromEvent(event))
        ghostty_surface_mouse_scroll(surface, event.scrollingDeltaX, event.scrollingDeltaY, scrollMods)
    }

    override func rightMouseDown(with event: NSEvent) {
        guard let surface else { return }
        _ = ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_PRESS, GHOSTTY_MOUSE_RIGHT, modsFromEvent(event))
    }

    override func rightMouseUp(with event: NSEvent) {
        guard let surface else { return }
        _ = ghostty_surface_mouse_button(surface, GHOSTTY_MOUSE_RELEASE, GHOSTTY_MOUSE_RIGHT, modsFromEvent(event))
    }

    // MARK: - Tracking area for mouseMoved

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas {
            removeTrackingArea(area)
        }
        let area = NSTrackingArea(
            rect: bounds,
            options: [.mouseMoved, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
    }

    // MARK: - Helpers

    private func modsFromEvent(_ event: NSEvent) -> ghostty_input_mods_e {
        var mods = GHOSTTY_MODS_NONE.rawValue
        let flags = event.modifierFlags
        if flags.contains(.shift) { mods |= GHOSTTY_MODS_SHIFT.rawValue }
        if flags.contains(.control) { mods |= GHOSTTY_MODS_CTRL.rawValue }
        if flags.contains(.option) { mods |= GHOSTTY_MODS_ALT.rawValue }
        if flags.contains(.command) { mods |= GHOSTTY_MODS_SUPER.rawValue }
        if flags.contains(.capsLock) { mods |= GHOSTTY_MODS_CAPS.rawValue }
        return ghostty_input_mods_e(rawValue: mods)
    }
}
