import XCTest

/// Base test class for all pmux UI tests.
/// Handles app launch/teardown and screenshot capture on failure.
class PmuxUITestCase: XCTestCase {
    var page: AppPage!

    override func setUp() {
        super.setUp()
        continueAfterFailure = false
        page = AppPage().launch()
    }

    override func tearDown() {
        if testRun?.failureCount ?? 0 > 0 {
            let screenshot = XCUIScreen.main.screenshot()
            let attachment = XCTAttachment(screenshot: screenshot)
            attachment.lifetime = .keepAlways
            add(attachment)
        }
        page.terminate()
        super.tearDown()
    }
}
