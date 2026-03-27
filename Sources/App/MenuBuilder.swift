import AppKit

enum MenuBuilder {
    static func buildMainMenu(target: AnyObject? = nil) -> NSMenu {
        let mainMenu = NSMenu()

        // App menu
        let appMenuItem = NSMenuItem()
        let appMenu = NSMenu()
        let settingsItem = NSMenuItem(title: "Settings...", action: #selector(MainWindowController.showSettings), keyEquivalent: ",")
        settingsItem.keyEquivalentModifierMask = .command
        settingsItem.target = target
        appMenu.addItem(settingsItem)
        appMenu.addItem(NSMenuItem.separator())
        let checkUpdateItem = NSMenuItem(title: "Check for Updates...", action: #selector(MainWindowController.checkForUpdates), keyEquivalent: "u")
        checkUpdateItem.keyEquivalentModifierMask = .command
        checkUpdateItem.target = target
        appMenu.addItem(checkUpdateItem)
        appMenu.addItem(NSMenuItem.separator())
        appMenu.addItem(withTitle: "Quit amux", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appMenuItem.submenu = appMenu
        mainMenu.addItem(appMenuItem)

        // File menu
        let fileMenuItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        let newBranchItem = NSMenuItem(title: "New Branch...", action: #selector(MainWindowController.showNewBranchDialog), keyEquivalent: "n")
        newBranchItem.keyEquivalentModifierMask = .command
        newBranchItem.target = target
        fileMenu.addItem(newBranchItem)
        let quickSwitchItem = NSMenuItem(title: "Quick Switch...", action: #selector(MainWindowController.showQuickSwitcher), keyEquivalent: "p")
        quickSwitchItem.keyEquivalentModifierMask = .command
        quickSwitchItem.target = target
        fileMenu.addItem(quickSwitchItem)
        fileMenuItem.submenu = fileMenu
        mainMenu.addItem(fileMenuItem)

        // Edit menu (standard Cut/Copy/Paste/Undo/Redo)
        let editMenuItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redoItem = NSMenuItem(title: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redoItem.keyEquivalentModifierMask = [.command, .shift]
        editMenu.addItem(redoItem)
        editMenu.addItem(NSMenuItem.separator())
        editMenu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editMenuItem.submenu = editMenu
        mainMenu.addItem(editMenuItem)

        // View menu
        let viewMenuItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")

        let dashItem = NSMenuItem(title: "Dashboard", action: #selector(MainWindowController.switchToDashboard), keyEquivalent: "0")
        dashItem.keyEquivalentModifierMask = .command
        dashItem.target = target
        viewMenu.addItem(dashItem)

        let closePaneItem = NSMenuItem(title: "Close Pane", action: #selector(MainWindowController.closePaneOrTab), keyEquivalent: "w")
        closePaneItem.keyEquivalentModifierMask = .command
        closePaneItem.target = target
        viewMenu.addItem(closePaneItem)

        let diffItem = NSMenuItem(title: "Show Diff...", action: #selector(MainWindowController.showDiffOverlay), keyEquivalent: "")
        diffItem.target = target
        viewMenu.addItem(diffItem)

        viewMenu.addItem(NSMenuItem.separator())

        let zoomInItem = NSMenuItem(title: "Zoom In (Smaller Cards)", action: #selector(MainWindowController.dashboardZoomIn), keyEquivalent: "-")
        zoomInItem.keyEquivalentModifierMask = .command
        zoomInItem.target = target
        viewMenu.addItem(zoomInItem)

        let zoomOutItem = NSMenuItem(title: "Zoom Out (Larger Cards)", action: #selector(MainWindowController.dashboardZoomOut), keyEquivalent: "=")
        zoomOutItem.keyEquivalentModifierMask = .command
        zoomOutItem.target = target
        viewMenu.addItem(zoomOutItem)

        viewMenuItem.submenu = viewMenu
        mainMenu.addItem(viewMenuItem)

        // Window menu (standard macOS window management)
        let windowMenuItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowMenu.addItem(withTitle: "Minimize", action: #selector(NSWindow.miniaturize(_:)), keyEquivalent: "m")
        windowMenu.addItem(withTitle: "Zoom", action: #selector(NSWindow.zoom(_:)), keyEquivalent: "")
        windowMenu.addItem(NSMenuItem.separator())
        let nextTabItem = NSMenuItem(title: "Next Tab", action: #selector(MainWindowController.selectNextTab), keyEquivalent: "}")
        nextTabItem.keyEquivalentModifierMask = .command
        nextTabItem.target = target
        windowMenu.addItem(nextTabItem)
        let prevTabItem = NSMenuItem(title: "Previous Tab", action: #selector(MainWindowController.selectPreviousTab), keyEquivalent: "{")
        prevTabItem.keyEquivalentModifierMask = .command
        prevTabItem.target = target
        windowMenu.addItem(prevTabItem)
        windowMenu.addItem(NSMenuItem.separator())
        windowMenu.addItem(withTitle: "Bring All to Front", action: #selector(NSApplication.arrangeInFront(_:)), keyEquivalent: "")
        windowMenuItem.submenu = windowMenu
        mainMenu.addItem(windowMenuItem)
        NSApp.windowsMenu = windowMenu

        // Help menu
        let helpMenuItem = NSMenuItem()
        let helpMenu = NSMenu(title: "Help")
        let keyboardShortcutsItem = NSMenuItem(title: "Keyboard Shortcuts", action: #selector(MainWindowController.showKeyboardShortcuts), keyEquivalent: "")
        keyboardShortcutsItem.target = target
        helpMenu.addItem(keyboardShortcutsItem)
        helpMenu.addItem(NSMenuItem.separator())
        let docsItem = NSMenuItem(title: "amux Documentation", action: #selector(MainWindowController.openDocumentation), keyEquivalent: "")
        docsItem.target = target
        helpMenu.addItem(docsItem)
        helpMenuItem.submenu = helpMenu
        mainMenu.addItem(helpMenuItem)
        NSApp.helpMenu = helpMenu

        return mainMenu
    }
}
