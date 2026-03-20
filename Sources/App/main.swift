import AppKit

let app = NSApplication.shared

// Force appearance BEFORE anything else — must happen before any views are created.
// This is the earliest possible point in the app lifecycle.
let themeMode = Config.load().themeMode
switch themeMode {
case "dark":
    app.appearance = NSAppearance(named: .darkAqua)
case "light":
    app.appearance = NSAppearance(named: .aqua)
default:
    app.appearance = NSAppearance(named: .darkAqua) // default to dark
}
NSAppearance.current = app.appearance ?? NSAppearance(named: .darkAqua)!

let delegate = AppDelegate()
app.delegate = delegate
app.run()
