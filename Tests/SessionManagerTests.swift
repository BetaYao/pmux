import XCTest
@testable import amux

class SessionManagerTests: XCTestCase {
    func testPersistentSessionNameSanitizesDots() {
        let name = SessionManager.persistentSessionName(for: "/Users/test/repos/my.project/feature-1")
        XCTAssertFalse(name.contains("."))
        XCTAssertTrue(name.hasPrefix("amux-"))
    }

    func testPersistentSessionNameSanitizesColons() {
        let name = SessionManager.persistentSessionName(for: "/Users/test/repo:name/branch")
        XCTAssertFalse(name.contains(":"))
    }

    func testPersistentSessionNameFormat() {
        let name = SessionManager.persistentSessionName(for: "/Users/test/myrepo/feature-branch")
        XCTAssertEqual(name, "amux-myrepo-feature-branch")
    }

    func testSessionNameWithNestedPath() {
        let name = SessionManager.persistentSessionName(for: "/home/user/workspace/org/repo/feature")
        XCTAssertEqual(name, "amux-repo-feature")
    }

    func testLongSessionNameIsTruncatedWithHash() {
        let name = SessionManager.persistentSessionName(for: "/Volumes/openbeta/workspace/pmux-swift/pmux-swift-dashboard-consolidation")
        XCTAssertTrue(name.count <= 40, "Session name '\(name)' exceeds 40 chars (\(name.count))")
        XCTAssertTrue(name.hasPrefix("amux-"))
    }

    func testTruncatedSessionNameIsDeterministic() {
        let path = "/Volumes/openbeta/workspace/pmux-swift/pmux-swift-dashboard-consolidation"
        let a = SessionManager.persistentSessionName(for: path)
        let b = SessionManager.persistentSessionName(for: path)
        XCTAssertEqual(a, b)
    }

    func testDifferentLongPathsProduceDifferentNames() {
        let a = SessionManager.persistentSessionName(for: "/workspace/very-long-repo-name-here/very-long-branch-name-alpha")
        let b = SessionManager.persistentSessionName(for: "/workspace/very-long-repo-name-here/very-long-branch-name-beta")
        XCTAssertNotEqual(a, b)
    }
}
