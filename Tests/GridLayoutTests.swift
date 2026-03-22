import XCTest
import AppKit
@testable import pmux

final class GridLayoutTests: XCTestCase {

    // MARK: - Helper

    private func makeLayout(
        width: CGFloat = 900,
        height: CGFloat = 600,
        cardCount: Int = 6,
        minCardWidth: CGFloat = 300,
        spacing: CGFloat = 12,
        aspectRatio: CGFloat = 0.6
    ) -> GridLayout {
        GridLayout(
            availableWidth: width,
            availableHeight: height,
            cardCount: cardCount,
            minCardWidth: minCardWidth,
            spacing: spacing,
            aspectRatio: aspectRatio
        )
    }

    // MARK: - Columns

    func testColumns_ExactFit() {
        let layout = makeLayout(width: 900, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 3)
    }

    func testColumns_PartialFit() {
        let layout = makeLayout(width: 700, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 2)
    }

    func testColumns_NarrowWidth_AtLeastOne() {
        let layout = makeLayout(width: 100, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 1)
    }

    func testColumns_WideWidth() {
        let layout = makeLayout(width: 1800, minCardWidth: 300)
        XCTAssertEqual(layout.columns, 6)
    }

    // MARK: - Rows

    func testRows_ExactDivision() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300)
        // 3 columns, 6 cards = 2 rows
        XCTAssertEqual(layout.rows, 2)
    }

    func testRows_PartialRow() {
        let layout = makeLayout(width: 900, cardCount: 7, minCardWidth: 300)
        // 3 columns, 7 cards = 3 rows
        XCTAssertEqual(layout.rows, 3)
    }

    func testRows_ZeroCards() {
        let layout = makeLayout(cardCount: 0)
        XCTAssertEqual(layout.rows, 0)
    }

    func testRows_SingleCard() {
        let layout = makeLayout(width: 900, cardCount: 1, minCardWidth: 300)
        XCTAssertEqual(layout.rows, 1)
    }

    // MARK: - Card Dimensions

    func testCardWidth_FillsSpace() {
        let layout = makeLayout(width: 900, minCardWidth: 300, spacing: 12)
        // 3 columns: (900 - 12*2) / 3 = 876/3 = 292
        XCTAssertEqual(layout.cardWidth, 292, accuracy: 0.01)
    }

    func testCardWidth_SingleColumn() {
        let layout = makeLayout(width: 200, minCardWidth: 300, spacing: 12)
        // 1 column: (200 - 0) / 1 = 200
        XCTAssertEqual(layout.cardWidth, 200, accuracy: 0.01)
    }

    func testCardHeight_AspectRatio() {
        let layout = makeLayout(width: 900, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let expectedWidth: CGFloat = 292
        XCTAssertEqual(layout.cardHeight, expectedWidth * 0.6, accuracy: 0.01)
    }

    // MARK: - Total Height

    func testTotalHeight_TwoRows() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // 2 rows, cardHeight = 292*0.6 = 175.2
        // totalHeight = 2*175.2 + 1*12 = 362.4
        let expectedCardHeight = 292.0 * 0.6
        let expected = 2 * expectedCardHeight + 12
        XCTAssertEqual(layout.totalHeight, expected, accuracy: 0.01)
    }

    func testTotalHeight_ZeroCards() {
        let layout = makeLayout(cardCount: 0)
        XCTAssertEqual(layout.totalHeight, 0)
    }

    // MARK: - Scroll Content Height

    func testScrollContentHeight_AtLeastAvailableHeight() {
        let layout = makeLayout(width: 900, height: 800, cardCount: 1, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // Single row, totalHeight small, should return availableHeight
        XCTAssertEqual(layout.scrollContentHeight, 800, accuracy: 0.01)
    }

    func testScrollContentHeight_LargeContent() {
        let layout = makeLayout(width: 900, height: 200, cardCount: 30, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // 10 rows, totalHeight >> 200
        XCTAssertGreaterThan(layout.scrollContentHeight, 200)
        XCTAssertEqual(layout.scrollContentHeight, layout.totalHeight)
    }

    // MARK: - Card Frame

    func testCardFrame_FirstCard() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.cardFrame(at: 0)
        // First card: col=0, row=0
        XCTAssertEqual(frame.origin.x, 0, accuracy: 0.01)
        XCTAssertEqual(frame.size.width, layout.cardWidth, accuracy: 0.01)
        XCTAssertEqual(frame.size.height, layout.cardHeight, accuracy: 0.01)
    }

    func testCardFrame_SecondCard() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.cardFrame(at: 1)
        // Second card: col=1, row=0
        let expectedX = 1 * (layout.cardWidth + 12)
        XCTAssertEqual(frame.origin.x, expectedX, accuracy: 0.01)
    }

    func testCardFrame_SecondRow() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame0 = layout.cardFrame(at: 0)
        let frame3 = layout.cardFrame(at: 3)
        // Card 3: col=0, row=1 — should be below row 0
        XCTAssertEqual(frame3.origin.x, 0, accuracy: 0.01)
        XCTAssertLessThan(frame3.origin.y, frame0.origin.y)
    }

    func testCardFrame_SingleCardAnchoredToTop() {
        let layout = makeLayout(width: 900, height: 800, cardCount: 1, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.cardFrame(at: 0)
        XCTAssertEqual(frame.origin.y, layout.scrollContentHeight - layout.cardHeight, accuracy: 0.01)
    }

    func testCardFrame_OutOfBounds() {
        let layout = makeLayout(cardCount: 3)
        XCTAssertEqual(layout.cardFrame(at: -1), .zero)
        XCTAssertEqual(layout.cardFrame(at: 3), .zero)
    }

    // MARK: - Grid Index from Point

    func testGridIndex_TopLeftCorner() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // Top-left of first card (top row in unflipped document coords)
        let topY = layout.scrollContentHeight - 1
        let index = layout.gridIndex(for: CGPoint(x: 10, y: topY))
        XCTAssertEqual(index, 0)
    }

    func testGridIndex_BottomRow() {
        let layout = makeLayout(width: 900, cardCount: 6, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        // Bottom row (low y) = row 1
        let index = layout.gridIndex(for: CGPoint(x: 10, y: 10))
        XCTAssertEqual(index, 3)
    }

    func testGridIndex_EmptyGrid() {
        let layout = makeLayout(cardCount: 0)
        XCTAssertEqual(layout.gridIndex(for: CGPoint(x: 100, y: 100)), 0)
    }

    // MARK: - Drop Indicator

    func testDropIndicator_FirstPosition() {
        let layout = makeLayout(width: 900, cardCount: 3, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.dropIndicatorFrame(at: 0)
        XCTAssertEqual(frame.origin.x, 0, accuracy: 0.01)  // clamped to 0 from -3
        XCTAssertEqual(frame.size.width, 4)
        XCTAssertEqual(frame.size.height, layout.cardHeight, accuracy: 0.01)
    }

    func testDropIndicator_MiddlePosition() {
        let layout = makeLayout(width: 900, cardCount: 3, minCardWidth: 300, spacing: 12, aspectRatio: 0.6)
        let frame = layout.dropIndicatorFrame(at: 1)
        let expectedX = 1 * (layout.cardWidth + 12) - 3
        XCTAssertEqual(frame.origin.x, expectedX, accuracy: 0.01)
    }

    // MARK: - Zoom Levels

    func testZoomLevels_Count() {
        XCTAssertEqual(GridLayout.zoomLevels.count, 6)
    }

    func testZoomLevels_DefaultIndex() {
        XCTAssertEqual(GridLayout.defaultZoomIndex, 3)
        XCTAssertEqual(GridLayout.zoomLevels[3], 300)
    }

    func testClampZoomIndex_InRange() {
        XCTAssertEqual(GridLayout.clampZoomIndex(0), 0)
        XCTAssertEqual(GridLayout.clampZoomIndex(3), 3)
        XCTAssertEqual(GridLayout.clampZoomIndex(5), 5)
    }

    func testClampZoomIndex_OutOfRange() {
        XCTAssertEqual(GridLayout.clampZoomIndex(-1), 0)
        XCTAssertEqual(GridLayout.clampZoomIndex(-10), 0)
        XCTAssertEqual(GridLayout.clampZoomIndex(6), 5)
        XCTAssertEqual(GridLayout.clampZoomIndex(100), 5)
    }

    // MARK: - Zoom changes columns

    func testZoom_SmallerMinWidth_MoreColumns() {
        let small = makeLayout(width: 900, minCardWidth: 180)  // zoom level 0
        let large = makeLayout(width: 900, minCardWidth: 480)  // zoom level 5
        XCTAssertGreaterThan(small.columns, large.columns)
    }

    func testZoom_AllLevelsProduceValidLayout() {
        for level in GridLayout.zoomLevels {
            let layout = makeLayout(width: 1200, cardCount: 10, minCardWidth: level)
            XCTAssertGreaterThanOrEqual(layout.columns, 1)
            XCTAssertGreaterThanOrEqual(layout.rows, 1)
            XCTAssertGreaterThan(layout.cardWidth, 0)
            XCTAssertGreaterThan(layout.cardHeight, 0)
        }
    }

    func testTitleBar_InstallsHoverTrackingArea() {
        let titleBar = TitleBarView(frame: NSRect(x: 0, y: 0, width: 900, height: 48))
        titleBar.updateTrackingAreas()
        XCTAssertGreaterThan(titleBar.trackingAreas.count, 0)
    }

    func testGlassBackgroundConfig_DarkModeEnabled() {
        let config = MainWindowController.glassBackgroundConfig(isDark: true)
        XCTAssertTrue(config.enabled)
        XCTAssertEqual(config.material, .hudWindow)
        XCTAssertEqual(config.blendingMode, .behindWindow)
    }

    func testGlassBackgroundConfig_LightModeEnabled() {
        let config = MainWindowController.glassBackgroundConfig(isDark: false)
        XCTAssertTrue(config.enabled)
        XCTAssertEqual(config.material, .underWindowBackground)
        XCTAssertEqual(config.blendingMode, .behindWindow)
    }

    func testIntegrateDiscoveredRepoForTesting_RegistersFallbackAgentWhenNoWorktrees() {
        let controller = MainWindowController()
        let repoPath = "/tmp/pmux-test-\(UUID().uuidString)"

        _ = controller.integrateDiscoveredRepoForTesting(repoPath: repoPath, worktrees: [], activateTab: false)

        let agent = AgentHead.shared.agent(for: repoPath)
        XCTAssertNotNil(agent)

        AgentHead.shared.unregister(terminalID: agent?.id ?? "")
    }

    func testDashboardTypographyBaselines_AreMacReadable() {
        XCTAssertEqual(AgentCardView.Typography.primaryPointSize, 13)
        XCTAssertEqual(AgentCardView.Typography.bodyPointSize, 12)
        XCTAssertEqual(AgentCardView.Typography.secondaryPointSize, 11)

        XCTAssertEqual(MiniCardView.Typography.primaryPointSize, 13)
        XCTAssertEqual(MiniCardView.Typography.bodyPointSize, 12)
        XCTAssertEqual(MiniCardView.Typography.secondaryPointSize, 11)

        XCTAssertEqual(FocusPanelView.Typography.primaryPointSize, 13)
        XCTAssertEqual(FocusPanelView.Typography.bodyPointSize, 12)
        XCTAssertEqual(FocusPanelView.Typography.secondaryPointSize, 11)
    }

    func testFocusPanel_DefaultHeaderPosition_IsBottom() {
        XCTAssertEqual(FocusPanelView.defaultHeaderPosition, .bottom)
    }

    func testRepoView_DefaultTopInset_IsEightPoints() {
        XCTAssertEqual(RepoViewController.layoutTopInset, 8)
    }

    func testStatusPublisherPollPolicy_PollsPreferredEveryCycle() {
        let preferred: Set<String> = ["/repo/a"]
        XCTAssertTrue(StatusPublisher.shouldPollPath("/repo/a", preferredPaths: preferred, pollCycle: 1, nonPreferredStride: 3))
        XCTAssertTrue(StatusPublisher.shouldPollPath("/repo/a", preferredPaths: preferred, pollCycle: 2, nonPreferredStride: 3))
    }

    func testStatusPublisherPollPolicy_SkipsNonPreferredBetweenStrideTicks() {
        let preferred: Set<String> = ["/repo/a"]
        XCTAssertFalse(StatusPublisher.shouldPollPath("/repo/b", preferredPaths: preferred, pollCycle: 1, nonPreferredStride: 3))
        XCTAssertFalse(StatusPublisher.shouldPollPath("/repo/b", preferredPaths: preferred, pollCycle: 2, nonPreferredStride: 3))
    }

    func testStatusPublisherPollPolicy_PollsNonPreferredOnStrideTick() {
        let preferred: Set<String> = ["/repo/a"]
        XCTAssertTrue(StatusPublisher.shouldPollPath("/repo/b", preferredPaths: preferred, pollCycle: 3, nonPreferredStride: 3))
        XCTAssertTrue(StatusPublisher.shouldPollPath("/repo/b", preferredPaths: preferred, pollCycle: 6, nonPreferredStride: 3))
    }

    func testStatusPublisherPollPolicy_NoPreferredPollsEverything() {
        XCTAssertTrue(StatusPublisher.shouldPollPath("/repo/a", preferredPaths: [], pollCycle: 1, nonPreferredStride: 3))
        XCTAssertTrue(StatusPublisher.shouldPollPath("/repo/b", preferredPaths: [], pollCycle: 2, nonPreferredStride: 3))
    }

    func testRepoView_BackgroundAndTerminalAdaptToAppearanceChanges() {
        let repoVC = RepoViewController()
        repoVC.loadViewIfNeeded()

        let terminal = findView(in: repoVC.view, identifier: "project.terminal")
        XCTAssertNotNil(terminal)

        repoVC.view.appearance = NSAppearance(named: .aqua)
        repoVC.view.viewDidChangeEffectiveAppearance()
        repoVC.view.needsDisplay = true
        repoVC.view.displayIfNeeded()
        let lightRoot = repoVC.view.layer?.backgroundColor
        let lightTerminalBg = terminal?.layer?.backgroundColor
        let lightTerminalBorder = terminal?.layer?.borderColor
        let lightTerminalBorderWidth = terminal?.layer?.borderWidth

        repoVC.view.appearance = NSAppearance(named: .darkAqua)
        repoVC.view.viewDidChangeEffectiveAppearance()
        repoVC.view.needsDisplay = true
        repoVC.view.displayIfNeeded()
        let darkRoot = repoVC.view.layer?.backgroundColor
        let darkTerminalBg = terminal?.layer?.backgroundColor
        let darkTerminalBorder = terminal?.layer?.borderColor
        let darkTerminalBorderWidth = terminal?.layer?.borderWidth

        XCTAssertNotNil(lightRoot)
        XCTAssertNotNil(darkRoot)

        XCTAssertNotEqual(lightRoot, darkRoot)
        XCTAssertNotEqual(lightTerminalBg, darkTerminalBg)
        XCTAssertNotEqual(lightTerminalBorder, darkTerminalBorder)
        XCTAssertEqual(lightTerminalBorderWidth, 1)
        XCTAssertEqual(darkTerminalBorderWidth, 0)
    }

    func testSidebar_DefaultBackground_IsTransparent() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        guard let bgColor = sidebarVC.view.layer?.backgroundColor,
              let nsColor = NSColor(cgColor: bgColor)
        else {
            XCTFail("Sidebar background color should exist")
            return
        }

        XCTAssertLessThanOrEqual(nsColor.alphaComponent, 0.001)
    }

    func testSidebar_UsesComfortableRowInsets() {
        XCTAssertEqual(SidebarViewController.Layout.listHorizontalInset, 0)
        XCTAssertEqual(SidebarViewController.Layout.rowBackgroundHorizontalInset, 8)
        XCTAssertEqual(SidebarViewController.Layout.cellLeadingInset, 8)
        XCTAssertEqual(SidebarViewController.Layout.cellTrailingInset, 6)
    }

    func testSidebar_HeaderSeparator_DefaultsToHidden() {
        XCTAssertFalse(SidebarViewController.Layout.showsHeaderSeparator)
    }

    func testRepoView_SideBySideTerminalCorners_LeftOnlyRounded() {
        XCTAssertEqual(RepoViewController.terminalCornerRadius, 10)
        let expected: CACornerMask = [.layerMinXMinYCorner, .layerMinXMaxYCorner]
        XCTAssertEqual(RepoViewController.sideBySideTerminalMaskedCorners, expected)
    }

    func testNewThreadDialog_ActionButtonsUseEqualSizingPolicy() {
        XCTAssertTrue(NewBranchDialog.Layout.actionButtonsFillEqually)
        XCTAssertEqual(NewBranchDialog.Layout.actionButtonHeight, 40)
    }

    func testSidebar_DefaultSelectionStyle_IsMacOSNative() {
        XCTAssertTrue(SidebarViewController.Layout.usesNativeSelectionStyle)
    }

    func testSidebar_TableDisallowsEmptySelection() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        let table = findView(in: sidebarVC.view, identifier: "sidebar.worktreeList") as? NSTableView
        XCTAssertNotNil(table)
        XCTAssertFalse(table?.allowsEmptySelection ?? true)
    }

    func testSidebar_TableDoesNotForceSourceListStyle() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        let table = findView(in: sidebarVC.view, identifier: "sidebar.worktreeList") as? NSTableView
        XCTAssertNotNil(table)
        XCTAssertNotEqual(table?.style, .sourceList)
    }

    func testSidebar_TableUsesRegularSelectionHighlight() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        let table = findView(in: sidebarVC.view, identifier: "sidebar.worktreeList") as? NSTableView
        XCTAssertNotNil(table)
        XCTAssertEqual(table?.selectionHighlightStyle, .regular)
    }

    func testSidebar_DoesNotProvideCustomRowViewForSelectionAppearance() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        let info = WorktreeInfo(path: NSHomeDirectory(), branch: "main", commitHash: "abc", isMainWorktree: true)
        sidebarVC.setWorktrees([info])

        let table = findView(in: sidebarVC.view, identifier: "sidebar.worktreeList") as? NSTableView
        XCTAssertNotNil(table)
        XCTAssertFalse(sidebarVC.responds(to: NSSelectorFromString("tableView:rowViewForRow:")))
    }

    func testRepoShowTerminal_DoesNotForceFirstResponderToTerminalView() {
        let repoVC = RepoViewController()
        let window = TrackingWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1200, height: 800),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.contentViewController = repoVC
        repoVC.loadViewIfNeeded()

        let info = WorktreeInfo(path: NSHomeDirectory(), branch: "main", commitHash: "abc", isMainWorktree: true)
        repoVC.configure(worktrees: [info], surfaces: [info.path: TerminalSurface()])

        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        window.makeFirstResponderCallCount = 0

        repoVC.showTerminal(at: 0)
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))

        XCTAssertEqual(window.makeFirstResponderCallCount, 0)
    }

    func testSidebar_AddButton_UsesNativeButtonChrome() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        let addButton = findButton(in: sidebarVC.view, identifier: "sidebar.addThread")
        XCTAssertNotNil(addButton)
        XCTAssertTrue(addButton?.isBordered == true)
        XCTAssertEqual(addButton?.bezelStyle, .texturedRounded)
        XCTAssertNotNil(addButton?.image)
    }

    func testSidebar_DiffButtonExistsAndDefaultDisabled() {
        let sidebarVC = SidebarViewController()
        sidebarVC.loadViewIfNeeded()

        let diffButton = findButton(in: sidebarVC.view, identifier: "sidebar.showDiff")
        XCTAssertNotNil(diffButton)
        XCTAssertFalse(diffButton?.isEnabled ?? true)
        XCTAssertEqual(diffButton?.bezelStyle, .texturedRounded)
        XCTAssertEqual(diffButton?.title, "")
        XCTAssertEqual(diffButton?.imagePosition, .imageOnly)
        XCTAssertNotNil(diffButton?.image)
    }

    func testNewThreadDialog_ActionButtonsUseNativeButtonChrome() {
        let dialog = NewBranchDialog(repoPaths: ["/tmp/repo"])
        dialog.loadViewIfNeeded()

        let createButton = findButton(in: dialog.view, identifier: "dialog.newBranch.createButton")
        XCTAssertNotNil(createButton)
        XCTAssertTrue(createButton?.isBordered == true)
        XCTAssertEqual(createButton?.bezelStyle, .rounded)
    }

    func testUnifiedModal_UsesNativeButtonChrome() {
        let modal = UnifiedModalView(frame: NSRect(x: 0, y: 0, width: 600, height: 400))
        modal.show(config: ModalConfig(title: "Title", subtitle: "Subtitle", confirmText: "Confirm"))

        let confirm = findButton(in: modal, identifier: "modal.confirm")
        let cancel = findButton(in: modal, identifier: "modal.cancel")

        XCTAssertNotNil(confirm)
        XCTAssertNotNil(cancel)
        XCTAssertTrue(confirm?.isBordered == true)
        XCTAssertTrue(cancel?.isBordered == true)
        XCTAssertEqual(confirm?.bezelStyle, .rounded)
        XCTAssertEqual(cancel?.bezelStyle, .rounded)
    }

    func testTitleBar_UsesSystemAlignedCapsuleHeights() {
        XCTAssertEqual(TitleBarView.Layout.barHeight, 45)
        XCTAssertEqual(TitleBarView.Layout.capsuleHeight, 37)
        XCTAssertEqual(TitleBarView.Layout.arcVerticalOffset, 2)
        XCTAssertEqual(TitleBarView.Layout.dashboardLeadingInset, 16)
        XCTAssertEqual(TitleBarView.Layout.dashboardHorizontalPadding, 10)
    }

    func testTitleBar_DashboardLabelUsesSemanticTextNotSpacePadding() {
        let titleBar = TitleBarView()
        titleBar.layoutSubtreeIfNeeded()

        let dashboard = titleBar.subviews
            .flatMap { $0.subviews }
            .compactMap { $0 as? NSButton }
            .first { $0.accessibilityIdentifier() == "titlebar.dashboardTab" }

        XCTAssertNotNil(dashboard)
        XCTAssertEqual(dashboard?.attributedTitle.string, "\u{FFFC} Dashboard")
    }

    func testTitleBar_DashboardTabUsesCenteredAlignment() {
        let titleBar = TitleBarView()
        titleBar.layoutSubtreeIfNeeded()

        let dashboard = titleBar.subviews
            .flatMap { $0.subviews }
            .compactMap { $0 as? NSButton }
            .first { $0.accessibilityIdentifier() == "titlebar.dashboardTab" }

        XCTAssertNotNil(dashboard)
        XCTAssertEqual(dashboard?.alignment, .center)
    }

    func testTitleBar_RenderTabsResetsScrollOffset() {
        let titleBar = TitleBarView(frame: NSRect(x: 0, y: 0, width: 320, height: 52))
        titleBar.projects = ["opencode", "ganwork", "feature/new-thread", "long-long-long-branch"]
        titleBar.currentView = "dashboard"
        titleBar.renderTabs()
        titleBar.layoutSubtreeIfNeeded()

        guard let tabsScroll = findFirstScrollView(in: titleBar) else {
            XCTFail("Expected tabs scroll view")
            return
        }

        tabsScroll.contentView.setBoundsOrigin(NSPoint(x: 120, y: 0))
        tabsScroll.reflectScrolledClipView(tabsScroll.contentView)

        titleBar.renderTabs()

        XCTAssertEqual(tabsScroll.contentView.bounds.origin.x, 0, accuracy: 0.001)
    }

    func testMainWindowController_TrafficLightsAlignWithCapsuleCenter() {
        let originY = MainWindowController.trafficLightButtonOriginY(containerHeight: 52, buttonHeight: 12)
        XCTAssertEqual(originY, 22, accuracy: 0.001)
    }

    func testMainWindowController_DoesNotUseEscAsGlobalShortcut() {
        XCTAssertFalse(MainWindowController.shouldHandleEscShortcut())
    }

    func testDashboardFocusLayouts_UseEdgeFlushSpacingAndCornerMasks() {
        XCTAssertEqual(DashboardViewController.LayoutMetrics.focusPanelCornerRadius, 10)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.containerHorizontalInset, 0)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.containerBottomInset, 0)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topSmallFocusJoinSpacing, 8)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topLargeFocusJoinSpacing, 0)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topSmallMiniRowHorizontalInset, 8)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topLargeMiniRowHorizontalInset, 8)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topLargeMiniRowBottomInset, 8)
        XCTAssertEqual(DashboardViewController.LayoutMetrics.leftRightSidebarTrailingInset, 8)

        let topSmallExpected: CACornerMask = [.layerMinXMinYCorner, .layerMaxXMinYCorner]
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topSmallFocusMaskedCorners, topSmallExpected)

        let topLargeExpected: CACornerMask = [.layerMinXMaxYCorner, .layerMaxXMaxYCorner]
        XCTAssertEqual(DashboardViewController.LayoutMetrics.topLargeFocusMaskedCorners, topLargeExpected)

        let leftRightExpected: CACornerMask = [.layerMaxXMinYCorner, .layerMaxXMaxYCorner]
        XCTAssertEqual(DashboardViewController.LayoutMetrics.leftRightFocusMaskedCorners, leftRightExpected)
    }

    func testDashboardLeftRightSidebarScroll_HasTrailingInset() {
        let dashboard = DashboardViewController()
        dashboard.loadViewIfNeeded()

        guard let leftRightContainer = findView(in: dashboard.view, identifier: "dashboard.layout.left-right") else {
            XCTFail("Expected left-right container")
            return
        }

        let trailingConstraint = leftRightContainer.constraints.first {
            guard
                let first = $0.firstItem as? NSView,
                let second = $0.secondItem as? NSView
            else { return false }

            return $0.firstAttribute == .trailing
                && $0.secondAttribute == .trailing
                && first is NSScrollView
                && second == leftRightContainer
        }

        XCTAssertNotNil(trailingConstraint)
        XCTAssertEqual(trailingConstraint?.constant ?? .greatestFiniteMagnitude, -8, accuracy: 0.001)
    }

    func testMainWindowController_WindowAutosaveDisabledInUITestEnvironment() {
        XCTAssertFalse(MainWindowController.shouldUseWindowFrameAutosave(
            environment: ["XCTestConfigurationFilePath": "/tmp/test.xctestconfiguration"],
            arguments: []
        ))
        XCTAssertFalse(MainWindowController.shouldUseWindowFrameAutosave(
            environment: [:],
            arguments: ["-PmuxUITesting"]
        ))
        XCTAssertTrue(MainWindowController.shouldUseWindowFrameAutosave(environment: [:], arguments: []))
    }

    func testMainWindowController_LightModeUsesGlassBackground() {
        let cfg = MainWindowController.glassBackgroundConfig(isDark: false)
        XCTAssertTrue(cfg.enabled)
        XCTAssertEqual(cfg.material, .underWindowBackground)
        XCTAssertEqual(cfg.blendingMode, .behindWindow)
    }

    func testMainWindowController_ResolvePreferredBackend_UsesZmxWhenAvailable() {
        let resolved = MainWindowController.resolvePreferredBackend(preferred: "zmx", zmxAvailable: true, tmuxAvailable: true)
        XCTAssertEqual(resolved, "zmx")
    }

    func testMainWindowController_ResolvePreferredBackend_FallsBackToTmuxWhenZmxMissing() {
        let resolved = MainWindowController.resolvePreferredBackend(preferred: "zmx", zmxAvailable: false, tmuxAvailable: true)
        XCTAssertEqual(resolved, "tmux")
    }

    func testMainWindowController_ResolvePreferredBackend_FallsBackToLocalWhenNoBackendInstalled() {
        let resolved = MainWindowController.resolvePreferredBackend(preferred: "zmx", zmxAvailable: false, tmuxAvailable: false)
        XCTAssertEqual(resolved, "local")
    }

    func testMainWindowController_ResolvePreferredBackend_LocalAutoRecoversToZmx() {
        let resolved = MainWindowController.resolvePreferredBackend(preferred: "local", zmxAvailable: true, tmuxAvailable: true)
        XCTAssertEqual(resolved, "zmx")
    }

    func testMainWindowController_IsSupportedZmxVersion() {
        XCTAssertTrue(MainWindowController.isSupportedZmxVersion("0.4.2"))
        XCTAssertTrue(MainWindowController.isSupportedZmxVersion("v0.4.9"))
        XCTAssertTrue(MainWindowController.isSupportedZmxVersion("0.5.0"))
        XCTAssertFalse(MainWindowController.isSupportedZmxVersion("0.3.9"))
        XCTAssertFalse(MainWindowController.isSupportedZmxVersion("bad-value"))
    }
}

private func findButton(in root: NSView, identifier: String) -> NSButton? {
    if let button = root as? NSButton,
       button.accessibilityIdentifier() == identifier {
        return button
    }
    for subview in root.subviews {
        if let button = findButton(in: subview, identifier: identifier) {
            return button
        }
    }
    return nil
}

private func findView(in root: NSView, identifier: String) -> NSView? {
    if root.accessibilityIdentifier() == identifier {
        return root
    }
    for subview in root.subviews {
        if let view = findView(in: subview, identifier: identifier) {
            return view
        }
    }
    return nil
}

private func findFirstScrollView(in root: NSView) -> NSScrollView? {
    if let scrollView = root as? NSScrollView {
        return scrollView
    }
    for subview in root.subviews {
        if let found = findFirstScrollView(in: subview) {
            return found
        }
    }
    return nil
}

private final class TrackingWindow: NSWindow {
    var makeFirstResponderCallCount = 0

    override func makeFirstResponder(_ responder: NSResponder?) -> Bool {
        makeFirstResponderCallCount += 1
        return super.makeFirstResponder(responder)
    }
}
