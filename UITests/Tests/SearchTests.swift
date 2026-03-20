import XCTest

/// Terminal search bar tests (Cmd+F).
class SearchTests: PmuxUITestCase {

    private func navigateToRepo() -> Bool {
        let repoTabs = page.tabBar.repoTabs
        guard repoTabs.firstMatch.waitForExistence(timeout: 5) else { return false }
        repoTabs.firstMatch.click()
        return page.sidebar.worktreeList.waitForExistence(timeout: 10)
    }

    func testCmdFOpensSearchBar() {
        guard navigateToRepo() else { return }

        page.app.typeKey("f", modifierFlags: .command)
        let searchBar = page.app.groups["search.bar"]
        XCTAssertTrue(searchBar.waitForExistence(timeout: 5),
                      "Cmd+F should open search bar")
    }

    func testEscClosesSearchBar() {
        guard navigateToRepo() else { return }

        page.app.typeKey("f", modifierFlags: .command)
        let searchBar = page.app.groups["search.bar"]
        guard searchBar.waitForExistence(timeout: 5) else {
            XCTFail("Search bar should appear")
            return
        }

        page.app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(searchBar.waitForNonExistence(timeout: 3),
                      "Esc should close search bar")
    }

    func testSearchFieldExists() {
        guard navigateToRepo() else { return }

        page.app.typeKey("f", modifierFlags: .command)
        let searchField = page.app.textFields["search.field"]
        XCTAssertTrue(searchField.waitForExistence(timeout: 5),
                      "Search field should be accessible")
    }

    func testCmdFTogglesSearchBar() {
        guard navigateToRepo() else { return }

        // Open
        page.app.typeKey("f", modifierFlags: .command)
        let searchBar = page.app.groups["search.bar"]
        XCTAssertTrue(searchBar.waitForExistence(timeout: 5))

        // Toggle off
        page.app.typeKey("f", modifierFlags: .command)
        XCTAssertTrue(searchBar.waitForNonExistence(timeout: 3),
                      "Cmd+F again should close search bar")
    }
}
