import XCTest

/// Page object for the layout selection popover (triggered from view menu).
final class LayoutPopoverPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var popover: XCUIElement { app.groups["layout.popover"] }
    var gridItem: XCUIElement { app.buttons["layout.item.grid"] }
    var leftRightItem: XCUIElement { app.buttons["layout.item.left-right"] }
    var topSmallItem: XCUIElement { app.buttons["layout.item.top-small"] }
    var topLargeItem: XCUIElement { app.buttons["layout.item.top-large"] }

    func selectGrid() { gridItem.waitAndClick() }
    func selectLeftRight() { leftRightItem.waitAndClick() }
    func selectTopSmall() { topSmallItem.waitAndClick() }
    func selectTopLarge() { topLargeItem.waitAndClick() }
}
