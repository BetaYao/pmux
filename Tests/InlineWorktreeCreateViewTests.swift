import XCTest
@testable import amux

final class InlineWorktreeCreateViewTests: XCTestCase {
    func testSubmitInvokesCallbackWithValues() {
        let view = InlineWorktreeCreateView()
        view.configure(repoPaths: ["/Users/me/repoA", "/Users/me/repoB"])
        var captured: (String, String, Bool)?
        view.onCreate = { name, repo, reuse in captured = (name, repo, reuse) }

        view.setNameForTesting("feature-x")
        view.setReuseEnvForTesting(true)
        view.submitForTesting()

        XCTAssertEqual(captured?.0, "feature-x")
        XCTAssertEqual(captured?.1, "/Users/me/repoA")
        XCTAssertEqual(captured?.2, true)
    }

    func testBlankNameDoesNotSubmit() {
        let view = InlineWorktreeCreateView()
        view.configure(repoPaths: ["/r"])
        var called = false
        view.onCreate = { _, _, _ in called = true }
        view.setNameForTesting("   ")
        view.submitForTesting()
        XCTAssertFalse(called)
    }

    func testExpandedStateTogglesOnFocus() {
        let view = InlineWorktreeCreateView()
        view.configure(repoPaths: ["/r"])
        XCTAssertFalse(view.isExpandedForTesting)
        view.setExpandedForTesting(true)
        XCTAssertTrue(view.isExpandedForTesting)
    }
}
