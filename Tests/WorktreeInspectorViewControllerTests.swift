import XCTest
@testable import seamux

final class WorktreeInspectorViewControllerTests: XCTestCase {
    func testYaziCommandUsesResolvedPathAndHiddenConfig() throws {
        let configDir = try makeTempDirectory()

        let command = try XCTUnwrap(WorktreeInspectorViewController.yaziCommand(
            yaziPath: "/opt/homebrew/bin/yazi",
            configDirectory: configDir
        ))

        XCTAssertEqual(
            command,
            "/usr/bin/env YAZI_CONFIG_HOME='\(configDir.path)' '/opt/homebrew/bin/yazi' ."
        )

        let configURL = configDir.appendingPathComponent("yazi.toml")
        let config = try String(contentsOf: configURL, encoding: .utf8)
        XCTAssertTrue(config.contains("[mgr]"))
        XCTAssertTrue(config.contains("show_hidden = true"))
    }

    func testInspectorShowsCloseButton() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziAvailability: { false }
        )

        vc.loadViewIfNeeded()

        let closeButton = vc.view.viewWithAccessibilityIdentifier("worktreeInspector.closeButton") as? NSButton
        XCTAssertEqual(closeButton?.title, "Close")
        XCTAssertTrue(closeButton?.target === vc)
        XCTAssertEqual(closeButton?.action, NSSelectorFromString("closeClicked"))
    }

    func testInitialTabFilesSelectsFilesSegment() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziAvailability: { false }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .files)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.filesMissingYazi"))
    }

    func testInitialTabChangesSelectsChangesSegment() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .changes,
            yaziAvailability: { false },
            makeDiffReviewView: { path in
                DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            }
        )

        vc.loadViewIfNeeded()

        XCTAssertEqual(vc.selectedTabForTesting, .changes)
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("diffReview"))
    }

    func testChangesTabEmbedsDiffReviewView() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .changes,
            yaziAvailability: { false },
            makeDiffReviewView: { path in
                XCTAssertEqual(path, "/repo/project")
                return DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            }
        )

        vc.loadViewIfNeeded()

        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("diffReview"))
    }

    func testFilesTabShowsYaziContainerWhenAvailable() {
        var capturedPath: String?
        var capturedCommand: String?
        let configDir = try! makeTempDirectory()

        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziPathProvider: { "/opt/homebrew/bin/yazi" },
            yaziConfigDirectoryProvider: { configDir },
            makeDiffReviewView: { path in
                DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            },
            createYaziSurface: { _, path, command in
                capturedPath = path
                capturedCommand = command
                return true
            }
        )

        vc.loadViewIfNeeded()

        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.yaziContainer"))
        XCTAssertEqual(capturedPath, "/repo/project")
        XCTAssertEqual(
            capturedCommand,
            "/usr/bin/env YAZI_CONFIG_HOME='\(configDir.path)' '/opt/homebrew/bin/yazi' ."
        )
    }

    func testFilesTabShowsMissingYaziWhenPathCannotBeResolved() {
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziPathProvider: { nil }
        )

        vc.loadViewIfNeeded()

        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.filesMissingYazi"))
    }

    func testFilesTabShowsFailureMessageWhenYaziSurfaceCannotStart() {
        let configDir = try! makeTempDirectory()
        let vc = WorktreeInspectorViewController(
            worktreePath: "/repo/project",
            initialTab: .files,
            yaziPathProvider: { "/opt/homebrew/bin/yazi" },
            yaziConfigDirectoryProvider: { configDir },
            makeDiffReviewView: { path in
                DiffReviewView(
                    worktreePath: path,
                    loadSnapshot: { GitDiffSnapshot(changedFiles: [], files: []) }
                )
            },
            createYaziSurface: { _, _, _ in false }
        )

        vc.loadViewIfNeeded()

        XCTAssertNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.yaziContainer"))
        XCTAssertNotNil(vc.view.viewWithAccessibilityIdentifier("worktreeInspector.filesYaziFailed"))
    }

    private func makeTempDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("amux-yazi-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}

private extension NSView {
    func viewWithAccessibilityIdentifier(_ identifier: String) -> NSView? {
        if accessibilityIdentifier() == identifier { return self }
        for subview in subviews {
            if let found = subview.viewWithAccessibilityIdentifier(identifier) {
                return found
            }
        }
        return nil
    }
}
