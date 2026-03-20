import AppKit

class AppDelegate: NSObject, NSApplicationDelegate {
    private var mainWindowController: MainWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Force dark appearance globally BEFORE any views are created
        // This ensures all NSColor(name:) dynamic colors resolve as dark mode
        let themeMode = Config.load().themeMode
        ThemeMode.applyAppearance(ThemeMode(rawValue: themeMode) ?? .dark)

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
