import AppKit

/// Singleton wrapper for the Ghostty application instance.
/// Manages global Ghostty lifecycle and runtime callbacks.
class GhosttyBridge {
    static let shared = GhosttyBridge()

    private(set) var app: ghostty_app_t?
    private var isInitialized = false

    private init() {}

    func initialize() {
        guard !isInitialized else { return }

        // Initialize Ghostty runtime
        let argc = CommandLine.argc
        let argv = CommandLine.unsafeArgv
        let result = ghostty_init(UInt(argc), argv)
        guard result == GHOSTTY_SUCCESS else {
            NSLog("Failed to initialize Ghostty: \(result)")
            return
        }

        // Create and configure config
        guard let config = ghostty_config_new() else {
            NSLog("Failed to create Ghostty config")
            return
        }
        ghostty_config_load_default_files(config)
        ghostty_config_finalize(config)

        // Always free config — ghostty_app_new copies what it needs
        defer { ghostty_config_free(config) }

        // Set up runtime callbacks
        var runtimeConfig = ghostty_runtime_config_s()
        runtimeConfig.userdata = Unmanaged.passUnretained(self).toOpaque()
        runtimeConfig.supports_selection_clipboard = false
        runtimeConfig.wakeup_cb = { userData in
            guard let userData else { return }
            let bridge = Unmanaged<GhosttyBridge>.fromOpaque(userData).takeUnretainedValue()
            DispatchQueue.main.async {
                bridge.tick()
            }
        }
        runtimeConfig.action_cb = { app, target, action in
            return GhosttyBridge.handleAction(app: app, target: target, action: action)
        }
        runtimeConfig.read_clipboard_cb = { userData, clipboard, state in
            GhosttyBridge.readClipboard(userData: userData, clipboard: clipboard, state: state)
        }
        runtimeConfig.confirm_read_clipboard_cb = { userData, text, state, request in
            // Auto-confirm clipboard reads
            guard let userData, let state else { return }
            // Find the surface for this clipboard state — for now just confirm
            // The state is passed back to complete_clipboard_request
        }
        runtimeConfig.write_clipboard_cb = { userData, clipboard, content, contentLen, confirm in
            guard let content, contentLen > 0 else { return }
            let item = content.pointee
            if let data = item.data {
                let str = String(cString: data)
                let pasteboard = NSPasteboard.general
                pasteboard.clearContents()
                pasteboard.setString(str, forType: .string)
            }
        }
        runtimeConfig.close_surface_cb = { userData, processAlive in
            NotificationCenter.default.post(name: .ghosttySurfaceCloseRequested, object: nil)
        }

        // Create the app
        guard let ghosttyApp = ghostty_app_new(&runtimeConfig, config) else {
            NSLog("Failed to create Ghostty app")
            return
        }

        self.app = ghosttyApp
        self.isInitialized = true

        NSLog("Ghostty initialized successfully")
    }

    func tick() {
        guard let app else { return }
        ghostty_app_tick(app)
    }

    func shutdown() {
        if let app {
            ghostty_app_free(app)
            self.app = nil
        }
        isInitialized = false
    }

    // MARK: - Static callback helpers

    private static func handleAction(app: ghostty_app_t?, target: ghostty_target_s, action: ghostty_action_s) -> Bool {
        switch action.tag {
        case GHOSTTY_ACTION_SET_TITLE:
            return true
        case GHOSTTY_ACTION_DESKTOP_NOTIFICATION:
            return true
        case GHOSTTY_ACTION_CLOSE_ALL_WINDOWS,
             GHOSTTY_ACTION_CLOSE_TAB,
             GHOSTTY_ACTION_CLOSE_WINDOW:
            return true
        case GHOSTTY_ACTION_START_SEARCH:
            // Ghostty requests search UI — we handle this ourselves
            return true
        case GHOSTTY_ACTION_SEARCH_TOTAL:
            let total = action.action.search_total.total
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .ghosttySearchTotal,
                    object: nil,
                    userInfo: ["total": Int(total)]
                )
            }
            return true
        case GHOSTTY_ACTION_SEARCH_SELECTED:
            let selected = action.action.search_selected.selected
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .ghosttySearchSelected,
                    object: nil,
                    userInfo: ["selected": Int(selected)]
                )
            }
            return true
        default:
            return false
        }
    }

    private static func readClipboard(userData: UnsafeMutableRawPointer?, clipboard: ghostty_clipboard_e, state: UnsafeMutableRawPointer?) {
        // For clipboard reads, we need to complete the request with the clipboard content
        // The state pointer needs to be passed back to ghostty_surface_complete_clipboard_request
        // For now, we don't have a clean way to get the surface, so skip clipboard reads
    }

    private static func writeClipboard(content: UnsafePointer<ghostty_clipboard_content_s>?, count: Int, confirm: Bool) {
        guard let content, count > 0 else { return }
        let item = content.pointee
        if let data = item.data {
            let str = String(cString: data)
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            pasteboard.setString(str, forType: .string)
        }
    }
}

// MARK: - Notifications

extension Notification.Name {
    static let ghosttySurfaceCloseRequested = Notification.Name("ghosttySurfaceCloseRequested")
    static let ghosttySearchTotal = Notification.Name("ghosttySearchTotal")
    static let ghosttySearchSelected = Notification.Name("ghosttySearchSelected")
}
