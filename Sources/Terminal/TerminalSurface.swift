import AppKit

/// Manages a single Ghostty terminal surface (NSView + PTY + Metal renderer).
/// Each worktree gets one TerminalSurface instance.
class TerminalSurface {
    /// The NSView that Ghostty renders into (layer-backed, Metal)
    private(set) var view: GhosttyNSView!
    private(set) var surface: ghostty_surface_t?
    private weak var containerView: NSView?

    /// Session name for persistence backend (nil = direct shell)
    var sessionName: String?
    /// Persistence backend for the sessionName above.
    var backend: String = "zmx"

    /// Create the terminal surface and add it to the given container view.
    /// If sessionName is provided, the surface runs inside a persistent backend session.
    func create(in container: NSView, workingDirectory: String? = nil, sessionName: String? = nil) -> Bool {
        guard let app = GhosttyBridge.shared.app else {
            NSLog("GhosttyBridge not initialized")
            return false
        }

        if let sessionName {
            if backend == "tmux" {
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

            if backend == "zmx" {
                let zmxCommand = "zmx attach \(sessionName)"
                _createWithCommand(app: app, container: container, workingDirectory: workingDirectory, command: zmxCommand)
                return surface != nil
            }
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
                self?.refreshSessionLayout()
            }
        }
    }

    /// Tell tmux to resize its window to match the terminal's actual grid size.
    /// zmx handles size syncing automatically, so this is tmux-only behavior.
    func refreshSessionLayout() {
        guard backend == "tmux" else { return }
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

    /// Static version for when we don't have the surface (tmux fallback).
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
class GhosttyNSView: NSView, NSTextInputClient {
    var surface: ghostty_surface_t?
    weak var terminalSurface: TerminalSurface?
    private var markedText = NSMutableAttributedString()
    private var keyTextAccumulator: [String]?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        focusRingType = .none
        applyFocusVisualState(false)
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        wantsLayer = true
        focusRingType = .none
        applyFocusVisualState(false)
    }

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

        // Resize backend session layout if needed (tmux only)
        terminalSurface?.refreshSessionLayout()
    }

    override func becomeFirstResponder() -> Bool {
        applyFocusVisualState(true)
        if let surface {
            ghostty_surface_set_focus(surface, true)
        }
        return super.becomeFirstResponder()
    }

    override func resignFirstResponder() -> Bool {
        applyFocusVisualState(false)
        if let surface {
            ghostty_surface_set_focus(surface, false)
        }
        return super.resignFirstResponder()
    }

    private func applyFocusVisualState(_ focused: Bool) {
        guard let layer else { return }
        layer.masksToBounds = false
        layer.shadowOffset = .zero
        layer.shadowRadius = 5
        layer.shadowColor = NSColor.controlAccentColor.withAlphaComponent(0.45).cgColor
        layer.shadowOpacity = focused ? 0.22 : 0
    }

    // MARK: - Keyboard

    override func keyDown(with event: NSEvent) {
        guard let surface else { return }

        let action = event.isARepeat ? GHOSTTY_ACTION_REPEAT : GHOSTTY_ACTION_PRESS
        keyTextAccumulator = []
        defer { keyTextAccumulator = nil }

        let markedTextBefore = hasMarkedText()
        interpretKeyEvents([event])
        syncPreedit(clearIfNeeded: markedTextBefore)

        let accumulated = keyTextAccumulator ?? []
        if !accumulated.isEmpty {
            for text in accumulated {
                sendKey(surface: surface, action: action, event: event, text: text)
            }
            return
        }

        guard Self.shouldSendRawKey(
            markedTextBefore: markedTextBefore,
            hasMarkedTextNow: hasMarkedText(),
            hasAccumulatedText: false
        ) else {
            return
        }

        sendKey(surface: surface, action: action, event: event, text: nil)
    }

    override func doCommand(by selector: Selector) {
        if selector == #selector(NSText.paste(_:)) || selector == NSSelectorFromString("pasteAsPlainText:") {
            paste(nil)
            return
        }

