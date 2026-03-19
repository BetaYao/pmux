import AppKit

class AppDelegate: NSObject, NSApplicationDelegate {
    private var mainWindowController: MainWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
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
