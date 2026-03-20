import AppKit

class AppDelegate: NSObject, NSApplicationDelegate {
    private var mainWindowController: MainWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Force dark appearance globally BEFORE any views are created.
        // Must set BOTH NSApp.appearance AND NSAppearance.current so that
        // NSColor(name:) dynamic colors resolve correctly even for views
        // not yet added to a window (e.g. during init/setup).
        let themeMode = Config.load().themeMode
        let mode = ThemeMode(rawValue: themeMode) ?? .dark
        ThemeMode.applyAppearance(mode)
        if let appearance = NSApp.appearance {
            NSAppearance.current = appearance
        }

        // Initialize GhosttyApp singleton
        GhosttyBridge.shared.initialize()

        // Create and show main window
        mainWindowController = MainWindowController()
        mainWindowController?.showWindow(nil)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }

    func applicationWillTerminate(_ notification: Notification) {
        GhosttyBridge.shared.shutdown()
    }
}