        // Prevent AppKit from beeping for unhandled selector commands.
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return false }
        if Self.isPasteShortcut(event) {
            paste(nil)
            return true
        }
        if Self.shouldHandleControlKeyEquivalent(event) {
            keyDown(with: event)
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    @IBAction func paste(_ sender: Any?) {
        guard let surface else { return }
        let action = "paste_from_clipboard"
        action.withCString { cstr in
            _ = ghostty_surface_binding_action(surface, cstr, UInt(strlen(cstr)))
        }
    }

    @IBAction func pasteAsPlainText(_ sender: Any?) {
        paste(sender)
    }

    func insertText(_ string: Any, replacementRange: NSRange) {
        guard let surface else { return }
        let text: String
        switch string {
        case let attributed as NSAttributedString:
            text = attributed.string
        case let plain as String:
            text = plain
        default:
            return
        }
        guard !text.isEmpty else { return }

        unmarkText()

        if var accumulator = keyTextAccumulator {
            accumulator.append(text)
            keyTextAccumulator = accumulator
            return
        }

        sendKey(surface: surface, action: GHOSTTY_ACTION_PRESS, event: NSApp.currentEvent, text: text)
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
        if window?.firstResponder !== self {
            window?.makeFirstResponder(self)
        }

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

    private func sendKey(
        surface: ghostty_surface_t,
        action: ghostty_input_action_e,
        event: NSEvent?,
        text: String?
    ) {
        var keyInput = ghostty_input_key_s()
        keyInput.action = action
        if let event {
            keyInput.keycode = UInt32(event.keyCode)
            keyInput.mods = modsFromEvent(event)
        } else {
            keyInput.keycode = 0
            keyInput.mods = GHOSTTY_MODS_NONE
        }

        if let text, !text.isEmpty {
            text.withCString { cStr in
                keyInput.text = cStr
                _ = ghostty_surface_key(surface, keyInput)
            }
        } else {
            _ = ghostty_surface_key(surface, keyInput)
        }
    }

    static func shouldSendRawKey(
        markedTextBefore: Bool,
        hasMarkedTextNow: Bool,
        hasAccumulatedText: Bool
    ) -> Bool {
        if hasAccumulatedText { return false }
        if markedTextBefore || hasMarkedTextNow { return false }
        return true
    }

    static func isPasteShortcut(_ event: NSEvent) -> Bool {
        guard event.type == .keyDown,
              event.charactersIgnoringModifiers?.lowercased() == "v"
        else {
            return false
        }

        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard mods.contains(.command) else { return false }

        let disallowed: NSEvent.ModifierFlags = [.control, .option, .shift, .function]
        if !mods.isDisjoint(with: disallowed) {
            return false
        }

        return true
    }

    static func shouldHandleControlKeyEquivalent(_ event: NSEvent) -> Bool {
        guard event.type == .keyDown else { return false }
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        return mods.contains(.control) && !mods.contains(.command)
    }

    // MARK: - NSTextInputClient

    func hasMarkedText() -> Bool {
        markedText.length > 0
    }

    func markedRange() -> NSRange {
        guard markedText.length > 0 else { return NSRange(location: NSNotFound, length: 0) }
        return NSRange(location: 0, length: markedText.length)
    }

    func selectedRange() -> NSRange {
        NSRange(location: NSNotFound, length: 0)
    }

    func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        switch string {
        case let attributed as NSAttributedString:
            markedText = NSMutableAttributedString(attributedString: attributed)
        case let plain as String:
            markedText = NSMutableAttributedString(string: plain)
        default:
            markedText = NSMutableAttributedString()
        }

        if keyTextAccumulator == nil {
            syncPreedit(clearIfNeeded: true)
        }
    }

    func unmarkText() {
        if markedText.length > 0 {
            markedText = NSMutableAttributedString()
            syncPreedit(clearIfNeeded: true)
        }
    }

    func validAttributesForMarkedText() -> [NSAttributedString.Key] {
        []
    }

    func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        nil
    }

    func characterIndex(for point: NSPoint) -> Int {
        0
    }

    func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        guard let surface else { return .zero }
        var x: Double = 0
        var y: Double = 0
        var width: Double = 0
        var height: Double = 0
        ghostty_surface_ime_point(surface, &x, &y, &width, &height)

        let viewRect = NSRect(
            x: x,
            y: frame.size.height - y,
            width: width,
            height: max(height, 1)
        )
        let winRect = convert(viewRect, to: nil)
        guard let window else { return winRect }
        return window.convertToScreen(winRect)
    }

    private func syncPreedit(clearIfNeeded: Bool) {
        guard let surface else { return }

        if markedText.length > 0 {
            let string = markedText.string
            let utf8 = string.utf8CString
            guard !utf8.isEmpty else { return }
            string.withCString { ptr in
                ghostty_surface_preedit(surface, ptr, UInt(utf8.count - 1))
            }
        } else if clearIfNeeded {
            ghostty_surface_preedit(surface, nil, 0)
        }
    }
}
