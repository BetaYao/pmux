import XCTest

/// Page object for the AI assistant side panel.
final class AIPanelPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var panel: XCUIElement { app.groups["panel.ai"] }
    var closeButton: XCUIElement { app.buttons["panel.ai.close"] }
    var inputField: XCUIElement { app.textFields["panel.ai.input"] }
    var sendButton: XCUIElement { app.buttons["panel.ai.send"] }
    var messages: XCUIElement { app.groups["panel.ai.messages"] }

    var isOpen: Bool { panel.waitForExistence(timeout: 2) }

    func sendMessage(_ text: String) {
        inputField.waitAndClick()
        inputField.typeText(text)
        sendButton.waitAndClick()
    }
    func close() { closeButton.waitAndClick() }
}
