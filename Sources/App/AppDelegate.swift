import AppKit

class AppDelegate: NSObject, NSApplicationDelegate {
    private(set) var mainWindowController: MainWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Force dark appearance globally BEFORE any views are created.
        // Must set BOTH NSApp.appearance AND NSAppearance.current so that
        // NSColor(name:) dynamic colors resolve correctly even for views
        // not yet added to a window (e.g. during init/setup).
        let config = Config.load()
        let mode = ThemeMode(rawValue: config.themeMode) ?? .dark
        ThemeMode.applyAppearance(mode)

        // Ensure Claude Code hooks are configured
        if config.webhook.enabled {
            ClaudeHooksSetup.ensureHooksConfigured(port: config.webhook.port)
        }
        NSAppearance.current = NSApp.effectiveAppearance

        // Load TODO and Ideas stores
        TodoStore.shared.load()
        IdeaStore.shared.load()

        // Initialize GhosttyApp singleton
        GhosttyBridge.shared.initialize()

        // Create and show main window
        mainWindowController = MainWindowController()
        mainWindowController?.showWindow(nil)
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        // Prevent macOS from creating a new window on reactivation (e.g. notification click)
        if let window = mainWindowController?.window {
            window.deminiaturize(nil)
            window.makeKeyAndOrderFront(nil)
        }
        return false
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }

    func applicationWillTerminate(_ notification: Notification) {
        mainWindowController?.cleanupBeforeTermination()
        GhosttyBridge.shared.shutdown()
    }
}
