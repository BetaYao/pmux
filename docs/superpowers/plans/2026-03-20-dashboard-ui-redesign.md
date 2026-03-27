# Dashboard UI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the amux Dashboard and Project Workspace UI to match the HTML hi-fi prototype 1:1, implementing 4 dashboard layouts, Zoom-style title bar, notification/AI side panels, unified modal system, status bar, and theme system.

**Architecture:** AppKit-only, delegate-driven. Replace existing TabBarView and DashboardViewController with new components aligned to the prototype. Reuse TerminalSurface lifecycle (long-lived, reparented). Theme system uses NSAppearance + custom semantic color tokens. Grid layout retains zoom and drag-to-reorder.

**Tech Stack:** Swift 5.10, AppKit, macOS 14.0+, Ghostty C interop (unchanged)

**Prototype reference:** Download from `https://raw.githubusercontent.com/zhoujinliang/amux/main/docs/dashboard-hifi-prototype.html` — open in browser for interactive reference
**Design doc reference:** Download from `https://raw.githubusercontent.com/zhoujinliang/amux/main/docs/2026-03-20-multi-agent-dashboard-workspace-design.md`

**Data model mapping:**
- "project" = repo display name (from `WorkspaceManager.WorkspaceTab.displayName`)
- "thread" = worktree branch name (from `WorktreeInfo.branch`)
- "totalDuration" / "roundDuration" = tracked by `StatusPublisher` via new `AgentDuration` struct (added in Task 1.5)
- "agent" = a worktree with its associated terminal surface and status

**Window chrome:** Use `titlebarAppearsTransparent = true` + hide real traffic lights via `standardWindowButton(.closeButton)?.isHidden = true` etc. TitleBarView provides decorative traffic dots matching prototype.

---

## File Structure

### New Files
- `Sources/UI/Shared/SemanticColors.swift` — Theme color tokens (dark/light variants), replaces hardcoded Theme colors
- `Sources/UI/Shared/AgentDisplayHelpers.swift` — Shared `statusColor()` and `compactDuration()` helpers used by all card views
- `Sources/UI/TitleBar/TitleBarView.swift` — Zoom-style title bar with tabs, toolbar buttons, traffic lights
- `Sources/UI/TitleBar/LayoutPopoverView.swift` — 4-layout popover menu
- `Sources/UI/Dashboard/AgentCardView.swift` — Information-only agent card (Grid layout)
- `Sources/UI/Dashboard/MiniCardView.swift` — 16:9 compact card for non-Grid layouts
- `Sources/UI/Dashboard/FocusPanelView.swift` — Focus panel (42px header + terminal)
- `Sources/UI/Panel/NotificationPanelView.swift` — Right-slide notification panel
- `Sources/UI/Panel/AIPanelView.swift` — Right-slide AI assistant panel (UI shell)
- `Sources/UI/Panel/PanelBackdropView.swift` — Shared backdrop overlay for panels
- `Sources/UI/StatusBar/StatusBarView.swift` — 32px bottom status bar
- `Sources/UI/Dialog/UnifiedModalView.swift` — Unified modal (close project, add project, new thread)

### Modified Files
- `Sources/UI/Shared/Theme.swift` — Extend with semantic color references and appearance management
- `Sources/UI/Dashboard/DashboardViewController.swift` — Rewrite: 2 modes → 4 layouts, use new card/focus components
- `Sources/UI/Dashboard/TerminalCardView.swift` — Remove (replaced by AgentCardView)
- `Sources/UI/Repo/RepoViewController.swift` — Simplify: remove split panes, single terminal
- `Sources/UI/Repo/SidebarViewController.swift` — Restyle to match prototype (no color bar, accent border)
- `Sources/App/MainWindowController.swift` — Major rewrite: new shell layout, title bar, panels, status bar, modal
- `Sources/Core/Config.swift` — Add `dashboardLayout` and `themeMode` fields
- `Sources/UI/TabBar/TabBarView.swift` — Remove (replaced by TitleBarView)
- `Sources/UI/Repo/TerminalSplitView.swift` — Remove (no more split panes)
- `project.yml` — Add new source files

### Kept Unchanged
- `Sources/UI/Dashboard/GridLayout.swift` — Pure layout math (zoom levels)
- `Sources/UI/Dashboard/DraggableGridView.swift` — Drag-drop for Grid layout only
- `Sources/UI/Dialog/QuickSwitcherViewController.swift` — Cmd+P quick switcher
- `Sources/UI/Shared/StatusBadge.swift` — Status dot component
- `Sources/Terminal/TerminalSurface.swift` — Terminal engine
- `Sources/Terminal/GhosttyBridge.swift` — C interop
- `Sources/Core/WorkspaceManager.swift` — Tab/workspace state
- `Sources/Status/*` — Status detection (StatusPublisher, StatusDetector, NotificationManager)
- `Sources/Git/*` — Git operations
- `Sources/Core/FuzzyMatch.swift` — Fuzzy search

---

## Task 1: Theme System — Semantic Colors + Appearance Management

**Files:**
- Modify: `Sources/UI/Shared/Theme.swift`
- Create: `Sources/UI/Shared/SemanticColors.swift`
- Modify: `Sources/Core/Config.swift`
- Test: `Tests/ConfigTests.swift`

### Goal
Replace hardcoded dark-only colors with a semantic color token system that supports Dark/Light/System appearance modes, matching the prototype's CSS custom properties.

- [ ] **Step 1: Create SemanticColors.swift with all tokens**

```swift
// Sources/UI/Shared/SemanticColors.swift
import AppKit

/// Semantic color tokens aligned with prototype CSS variables.
/// Colors automatically adapt to current NSAppearance (dark/light).
enum SemanticColors {
    // MARK: - Backgrounds
    static var bg: NSColor {
        NSColor(name: "semanticBg") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x0f1115)
                : NSColor(hex: 0xf3f4f7)
        }
    }

    static var panel: NSColor {
        NSColor(name: "semanticPanel") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x15171c)
                : NSColor(hex: 0xffffff)
        }
    }

    static var panel2: NSColor {
        NSColor(name: "semanticPanel2") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x1b1e25)
                : NSColor(hex: 0xf7f8fb)
        }
    }

    // MARK: - Text
    static var text: NSColor {
        NSColor(name: "semanticText") { appearance in
            appearance.isDark
                ? NSColor(hex: 0xf3f5f8)
                : NSColor(hex: 0x1f232b)
        }
    }

    static var muted: NSColor {
        NSColor(name: "semanticMuted") { appearance in
            appearance.isDark
                ? NSColor(hex: 0xa8afbc)
                : NSColor(hex: 0x636b78)
        }
    }

    // MARK: - Borders
    static var line: NSColor {
        NSColor(name: "semanticLine") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x262a33)
                : NSColor(hex: 0xd7dbe3)
        }
    }

    // MARK: - Status
    static var running: NSColor {
        NSColor(name: "semanticRunning") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x33c17b)
                : NSColor(hex: 0x1f9d63)
        }
    }

    static var waiting: NSColor {
        NSColor(name: "semanticWaiting") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x3b82f6)
                : NSColor(hex: 0x2563eb)
        }
    }

    static var idle: NSColor {
        NSColor(name: "semanticIdle") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x9ca3af)
                : NSColor(hex: 0x8a93a1)
        }
    }

    static var accent: NSColor {
        NSColor(name: "semanticAccent") { appearance in
            appearance.isDark
                ? NSColor(hex: 0x4f8cff)
                : NSColor(hex: 0x2563eb)
        }
    }

    static var danger: NSColor {
        NSColor(name: "semanticDanger") { appearance in
            appearance.isDark
                ? NSColor(hex: 0xff453a)
                : NSColor(hex: 0xdc2626)
        }
    }
}

// MARK: - Helpers

extension NSColor {
    convenience init(hex: Int) {
        self.init(
            red: CGFloat((hex >> 16) & 0xFF) / 255.0,
            green: CGFloat((hex >> 8) & 0xFF) / 255.0,
            blue: CGFloat(hex & 0xFF) / 255.0,
            alpha: 1.0
        )
    }
}

extension NSAppearance {
    var isDark: Bool {
        bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
    }
}
```

- [ ] **Step 2: Add ThemeMode to Config**

Add to `Sources/Core/Config.swift`:

```swift
// Add property to Config struct:
var themeMode: String  // "dark", "light", "system"

// Add to CodingKeys:
case themeMode = "theme_mode"

// Add to init():
themeMode = "system"

// Add to init(from decoder:):
themeMode = try container.decodeIfPresent(String.self, forKey: .themeMode) ?? "system"
```

- [ ] **Step 3: Add dashboardLayout to Config**

Add to `Sources/Core/Config.swift`:

```swift
// Add property to Config struct:
var dashboardLayout: String  // "grid", "left-right", "top-small", "top-large"

// Add to CodingKeys:
case dashboardLayout = "dashboard_layout"

// Add to init():
dashboardLayout = "left-right"

// Add to init(from decoder:):
dashboardLayout = try container.decodeIfPresent(String.self, forKey: .dashboardLayout) ?? "left-right"
```

- [ ] **Step 4: Add appearance helper to Theme.swift**

```swift
// Add to Theme.swift:
enum ThemeMode: String {
    case dark, light, system
}

static func applyAppearance(_ mode: ThemeMode) {
    switch mode {
    case .dark:
        NSApp.appearance = NSAppearance(named: .darkAqua)
    case .light:
        NSApp.appearance = NSAppearance(named: .aqua)
    case .system:
        NSApp.appearance = nil
    }
}
```

- [ ] **Step 5: Update existing Theme colors to use SemanticColors**

Replace hardcoded hex values in `Theme.swift` to delegate to `SemanticColors`:

```swift
static var background: NSColor { SemanticColors.bg }
static var surface: NSColor { SemanticColors.panel2 }
static var surfaceHover: NSColor { SemanticColors.panel2 }
static var border: NSColor { SemanticColors.line }
static var textPrimary: NSColor { SemanticColors.text }
static var textSecondary: NSColor { SemanticColors.muted }
static var textDim: NSColor { SemanticColors.muted }
static var accent: NSColor { SemanticColors.accent }
```

- [ ] **Step 6: Write config test for new fields**

Add to `Tests/ConfigTests.swift`:

```swift
func testDefaultDashboardLayout() {
    let config = Config()
    XCTAssertEqual(config.dashboardLayout, "left-right")
}

func testDefaultThemeMode() {
    let config = Config()
    XCTAssertEqual(config.themeMode, "system")
}

func testDecodeMissingNewFields() {
    let json = """
    {"workspace_paths": ["/tmp/test"]}
    """.data(using: .utf8)!
    let config = try! JSONDecoder().decode(Config.self, from: json)
    XCTAssertEqual(config.dashboardLayout, "left-right")
    XCTAssertEqual(config.themeMode, "system")
}
```

- [ ] **Step 7: Run tests**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test -only-testing:amuxTests/ConfigTests 2>&1 | tail -20`

- [ ] **Step 8: Commit**

```
feat: add semantic color system and theme mode support
```

---

## Task 2: Status Bar View

**Files:**
- Create: `Sources/UI/StatusBar/StatusBarView.swift`

### Goal
32px bottom status bar with left status text and right keyboard hint badges.

- [ ] **Step 1: Create StatusBarView**

```swift
// Sources/UI/StatusBar/StatusBarView.swift
import AppKit

final class StatusBarView: NSView {
    private let statusLabel = NSTextField(labelWithString: "")
    private let hintsStack = NSStackView()

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        let topBorder = NSView()
        topBorder.wantsLayer = true
        topBorder.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.55).cgColor
        addSubview(topBorder)
        topBorder.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            topBorder.topAnchor.constraint(equalTo: topAnchor),
            topBorder.leadingAnchor.constraint(equalTo: leadingAnchor),
            topBorder.trailingAnchor.constraint(equalTo: trailingAnchor),
            topBorder.heightAnchor.constraint(equalToConstant: 1)
        ])

        wantsLayer = true
        layer?.backgroundColor = SemanticColors.panel.cgColor

        statusLabel.font = NSFont.systemFont(ofSize: 11)
        statusLabel.textColor = SemanticColors.muted
        statusLabel.lineBreakMode = .byTruncatingTail

        hintsStack.orientation = .horizontal
        hintsStack.spacing = 10
        hintsStack.alignment = .centerY

        addHint("切换布局", kbd: "V")
        addHint("新建 Thread", kbd: "N")
        addHint("提交弹窗", kbd: "⌘", kbd2: "Enter")

        let mainStack = NSStackView(views: [statusLabel, hintsStack])
        mainStack.orientation = .horizontal
        mainStack.distribution = .fill
        mainStack.alignment = .centerY
        mainStack.edgeInsets = NSEdgeInsets(top: 0, left: 10, bottom: 0, right: 10)
        addSubview(mainStack)
        mainStack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            mainStack.leadingAnchor.constraint(equalTo: leadingAnchor),
            mainStack.trailingAnchor.constraint(equalTo: trailingAnchor),
            mainStack.topAnchor.constraint(equalTo: topAnchor),
            mainStack.bottomAnchor.constraint(equalTo: bottomAnchor)
        ])

        statusLabel.setContentHuggingPriority(.defaultLow, for: .horizontal)
        hintsStack.setContentHuggingPriority(.required, for: .horizontal)
    }

    private func addHint(_ text: String, kbd: String, kbd2: String? = nil) {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.borderWidth = 1
        container.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.6).cgColor
        container.layer?.cornerRadius = 6

        let label = NSTextField(labelWithString: text)
        label.font = NSFont.systemFont(ofSize: 11)
        label.textColor = SemanticColors.muted

        let kbdView = makeKbd(kbd)
        container.addSubview(label)
        container.addSubview(kbdView)
        label.translatesAutoresizingMaskIntoConstraints = false
        kbdView.translatesAutoresizingMaskIntoConstraints = false

        var constraints = [
            label.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 7),
            label.centerYAnchor.constraint(equalTo: container.centerYAnchor),
            container.heightAnchor.constraint(equalToConstant: 23),
        ]

        if let kbd2 = kbd2 {
            let plus = NSTextField(labelWithString: "+")
            plus.font = NSFont.systemFont(ofSize: 10)
            plus.textColor = SemanticColors.muted
            let kbdView2 = makeKbd(kbd2)
            container.addSubview(plus)
            container.addSubview(kbdView2)
            plus.translatesAutoresizingMaskIntoConstraints = false
            kbdView2.translatesAutoresizingMaskIntoConstraints = false
            constraints += [
                kbdView.leadingAnchor.constraint(equalTo: label.trailingAnchor, constant: 5),
                plus.leadingAnchor.constraint(equalTo: kbdView.trailingAnchor, constant: 2),
                kbdView2.leadingAnchor.constraint(equalTo: plus.trailingAnchor, constant: 2),
                kbdView2.centerYAnchor.constraint(equalTo: container.centerYAnchor),
                plus.centerYAnchor.constraint(equalTo: container.centerYAnchor),
                container.trailingAnchor.constraint(equalTo: kbdView2.trailingAnchor, constant: 7),
            ]
        } else {
            constraints += [
                kbdView.leadingAnchor.constraint(equalTo: label.trailingAnchor, constant: 5),
                container.trailingAnchor.constraint(equalTo: kbdView.trailingAnchor, constant: 7),
            ]
        }
        constraints.append(kbdView.centerYAnchor.constraint(equalTo: container.centerYAnchor))
        NSLayoutConstraint.activate(constraints)

        hintsStack.addArrangedSubview(container)
    }

    private func makeKbd(_ text: String) -> NSView {
        let label = NSTextField(labelWithString: text)
        label.font = NSFont.systemFont(ofSize: 10, weight: .medium)
        label.textColor = SemanticColors.text
        label.alignment = .center

        let box = NSView()
        box.wantsLayer = true
        box.layer?.cornerRadius = 4
        box.layer?.borderWidth = 1
        box.layer?.borderColor = SemanticColors.line.cgColor
        box.layer?.backgroundColor = SemanticColors.panel2.cgColor
        box.addSubview(label)
        label.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: box.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: box.centerYAnchor),
            box.widthAnchor.constraint(greaterThanOrEqualToConstant: 17),
            box.heightAnchor.constraint(equalToConstant: 17),
            label.leadingAnchor.constraint(greaterThanOrEqualTo: box.leadingAnchor, constant: 3),
            label.trailingAnchor.constraint(lessThanOrEqualTo: box.trailingAnchor, constant: -3),
        ])
        return box
    }

    func updateStatus(_ text: String) {
        statusLabel.stringValue = text
    }

    override func updateLayer() {
        layer?.backgroundColor = SemanticColors.panel.cgColor
    }
}
```

- [ ] **Step 2: Commit**

```
feat: add StatusBarView component (32px bottom bar)
```

---

## Task 3: Unified Modal View

**Files:**
- Create: `Sources/UI/Dialog/UnifiedModalView.swift`

### Goal
A reusable modal that handles: close project confirmation, add project (path input), new thread (multiline input). Matches prototype exactly.

- [ ] **Step 1: Create UnifiedModalView**

```swift
// Sources/UI/Dialog/UnifiedModalView.swift
import AppKit

struct ModalConfig {
    let title: String
    let subtitle: String
    var placeholder: String = ""
    var initialValue: String = ""
    var confirmText: String = "确认"
    var isMultiline: Bool = false
    var confirmStyle: ModalButtonStyle = .primary

    enum ModalButtonStyle { case primary, warn }
}

protocol UnifiedModalDelegate: AnyObject {
    func modalDidConfirm(value: String)
    func modalDidCancel()
}

final class UnifiedModalView: NSView {
    weak var delegate: UnifiedModalDelegate?

    private let overlayView = NSView()
    private let cardView = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let subtitleLabel = NSTextField(labelWithString: "")
    private var inputField: NSTextField?
    private var textView: NSTextView?
    private var scrollView: NSScrollView?
    private let cancelButton = NSButton(title: "取消", target: nil, action: nil)
    private let confirmButton = NSButton(title: "确认", target: nil, action: nil)
    private var config: ModalConfig?

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        isHidden = true

        // Overlay
        overlayView.wantsLayer = true
        overlayView.layer?.backgroundColor = NSColor(red: 7/255, green: 10/255, blue: 20/255, alpha: 0.72).cgColor
        addSubview(overlayView)
        overlayView.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            overlayView.leadingAnchor.constraint(equalTo: leadingAnchor),
            overlayView.trailingAnchor.constraint(equalTo: trailingAnchor),
            overlayView.topAnchor.constraint(equalTo: topAnchor),
            overlayView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])

        let clickGesture = NSClickGestureRecognizer(target: self, action: #selector(overlayClicked))
        overlayView.addGestureRecognizer(clickGesture)

        // Card
        cardView.wantsLayer = true
        cardView.layer?.cornerRadius = 10
        cardView.layer?.borderWidth = 1
        cardView.layer?.borderColor = SemanticColors.line.cgColor
        cardView.layer?.backgroundColor = SemanticColors.panel.cgColor
        addSubview(cardView)
        cardView.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            cardView.centerXAnchor.constraint(equalTo: centerXAnchor),
            cardView.centerYAnchor.constraint(equalTo: centerYAnchor),
            cardView.widthAnchor.constraint(lessThanOrEqualToConstant: 560),
            cardView.widthAnchor.constraint(greaterThanOrEqualToConstant: 400),
        ])

        // Title
        titleLabel.font = NSFont.systemFont(ofSize: 16, weight: .bold)
        titleLabel.textColor = SemanticColors.text

        // Subtitle
        subtitleLabel.font = NSFont.systemFont(ofSize: 12)
        subtitleLabel.textColor = SemanticColors.muted

        // Buttons
        cancelButton.target = self
        cancelButton.action = #selector(cancelClicked)
        cancelButton.bezelStyle = .rounded

        confirmButton.target = self
        confirmButton.action = #selector(confirmClicked)
        confirmButton.bezelStyle = .rounded
        confirmButton.keyEquivalent = "\r"
    }

    func show(config: ModalConfig) {
        self.config = config
        titleLabel.stringValue = config.title
        subtitleLabel.stringValue = config.subtitle
        confirmButton.title = config.confirmText

        // Remove old input
        cardView.subviews.forEach { $0.removeFromSuperview() }
        inputField = nil
        textView = nil
        scrollView = nil

        // Build card content
        cardView.addSubview(titleLabel)
        cardView.addSubview(subtitleLabel)
        titleLabel.translatesAutoresizingMaskIntoConstraints = false
        subtitleLabel.translatesAutoresizingMaskIntoConstraints = false

        var constraints: [NSLayoutConstraint] = [
            titleLabel.topAnchor.constraint(equalTo: cardView.topAnchor, constant: 16),
            titleLabel.leadingAnchor.constraint(equalTo: cardView.leadingAnchor, constant: 16),
            titleLabel.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -16),
            subtitleLabel.topAnchor.constraint(equalTo: titleLabel.bottomAnchor, constant: 8),
            subtitleLabel.leadingAnchor.constraint(equalTo: cardView.leadingAnchor, constant: 16),
            subtitleLabel.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -16),
        ]

        let inputBottom: NSLayoutYAxisAnchor

        if config.isMultiline {
            let sv = NSScrollView()
            let tv = NSTextView()
            tv.font = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
            tv.textColor = SemanticColors.text
            tv.backgroundColor = SemanticColors.panel2
            tv.isRichText = false
            tv.string = config.initialValue
            tv.textContainerInset = NSSize(width: 8, height: 8)
            sv.documentView = tv
            sv.hasVerticalScroller = true
            sv.wantsLayer = true
            sv.layer?.cornerRadius = 8
            sv.layer?.borderWidth = 1
            sv.layer?.borderColor = SemanticColors.line.cgColor
            cardView.addSubview(sv)
            sv.translatesAutoresizingMaskIntoConstraints = false
            tv.translatesAutoresizingMaskIntoConstraints = false
            constraints += [
                sv.topAnchor.constraint(equalTo: subtitleLabel.bottomAnchor, constant: 12),
                sv.leadingAnchor.constraint(equalTo: cardView.leadingAnchor, constant: 16),
                sv.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -16),
                sv.heightAnchor.constraint(greaterThanOrEqualToConstant: 108),
                tv.widthAnchor.constraint(equalTo: sv.contentView.widthAnchor),
            ]
            inputBottom = sv.bottomAnchor
            textView = tv
            scrollView = sv
        } else {
            let field = NSTextField()
            field.font = NSFont.systemFont(ofSize: 13)
            field.textColor = SemanticColors.text
            field.backgroundColor = SemanticColors.panel2
            field.placeholderString = config.placeholder
            field.stringValue = config.initialValue
            field.isBordered = true
            field.wantsLayer = true
            field.layer?.cornerRadius = 8
            cardView.addSubview(field)
            field.translatesAutoresizingMaskIntoConstraints = false
            constraints += [
                field.topAnchor.constraint(equalTo: subtitleLabel.bottomAnchor, constant: 12),
                field.leadingAnchor.constraint(equalTo: cardView.leadingAnchor, constant: 16),
                field.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -16),
                field.heightAnchor.constraint(equalToConstant: 32),
            ]
            inputBottom = field.bottomAnchor
            inputField = field
        }

        // Buttons row
        let buttonsStack = NSStackView(views: [cancelButton, confirmButton])
        buttonsStack.orientation = .horizontal
        buttonsStack.spacing = 8
        buttonsStack.alignment = .centerY
        cardView.addSubview(buttonsStack)
        buttonsStack.translatesAutoresizingMaskIntoConstraints = false
        constraints += [
            buttonsStack.topAnchor.constraint(equalTo: inputBottom, constant: 12),
            buttonsStack.trailingAnchor.constraint(equalTo: cardView.trailingAnchor, constant: -16),
            buttonsStack.bottomAnchor.constraint(equalTo: cardView.bottomAnchor, constant: -16),
        ]

        NSLayoutConstraint.activate(constraints)
        isHidden = false
        window?.makeFirstResponder(config.isMultiline ? textView : inputField)
    }

    func dismiss() {
        isHidden = true
        config = nil
    }

    private var inputValue: String {
        if let tv = textView { return tv.string.trimmingCharacters(in: .whitespacesAndNewlines) }
        return inputField?.stringValue.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    @objc private func confirmClicked() { delegate?.modalDidConfirm(value: inputValue) }
    @objc private func cancelClicked() { delegate?.modalDidCancel() }
    @objc private func overlayClicked() { delegate?.modalDidCancel() }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 53 { // Escape
            delegate?.modalDidCancel()
            return
        }
        if config?.isMultiline == true && event.modifierFlags.contains(.command) && event.keyCode == 36 {
            // Cmd+Enter in multiline
            delegate?.modalDidConfirm(value: inputValue)
            return
        }
        super.keyDown(with: event)
    }
}
```

- [ ] **Step 2: Commit**

```
feat: add UnifiedModalView for project/thread dialogs
```

---

## Task 4: Side Panels — Notification + AI + Backdrop

**Files:**
- Create: `Sources/UI/Panel/PanelBackdropView.swift`
- Create: `Sources/UI/Panel/NotificationPanelView.swift`
- Create: `Sources/UI/Panel/AIPanelView.swift`

### Goal
Right-slide panels (360px, mutual exclusion) with shared backdrop overlay, matching prototype.

- [ ] **Step 1: Create PanelBackdropView**

Shared backdrop overlay: fixed, semi-transparent, click to dismiss.

```swift
// Sources/UI/Panel/PanelBackdropView.swift
import AppKit

protocol PanelBackdropDelegate: AnyObject {
    func backdropClicked()
}

final class PanelBackdropView: NSView {
    weak var delegate: PanelBackdropDelegate?

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.withAlphaComponent(0.2).cgColor
        isHidden = true
        let click = NSClickGestureRecognizer(target: self, action: #selector(clicked))
        addGestureRecognizer(click)
    }

    required init?(coder: NSCoder) { fatalError() }

    @objc private func clicked() { delegate?.backdropClicked() }

    func setVisible(_ visible: Bool) {
        isHidden = !visible
    }
}
```

- [ ] **Step 2: Create NotificationPanelView**

Right-slide panel with header, scrollable notification list.

```swift
// Sources/UI/Panel/NotificationPanelView.swift
import AppKit

protocol NotificationPanelDelegate: AnyObject {
    func notificationPanelDidRequestClose()
    func notificationPanelDidSelectItem(worktreePath: String)
}

final class NotificationPanelView: NSView {
    weak var delegate: NotificationPanelDelegate?

    private let headerLabel = NSTextField(labelWithString: "通知")
    private let closeButton = NSButton(title: "×", target: nil, action: nil)
    private let scrollView = NSScrollView()
    private let stackView = NSStackView()
    private(set) var isOpen = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = SemanticColors.panel.cgColor

        // Header
        headerLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        headerLabel.textColor = SemanticColors.text

        closeButton.target = self
        closeButton.action = #selector(closeTapped)
        closeButton.isBordered = false
        closeButton.font = NSFont.systemFont(ofSize: 18)
        closeButton.contentTintColor = SemanticColors.muted

        let headerStack = NSStackView(views: [headerLabel, closeButton])
        headerStack.orientation = .horizontal
        headerStack.distribution = .fill
        headerStack.edgeInsets = NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12)

        // List
        stackView.orientation = .vertical
        stackView.spacing = 6
        stackView.alignment = .leading
        scrollView.documentView = stackView
        scrollView.hasVerticalScroller = true
        scrollView.drawsBackground = false

        let container = NSStackView(views: [headerStack, scrollView])
        container.orientation = .vertical
        container.spacing = 0
        container.alignment = .leading
        addSubview(container)
        container.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            container.leadingAnchor.constraint(equalTo: leadingAnchor),
            container.trailingAnchor.constraint(equalTo: trailingAnchor),
            container.topAnchor.constraint(equalTo: topAnchor),
            container.bottomAnchor.constraint(equalTo: bottomAnchor),
            stackView.widthAnchor.constraint(equalTo: scrollView.widthAnchor),
        ])
    }

    func setOpen(_ open: Bool, animated: Bool = true) {
        isOpen = open
        NSAnimationContext.runAnimationGroup { ctx in
            ctx.duration = animated ? 0.22 : 0
            ctx.timingFunction = CAMediaTimingFunction(name: .easeInEaseOut)
            self.animator().alphaValue = open ? 1 : 0
        }
        // Slide handled by parent constraint updates
    }

    func updateNotifications(_ items: [(title: String, meta: String)]) {
        stackView.arrangedSubviews.forEach { $0.removeFromSuperview() }
        for item in items {
            let view = makeNotificationItem(title: item.title, meta: item.meta)
            stackView.addArrangedSubview(view)
            view.widthAnchor.constraint(equalTo: stackView.widthAnchor, constant: -16).isActive = true
        }
    }

    private func makeNotificationItem(title: String, meta: String) -> NSView {
        let container = NSView()
        container.wantsLayer = true
        container.layer?.cornerRadius = 8
        container.layer?.backgroundColor = SemanticColors.panel2.cgColor

        let titleLabel = NSTextField(labelWithString: title)
        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .semibold)
        titleLabel.textColor = SemanticColors.text

        let metaLabel = NSTextField(labelWithString: meta)
        metaLabel.font = NSFont.systemFont(ofSize: 11)
        metaLabel.textColor = SemanticColors.muted

        let stack = NSStackView(views: [titleLabel, metaLabel])
        stack.orientation = .vertical
        stack.spacing = 4
        stack.alignment = .leading
        container.addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: container.topAnchor, constant: 10),
            stack.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: 12),
            stack.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -12),
            stack.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -10),
        ])
        return container
    }

    @objc private func closeTapped() { delegate?.notificationPanelDidRequestClose() }
}
```

- [ ] **Step 3: Create AIPanelView (UI shell)**

Right-slide AI panel with message bubbles and input area. Placeholder logic only.

```swift
// Sources/UI/Panel/AIPanelView.swift
import AppKit

protocol AIPanelDelegate: AnyObject {
    func aiPanelDidRequestClose()
}

final class AIPanelView: NSView, NSTextFieldDelegate {
    weak var delegate: AIPanelDelegate?

    private let messagesScroll = NSScrollView()
    private let messagesStack = NSStackView()
    private let inputField = NSTextField()
    private let sendButton = NSButton(title: "发送", target: nil, action: nil)
    private(set) var isOpen = false

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = SemanticColors.panel.cgColor

        // Header
        let headerLabel = NSTextField(labelWithString: "AI 助手")
        headerLabel.font = NSFont.systemFont(ofSize: 13, weight: .bold)
        headerLabel.textColor = SemanticColors.text

        let closeBtn = NSButton(title: "×", target: self, action: #selector(closeTapped))
        closeBtn.isBordered = false
        closeBtn.font = NSFont.systemFont(ofSize: 18)
        closeBtn.contentTintColor = SemanticColors.muted

        let header = NSStackView(views: [headerLabel, closeBtn])
        header.orientation = .horizontal
        header.distribution = .fill
        header.edgeInsets = NSEdgeInsets(top: 10, left: 12, bottom: 10, right: 12)

        // Messages
        messagesStack.orientation = .vertical
        messagesStack.spacing = 8
        messagesStack.alignment = .leading
        messagesScroll.documentView = messagesStack
        messagesScroll.hasVerticalScroller = true
        messagesScroll.drawsBackground = false

        // Welcome message
        addBubble(role: .assistant, text: "你好，我是工作区助手。可以问我关于当前 project、thread 或命令的问题。（原型演示）")

        // Input
        inputField.placeholderString = "输入消息… Shift+Enter 换行"
        inputField.font = NSFont.systemFont(ofSize: 12)
        inputField.delegate = self
        inputField.backgroundColor = SemanticColors.panel2
        inputField.textColor = SemanticColors.text

        sendButton.target = self
        sendButton.action = #selector(sendTapped)
        sendButton.bezelStyle = .rounded

        let inputRow = NSStackView(views: [inputField, sendButton])
        inputRow.orientation = .horizontal
        inputRow.spacing = 6
        inputRow.alignment = .centerY
        inputRow.edgeInsets = NSEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)

        let container = NSStackView(views: [header, messagesScroll, inputRow])
        container.orientation = .vertical
        container.spacing = 0
        addSubview(container)
        container.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            container.leadingAnchor.constraint(equalTo: leadingAnchor),
            container.trailingAnchor.constraint(equalTo: trailingAnchor),
            container.topAnchor.constraint(equalTo: topAnchor),
            container.bottomAnchor.constraint(equalTo: bottomAnchor),
            messagesStack.widthAnchor.constraint(equalTo: messagesScroll.widthAnchor),
            inputField.heightAnchor.constraint(greaterThanOrEqualToConstant: 36),
        ])
    }

    enum BubbleRole { case user, assistant }

    func addBubble(role: BubbleRole, text: String) {
        let bubble = NSTextField(wrappingLabelWithString: text)
        bubble.font = NSFont.systemFont(ofSize: 12)
        bubble.textColor = SemanticColors.text
        bubble.wantsLayer = true
        bubble.layer?.cornerRadius = 10
        bubble.drawsBackground = true

        switch role {
        case .user:
            bubble.backgroundColor = SemanticColors.accent.withAlphaComponent(0.18).blended(withFraction: 0.82, of: SemanticColors.panel2) ?? SemanticColors.panel2
            bubble.alignment = .right
        case .assistant:
            bubble.backgroundColor = SemanticColors.panel2
            bubble.alignment = .left
        }

        let wrapper = NSView()
        wrapper.addSubview(bubble)
        bubble.translatesAutoresizingMaskIntoConstraints = false
        let maxWidth = min(360 * 0.92, 320)
        NSLayoutConstraint.activate([
            bubble.topAnchor.constraint(equalTo: wrapper.topAnchor),
            bubble.bottomAnchor.constraint(equalTo: wrapper.bottomAnchor),
            bubble.widthAnchor.constraint(lessThanOrEqualToConstant: maxWidth),
        ])

        if role == .user {
            bubble.trailingAnchor.constraint(equalTo: wrapper.trailingAnchor, constant: -4).isActive = true
        } else {
            bubble.leadingAnchor.constraint(equalTo: wrapper.leadingAnchor, constant: 4).isActive = true
        }

        messagesStack.addArrangedSubview(wrapper)
        wrapper.widthAnchor.constraint(equalTo: messagesStack.widthAnchor).isActive = true

        DispatchQueue.main.async {
            self.messagesScroll.contentView.scroll(to: NSPoint(x: 0, y: self.messagesStack.frame.height))
        }
    }

    func setOpen(_ open: Bool) {
        isOpen = open
        if open {
            DispatchQueue.main.async { self.window?.makeFirstResponder(self.inputField) }
        }
    }

    @objc private func closeTapped() { delegate?.aiPanelDidRequestClose() }

    @objc private func sendTapped() {
        let text = inputField.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        addBubble(role: .user, text: text)
        inputField.stringValue = ""
        // Placeholder response (~450ms delay)
        let truncated = String(text.prefix(120)) + (text.count > 120 ? "…" : "")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { [weak self] in
            self?.addBubble(role: .assistant, text: "（原型）已收到。实际产品里这里会接模型回复。你刚才说：「\(truncated)」")
        }
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(insertNewline(_:)) {
            sendTapped()
            return true
        }
        return false
    }
}
```

- [ ] **Step 4: Commit**

```
feat: add notification panel, AI panel, and backdrop overlay
```

---

## Task 5: Title Bar View

**Files:**
- Create: `Sources/UI/TitleBar/TitleBarView.swift`
- Create: `Sources/UI/TitleBar/LayoutPopoverView.swift`

### Goal
Zoom-style title bar: traffic lights (decorative) + Dashboard capsule tab + separator + project tabs (status dot + close) + "+" button. Right: New Thread, layout popover, notification, AI, theme toggle.

- [ ] **Step 1: Create LayoutPopoverView**

4-item popover menu for layout selection.

```swift
// Sources/UI/TitleBar/LayoutPopoverView.swift
import AppKit

enum DashboardLayout: String, CaseIterable {
    case grid = "grid"
    case leftRight = "left-right"
    case topSmall = "top-small"
    case topLarge = "top-large"

    var displayName: String {
        switch self {
        case .grid: return "1 Grid"
        case .leftRight: return "2 左大右列"
        case .topSmall: return "3 上小下大"
        case .topLarge: return "4 上大下小"
        }
    }
}

protocol LayoutPopoverDelegate: AnyObject {
    func layoutPopover(_ popover: LayoutPopoverView, didSelect layout: DashboardLayout)
}

final class LayoutPopoverView: NSView {
    weak var delegate: LayoutPopoverDelegate?
    private(set) var currentLayout: DashboardLayout = .leftRight
    private var buttons: [NSButton] = []

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        layer?.borderColor = SemanticColors.line.withAlphaComponent(0.4).cgColor
        layer?.backgroundColor = SemanticColors.panel.cgColor
        layer?.shadowOpacity = 0.18
        layer?.shadowRadius = 14
        layer?.shadowOffset = CGSize(width: 0, height: -6)
        isHidden = true

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.spacing = 1

        for layout in DashboardLayout.allCases {
            let btn = NSButton(title: layout.displayName, target: self, action: #selector(itemClicked(_:)))
            btn.tag = DashboardLayout.allCases.firstIndex(of: layout) ?? 0
            btn.isBordered = false
            btn.font = NSFont.systemFont(ofSize: 12)
            btn.alignment = .left
            btn.contentTintColor = SemanticColors.text
            btn.wantsLayer = true
            btn.layer?.cornerRadius = 5
            let trackingArea = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: btn, userInfo: nil)
            btn.addTrackingArea(trackingArea)
            stack.addArrangedSubview(btn)
            btn.widthAnchor.constraint(equalToConstant: 192).isActive = true
            btn.heightAnchor.constraint(equalToConstant: 28).isActive = true
            buttons.append(btn)
        }

        addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 4),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 4),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -4),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
            widthAnchor.constraint(equalToConstant: 200),
        ])

        updateSelection()
    }

    func setLayout(_ layout: DashboardLayout) {
        currentLayout = layout
        updateSelection()
    }

    private func updateSelection() {
        for (i, btn) in buttons.enumerated() {
            let isActive = DashboardLayout.allCases[i] == currentLayout
            btn.font = isActive
                ? NSFont.systemFont(ofSize: 12, weight: .semibold)
                : NSFont.systemFont(ofSize: 12)
            btn.layer?.backgroundColor = isActive
                ? SemanticColors.accent.withAlphaComponent(0.12).cgColor
                : NSColor.clear.cgColor
        }
    }

    func toggle() {
        isHidden = !isHidden
    }

    func dismiss() {
        isHidden = true
    }

    @objc private func itemClicked(_ sender: NSButton) {
        let layout = DashboardLayout.allCases[sender.tag]
        currentLayout = layout
        updateSelection()
        dismiss()
        delegate?.layoutPopover(self, didSelect: layout)
    }
}
```

- [ ] **Step 2: Create TitleBarView**

Full title bar matching prototype.

```swift
// Sources/UI/TitleBar/TitleBarView.swift
import AppKit

protocol TitleBarDelegate: AnyObject {
    func titleBarDidSelectDashboard()
    func titleBarDidSelectProject(_ projectName: String)
    func titleBarDidRequestCloseProject(_ projectName: String)
    func titleBarDidRequestAddProject()
    func titleBarDidRequestNewThread()
    func titleBarDidSelectLayout(_ layout: DashboardLayout)
    func titleBarDidToggleNotifications()
    func titleBarDidToggleAI()
    func titleBarDidToggleTheme()
}

final class TitleBarView: NSView, LayoutPopoverDelegate {
    weak var delegate: TitleBarDelegate?

    // Left side
    private let trafficDots = NSStackView()
    private let tabsScroll = NSScrollView()
    private let tabsStack = NSStackView()

    // Right side
    let newThreadButton = NSButton(title: "New Thread", target: nil, action: nil)
    private let layoutPopover = LayoutPopoverView()
    private var viewMenuButton: NSButton!
    private var notifButton: NSButton!
    private let notifBadge = NSTextField(labelWithString: "2")
    private var aiButton: NSButton!
    private let themeButton = NSButton(title: "◐", target: nil, action: nil)

    // State
    var currentView: String = "dashboard" // "dashboard" or "project"
    var currentProject: String = ""
    var projects: [String] = []
    var projectStatusProvider: ((String) -> String)? // returns "running"/"waiting"/"error"/"idle"

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.backgroundColor = SemanticColors.panel.withAlphaComponent(0.88).cgColor

        // Traffic lights (decorative)
        for color in [NSColor(hex: 0xff5f57), NSColor(hex: 0xfebb2e), NSColor(hex: 0x28c840)] {
            let dot = NSView()
            dot.wantsLayer = true
            dot.layer?.cornerRadius = 5.5
            dot.layer?.backgroundColor = color.cgColor
            dot.widthAnchor.constraint(equalToConstant: 11).isActive = true
            dot.heightAnchor.constraint(equalToConstant: 11).isActive = true
            trafficDots.addArrangedSubview(dot)
        }
        trafficDots.orientation = .horizontal
        trafficDots.spacing = 6

        // Tabs
        tabsStack.orientation = .horizontal
        tabsStack.spacing = 8
        tabsStack.alignment = .centerY

        // Right buttons
        viewMenuButton = makeIconButton(svgName: "grid", action: #selector(viewMenuTapped))
        notifButton = makeIconButton(svgName: "bell", action: #selector(notifTapped))
        aiButton = makeIconButton(svgName: "sparkle", action: #selector(aiTapped))

        themeButton.target = self
        themeButton.action = #selector(themeTapped)
        themeButton.isBordered = false
        themeButton.font = NSFont.systemFont(ofSize: 16)
        themeButton.widthAnchor.constraint(equalToConstant: 32).isActive = true
        themeButton.heightAnchor.constraint(equalToConstant: 32).isActive = true

        newThreadButton.target = self
        newThreadButton.action = #selector(newThreadTapped)
        newThreadButton.font = NSFont.systemFont(ofSize: 11)
        newThreadButton.isBordered = true
        newThreadButton.bezelStyle = .rounded
        newThreadButton.isHidden = true

        // Notification badge
        notifBadge.font = NSFont.systemFont(ofSize: 9, weight: .bold)
        notifBadge.textColor = .white
        notifBadge.backgroundColor = SemanticColors.danger
        notifBadge.drawsBackground = true
        notifBadge.wantsLayer = true
        notifBadge.layer?.cornerRadius = 7
        notifBadge.alignment = .center
        notifBadge.isBezeled = false
        notifBadge.isEditable = false

        // Layout
        let leftStack = NSStackView(views: [trafficDots, tabsStack])
        leftStack.orientation = .horizontal
        leftStack.spacing = 10
        leftStack.alignment = .centerY

        layoutPopover.delegate = self
        addSubview(layoutPopover)

        let notifWrap = NSView()
        notifWrap.addSubview(notifButton)
        notifWrap.addSubview(notifBadge)
        notifButton.translatesAutoresizingMaskIntoConstraints = false
        notifBadge.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            notifButton.leadingAnchor.constraint(equalTo: notifWrap.leadingAnchor),
            notifButton.trailingAnchor.constraint(equalTo: notifWrap.trailingAnchor),
            notifButton.topAnchor.constraint(equalTo: notifWrap.topAnchor),
            notifButton.bottomAnchor.constraint(equalTo: notifWrap.bottomAnchor),
            notifBadge.topAnchor.constraint(equalTo: notifWrap.topAnchor, constant: 2),
            notifBadge.trailingAnchor.constraint(equalTo: notifWrap.trailingAnchor, constant: -2),
            notifBadge.widthAnchor.constraint(greaterThanOrEqualToConstant: 14),
            notifBadge.heightAnchor.constraint(equalToConstant: 14),
        ])

        let rightStack = NSStackView(views: [newThreadButton, viewMenuButton, notifWrap, aiButton, themeButton])
        rightStack.orientation = .horizontal
        rightStack.spacing = 8
        rightStack.alignment = .centerY

        addSubview(leftStack)
        addSubview(rightStack)
        leftStack.translatesAutoresizingMaskIntoConstraints = false
        rightStack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            leftStack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            leftStack.centerYAnchor.constraint(equalTo: centerYAnchor),
            rightStack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            rightStack.centerYAnchor.constraint(equalTo: centerYAnchor),
            heightAnchor.constraint(equalToConstant: 40),
        ])

        // Layout popover positioning
        layoutPopover.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            layoutPopover.topAnchor.constraint(equalTo: bottomAnchor, constant: 4),
            layoutPopover.trailingAnchor.constraint(equalTo: viewMenuButton.trailingAnchor),
        ])
    }

    func renderTabs() {
        tabsStack.arrangedSubviews.forEach { $0.removeFromSuperview() }

        // Dashboard tab (capsule)
        let dashTab = makeDashboardTab()
        tabsStack.addArrangedSubview(dashTab)

        // Separator
        let sep = NSView()
        sep.wantsLayer = true
        sep.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.75).cgColor
        sep.widthAnchor.constraint(equalToConstant: 1).isActive = true
        sep.heightAnchor.constraint(equalToConstant: 18).isActive = true
        tabsStack.addArrangedSubview(sep)

        // Project tabs
        for project in projects {
            let tab = makeProjectTab(project)
            tabsStack.addArrangedSubview(tab)
        }

        // Add tab
        let addTab = NSButton(title: "+", target: self, action: #selector(addProjectTapped))
        addTab.isBordered = false
        addTab.font = NSFont.systemFont(ofSize: 16)
        addTab.contentTintColor = SemanticColors.text
        addTab.wantsLayer = true
        addTab.layer?.cornerRadius = 7
        addTab.widthAnchor.constraint(equalToConstant: 30).isActive = true
        addTab.heightAnchor.constraint(equalToConstant: 28).isActive = true
        tabsStack.addArrangedSubview(addTab)

        // Visibility
        newThreadButton.isHidden = (currentView == "dashboard")
        viewMenuButton.alphaValue = (currentView == "dashboard") ? 1 : 0.3
        viewMenuButton.isEnabled = (currentView == "dashboard")
    }

    private func makeDashboardTab() -> NSButton {
        let isActive = currentView == "dashboard"
        let btn = NSButton(title: "Dashboard", target: self, action: #selector(dashboardTapped))
        btn.isBordered = false
        btn.font = NSFont.systemFont(ofSize: 11, weight: .semibold)
        btn.contentTintColor = SemanticColors.text
        btn.wantsLayer = true
        btn.layer?.cornerRadius = 14
        btn.heightAnchor.constraint(equalToConstant: 28).isActive = true
        if isActive {
            btn.layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.16).cgColor
            btn.layer?.borderWidth = 1
            btn.layer?.borderColor = SemanticColors.accent.withAlphaComponent(0.38).blended(withFraction: 0.62, of: SemanticColors.line)?.cgColor
        } else {
            btn.layer?.backgroundColor = SemanticColors.panel.withAlphaComponent(0.92).cgColor
            btn.layer?.borderWidth = 1
            btn.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.5).cgColor
        }
        return btn
    }

    private func makeProjectTab(_ project: String) -> NSView {
        let isActive = currentView == "project" && currentProject == project
        let container = NSView()
        container.wantsLayer = true
        container.layer?.cornerRadius = 7
        container.layer?.borderWidth = 1
        if isActive {
            container.layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.1).blended(withFraction: 0.9, of: SemanticColors.panel)?.cgColor
            container.layer?.borderColor = SemanticColors.accent.withAlphaComponent(0.45).blended(withFraction: 0.55, of: SemanticColors.line)?.cgColor
        } else {
            container.layer?.backgroundColor = SemanticColors.panel2.cgColor
            container.layer?.borderColor = SemanticColors.line.withAlphaComponent(0.65).cgColor
        }

        // Status dot
        let statusDot = NSView()
        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        let status = projectStatusProvider?(project) ?? "idle"
        statusDot.layer?.backgroundColor = statusColor(status).cgColor
        statusDot.widthAnchor.constraint(equalToConstant: 8).isActive = true
        statusDot.heightAnchor.constraint(equalToConstant: 8).isActive = true

        // Name
        let nameLabel = NSTextField(labelWithString: project)
        nameLabel.font = NSFont.systemFont(ofSize: 11)
        nameLabel.textColor = SemanticColors.text
        nameLabel.lineBreakMode = .byTruncatingTail

        // Close button
        let closeBtn = NSButton(title: "×", target: self, action: #selector(closeProjectTapped(_:)))
        closeBtn.tag = projects.firstIndex(of: project) ?? 0
        closeBtn.isBordered = false
        closeBtn.font = NSFont.systemFont(ofSize: 13)
        closeBtn.contentTintColor = SemanticColors.muted

        let stack = NSStackView(views: [statusDot, nameLabel, closeBtn])
        stack.orientation = .horizontal
        stack.spacing = 6
        stack.alignment = .centerY
        stack.edgeInsets = NSEdgeInsets(top: 5, left: 10, bottom: 5, right: 6)
        container.addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            stack.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            stack.topAnchor.constraint(equalTo: container.topAnchor),
            stack.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])

        // Click to select
        let click = NSClickGestureRecognizer(target: self, action: #selector(projectTabClicked(_:)))
        container.addGestureRecognizer(click)
        container.identifier = NSUserInterfaceItemIdentifier(project)

        return container
    }

    private func statusColor(_ status: String) -> NSColor {
        switch status {
        case "running": return SemanticColors.running
        case "waiting": return SemanticColors.waiting
        case "error": return SemanticColors.danger
        default: return SemanticColors.idle
        }
    }

    func updateNotifBadge(_ count: Int) {
        notifBadge.stringValue = "\(count)"
        notifBadge.isHidden = count == 0
    }

    func setCurrentLayout(_ layout: DashboardLayout) {
        layoutPopover.setLayout(layout)
    }

    private func makeIconButton(svgName: String, action: Selector) -> NSButton {
        let btn = NSButton(title: "", target: self, action: action)
        btn.isBordered = false
        btn.wantsLayer = true
        btn.layer?.cornerRadius = 10
        btn.widthAnchor.constraint(equalToConstant: 32).isActive = true
        btn.heightAnchor.constraint(equalToConstant: 32).isActive = true
        // Use SF Symbols or custom drawing
        switch svgName {
        case "grid":
            btn.image = NSImage(systemSymbolName: "square.grid.2x2", accessibilityDescription: "Layout")
        case "bell":
            btn.image = NSImage(systemSymbolName: "bell", accessibilityDescription: "Notifications")
        case "sparkle":
            btn.image = NSImage(systemSymbolName: "sparkles", accessibilityDescription: "AI Assistant")
        default: break
        }
        btn.imageScaling = .scaleProportionallyUpOrDown
        btn.contentTintColor = SemanticColors.text
        return btn
    }

    // MARK: - Actions
    @objc private func dashboardTapped() { delegate?.titleBarDidSelectDashboard() }
    @objc private func projectTabClicked(_ gesture: NSGestureRecognizer) {
        guard let id = gesture.view?.identifier?.rawValue else { return }
        delegate?.titleBarDidSelectProject(id)
    }
    @objc private func closeProjectTapped(_ sender: NSButton) {
        guard sender.tag < projects.count else { return }
        delegate?.titleBarDidRequestCloseProject(projects[sender.tag])
    }
    @objc private func addProjectTapped() { delegate?.titleBarDidRequestAddProject() }
    @objc private func newThreadTapped() { delegate?.titleBarDidRequestNewThread() }
    @objc private func viewMenuTapped() {
        layoutPopover.toggle()
    }
    @objc private func notifTapped() { delegate?.titleBarDidToggleNotifications() }
    @objc private func aiTapped() { delegate?.titleBarDidToggleAI() }
    @objc private func themeTapped() { delegate?.titleBarDidToggleTheme() }

    // MARK: - LayoutPopoverDelegate
    func layoutPopover(_ popover: LayoutPopoverView, didSelect layout: DashboardLayout) {
        delegate?.titleBarDidSelectLayout(layout)
    }
}
```

- [ ] **Step 3: Commit**

```
feat: add TitleBarView and LayoutPopoverView
```

---

## Task 6: Shared Agent Display Helpers

**Files:**
- Create: `Sources/UI/Shared/AgentDisplayHelpers.swift`

### Goal
Extract shared helpers used by all card/panel views to avoid duplication.

- [ ] **Step 1: Create AgentDisplayHelpers**

```swift
// Sources/UI/Shared/AgentDisplayHelpers.swift
import AppKit

enum AgentDisplayHelpers {
    static func statusColor(_ status: String) -> NSColor {
        switch status {
        case "running": return SemanticColors.running
        case "waiting": return SemanticColors.waiting
        case "error": return SemanticColors.danger
        default: return SemanticColors.idle
        }
    }

    static func compactDuration(_ hms: String) -> String {
        let parts = hms.split(separator: ":").compactMap { Int($0) }
        guard parts.count == 3 else { return hms }
        let (h, m, s) = (parts[0], parts[1], parts[2])
        if h > 0 { return "\(h)h\(String(format: "%02d", m))m" }
        if m > 0 { return "\(m)m\(String(format: "%02d", s))s" }
        return "\(s)s"
    }
}
```

- [ ] **Step 2: Commit**

```
feat: add shared AgentDisplayHelpers for status colors and duration formatting
```

---

## Task 7: Agent Card Views (AgentCardView + MiniCardView + FocusPanelView)

**Files:**
- Create: `Sources/UI/Dashboard/AgentCardView.swift`
- Create: `Sources/UI/Dashboard/MiniCardView.swift`
- Create: `Sources/UI/Dashboard/FocusPanelView.swift`

### Goal
Three card components matching prototype: Grid card (info only), mini card (16:9), focus panel (header + terminal).

- [ ] **Step 1: Create AgentCardView for Grid layout**

Information-only card for Grid: status dot + `project - thread` title, lastMessage (3 lines), compact duration.

```swift
// Sources/UI/Dashboard/AgentCardView.swift
import AppKit

protocol AgentCardDelegate: AnyObject {
    func agentCardClicked(agentId: String)
}

final class AgentCardView: NSView {
    weak var delegate: AgentCardDelegate?
    var agentId: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }

    private let statusDot = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let messageLabel = NSTextField(wrappingLabelWithString: "")
    private let timeLabel = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 10
        layer?.borderWidth = 1

        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.widthAnchor.constraint(equalToConstant: 8).isActive = true
        statusDot.heightAnchor.constraint(equalToConstant: 8).isActive = true

        titleLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1

        messageLabel.font = NSFont.systemFont(ofSize: 12)
        messageLabel.textColor = SemanticColors.muted
        messageLabel.maximumNumberOfLines = 3
        messageLabel.lineBreakMode = .byTruncatingTail

        timeLabel.font = NSFont.systemFont(ofSize: 12)
        timeLabel.textColor = SemanticColors.muted

        let titleRow = NSStackView(views: [statusDot, titleLabel])
        titleRow.orientation = .horizontal
        titleRow.spacing = 8
        titleRow.alignment = .centerY

        let stack = NSStackView(views: [titleRow, messageLabel, timeLabel])
        stack.orientation = .vertical
        stack.spacing = 8
        stack.alignment = .leading
        addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 11),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 10),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -10),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -11),
        ])

        let click = NSClickGestureRecognizer(target: self, action: #selector(clicked))
        addGestureRecognizer(click)

        updateAppearance()
    }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String) {
        agentId = id
        titleLabel.stringValue = "\(project) - \(thread)"
        messageLabel.stringValue = lastMessage
        timeLabel.stringValue = "Σ \(compactDuration(totalDuration)) · ⟳ \(compactDuration(roundDuration))"
        statusDot.layer?.backgroundColor = statusColor(status).cgColor
    }

    private func updateAppearance() {
        if isSelected {
            layer?.backgroundColor = SemanticColors.accent.withAlphaComponent(0.12).blended(withFraction: 0.88, of: SemanticColors.panel2)?.cgColor
            layer?.borderColor = SemanticColors.accent.withAlphaComponent(0.55).blended(withFraction: 0.45, of: SemanticColors.line)?.cgColor
        } else {
            layer?.backgroundColor = SemanticColors.panel2.cgColor
            layer?.borderColor = SemanticColors.line.withAlphaComponent(0.78).cgColor
        }
    }

    private func statusColor(_ status: String) -> NSColor {
        switch status {
        case "running": return SemanticColors.running
        case "waiting": return SemanticColors.waiting
        case "error": return SemanticColors.danger
        default: return SemanticColors.idle
        }
    }

    private func compactDuration(_ hms: String) -> String {
        let parts = hms.split(separator: ":").compactMap { Int($0) }
        guard parts.count == 3 else { return hms }
        let (h, m, s) = (parts[0], parts[1], parts[2])
        if h > 0 { return "\(h)h\(String(format: "%02d", m))m" }
        if m > 0 { return "\(m)m\(String(format: "%02d", s))s" }
        return "\(s)s"
    }

    @objc private func clicked() { delegate?.agentCardClicked(agentId: agentId) }
}
```

- [ ] **Step 2: Create MiniCardView for non-Grid layouts**

16:9 aspect ratio mini card.

```swift
// Sources/UI/Dashboard/MiniCardView.swift
import AppKit

final class MiniCardView: NSView {
    weak var delegate: AgentCardDelegate?
    var agentId: String = ""
    var isSelected: Bool = false { didSet { updateAppearance() } }

    private let statusDot = NSView()
    private let titleLabel = NSTextField(labelWithString: "")
    private let messageLabel = NSTextField(wrappingLabelWithString: "")
    private let timeLabel = NSTextField(labelWithString: "")

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 9
        layer?.borderWidth = 1

        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.widthAnchor.constraint(equalToConstant: 8).isActive = true
        statusDot.heightAnchor.constraint(equalToConstant: 8).isActive = true

        titleLabel.font = NSFont.systemFont(ofSize: 11, weight: .bold)
        titleLabel.textColor = SemanticColors.text
        titleLabel.lineBreakMode = .byTruncatingTail
        titleLabel.maximumNumberOfLines = 1

        messageLabel.font = NSFont.systemFont(ofSize: 11)
        messageLabel.textColor = SemanticColors.muted
        messageLabel.maximumNumberOfLines = 2

        timeLabel.font = NSFont.systemFont(ofSize: 10)
        timeLabel.textColor = SemanticColors.muted

        let titleRow = NSStackView(views: [statusDot, titleLabel])
        titleRow.orientation = .horizontal
        titleRow.spacing = 6
        titleRow.alignment = .centerY

        let stack = NSStackView(views: [titleRow, messageLabel, timeLabel])
        stack.orientation = .vertical
        stack.spacing = 6
        stack.alignment = .leading
        addSubview(stack)
        stack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            stack.topAnchor.constraint(equalTo: topAnchor, constant: 8),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -8),
            stack.bottomAnchor.constraint(lessThanOrEqualTo: bottomAnchor, constant: -8),
        ])

        // 16:9 aspect ratio
        widthAnchor.constraint(equalTo: heightAnchor, multiplier: 16.0/9.0).isActive = true

        let click = NSClickGestureRecognizer(target: self, action: #selector(clicked))
        addGestureRecognizer(click)

        updateAppearance()
    }

    func configure(id: String, project: String, thread: String, status: String, lastMessage: String, totalDuration: String, roundDuration: String) {
        agentId = id
        titleLabel.stringValue = "\(project) - \(thread)"
        messageLabel.stringValue = lastMessage
        timeLabel.stringValue = "Σ \(compactDuration(totalDuration)) · ⟳ \(compactDuration(roundDuration))"
        statusDot.layer?.backgroundColor = statusColor(status).cgColor
    }

    private func updateAppearance() {
        layer?.backgroundColor = SemanticColors.panel2.cgColor
        if isSelected {
            layer?.borderColor = SemanticColors.accent.withAlphaComponent(0.65).blended(withFraction: 0.35, of: SemanticColors.line)?.cgColor
        } else {
            layer?.borderColor = SemanticColors.line.withAlphaComponent(0.58).cgColor
        }
    }

    private func statusColor(_ status: String) -> NSColor {
        switch status {
        case "running": return SemanticColors.running
        case "waiting": return SemanticColors.waiting
        case "error": return SemanticColors.danger
        default: return SemanticColors.idle
        }
    }

    private func compactDuration(_ hms: String) -> String {
        let parts = hms.split(separator: ":").compactMap { Int($0) }
        guard parts.count == 3 else { return hms }
        let (h, m, s) = (parts[0], parts[1], parts[2])
        if h > 0 { return "\(h)h\(String(format: "%02d", m))m" }
        if m > 0 { return "\(m)m\(String(format: "%02d", s))s" }
        return "\(s)s"
    }

    @objc private func clicked() { delegate?.agentCardClicked(agentId: agentId) }
}
```

- [ ] **Step 3: Create FocusPanelView**

42px header + terminal area, with "Enter Project" button.

```swift
// Sources/UI/Dashboard/FocusPanelView.swift
import AppKit

protocol FocusPanelDelegate: AnyObject {
    func focusPanelDidRequestEnterProject(_ projectName: String)
}

final class FocusPanelView: NSView {
    weak var delegate: FocusPanelDelegate?

    private let headerView = NSView()
    private let statusDot = NSView()
    private let nameLabel = NSTextField(labelWithString: "")
    private let metaLabel = NSTextField(labelWithString: "")
    private let durationLabel = NSTextField(labelWithString: "")
    private let enterProjectButton = NSButton(title: "", target: nil, action: nil)
    let terminalContainer = NSView()
    private var projectName: String = ""

    override init(frame: NSRect) {
        super.init(frame: frame)
        setup()
    }

    required init?(coder: NSCoder) { fatalError() }

    private func setup() {
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        layer?.borderColor = SemanticColors.line.withAlphaComponent(0.38).cgColor
        layer?.backgroundColor = SemanticColors.panel2.cgColor

        // Header (42px)
        headerView.wantsLayer = true

        statusDot.wantsLayer = true
        statusDot.layer?.cornerRadius = 4
        statusDot.widthAnchor.constraint(equalToConstant: 8).isActive = true
        statusDot.heightAnchor.constraint(equalToConstant: 8).isActive = true

        nameLabel.font = NSFont.systemFont(ofSize: 12, weight: .bold)
        nameLabel.textColor = SemanticColors.text
        metaLabel.font = NSFont.systemFont(ofSize: 12)
        metaLabel.textColor = SemanticColors.muted
        durationLabel.font = NSFont.systemFont(ofSize: 12)
        durationLabel.textColor = SemanticColors.muted

        enterProjectButton.target = self
        enterProjectButton.action = #selector(enterProjectTapped)
        enterProjectButton.isBordered = false
        enterProjectButton.image = NSImage(systemSymbolName: "arrow.up.right", accessibilityDescription: "Enter Project")
        enterProjectButton.contentTintColor = SemanticColors.text
        enterProjectButton.wantsLayer = true
        enterProjectButton.layer?.cornerRadius = 8
        enterProjectButton.widthAnchor.constraint(equalToConstant: 28).isActive = true
        enterProjectButton.heightAnchor.constraint(equalToConstant: 28).isActive = true
        enterProjectButton.toolTip = "进入 Project"

        let headerLeft = NSStackView(views: [statusDot, nameLabel, metaLabel, durationLabel])
        headerLeft.orientation = .horizontal
        headerLeft.spacing = 8
        headerLeft.alignment = .centerY

        let headerStack = NSStackView(views: [headerLeft, enterProjectButton])
        headerStack.orientation = .horizontal
        headerStack.distribution = .fill
        headerStack.alignment = .centerY
        headerView.addSubview(headerStack)
        headerStack.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            headerStack.leadingAnchor.constraint(equalTo: headerView.leadingAnchor, constant: 10),
            headerStack.trailingAnchor.constraint(equalTo: headerView.trailingAnchor, constant: -10),
            headerStack.centerYAnchor.constraint(equalTo: headerView.centerYAnchor),
        ])
        headerLeft.setContentHuggingPriority(.defaultLow, for: .horizontal)

        // Bottom border on header
        let headerBorder = NSView()
        headerBorder.wantsLayer = true
        headerBorder.layer?.backgroundColor = SemanticColors.line.withAlphaComponent(0.55).cgColor
        headerView.addSubview(headerBorder)
        headerBorder.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            headerBorder.leadingAnchor.constraint(equalTo: headerView.leadingAnchor),
            headerBorder.trailingAnchor.constraint(equalTo: headerView.trailingAnchor),
            headerBorder.bottomAnchor.constraint(equalTo: headerView.bottomAnchor),
            headerBorder.heightAnchor.constraint(equalToConstant: 1),
        ])

        // Layout: header + terminal
        addSubview(headerView)
        addSubview(terminalContainer)
        headerView.translatesAutoresizingMaskIntoConstraints = false
        terminalContainer.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            headerView.topAnchor.constraint(equalTo: topAnchor),
            headerView.leadingAnchor.constraint(equalTo: leadingAnchor),
            headerView.trailingAnchor.constraint(equalTo: trailingAnchor),
            headerView.heightAnchor.constraint(equalToConstant: 42),
            terminalContainer.topAnchor.constraint(equalTo: headerView.bottomAnchor),
            terminalContainer.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalContainer.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalContainer.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    func configure(name: String, project: String, thread: String, status: String, total: String, round: String) {
        projectName = project
        nameLabel.stringValue = name
        metaLabel.stringValue = "\(project) · \(thread)"
        durationLabel.stringValue = "Total \(total) / Round \(round)"
        statusDot.layer?.backgroundColor = statusColor(status).cgColor
    }

    private func statusColor(_ status: String) -> NSColor {
        switch status {
        case "running": return SemanticColors.running
        case "waiting": return SemanticColors.waiting
        case "error": return SemanticColors.danger
        default: return SemanticColors.idle
        }
    }

    @objc private func enterProjectTapped() { delegate?.focusPanelDidRequestEnterProject(projectName) }
}
```

- [ ] **Step 4: Commit**

```
feat: add AgentCardView, MiniCardView, and FocusPanelView
```

---

## Task 8: Rewrite DashboardViewController — 4 Layouts

**Files:**
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (full rewrite)
- Delete: `Sources/UI/Dashboard/TerminalCardView.swift` (replaced by AgentCardView)

### Goal
Replace 2-mode (grid/spotlight) dashboard with 4-layout dashboard matching prototype. Grid retains zoom + drag-to-reorder. Other layouts use focus panel + mini cards.

- [ ] **Step 1: Rewrite DashboardViewController**

The rewritten controller manages 4 layout containers, card creation, agent sorting (waiting > running > others), and layout switching. Grid uses existing `GridLayout` + `DraggableGridView`. Other layouts use `FocusPanelView` + `MiniCardView` collections.

Key behaviors from prototype:
- `sortedAgents()`: waiting → running → others
- Grid click: directly enter Project view (delegate call)
- Other layout click: switch `selectedAgentId`, update focus panel
- Focus panel "Enter Project": enter project view (delegate call)
- Layout switch preserves selected agent and context

The full implementation should follow the prototype's `updateLayouts()` function logic, creating:
- `layout-grid`: Grid of `AgentCardView` with existing zoom/drag
- `layout-left-right`: 78%/22% split, left = `FocusPanelView`, right = vertical `MiniCardView` stack
- `layout-top-small`: `auto 1fr` rows, top = horizontal scrolling `MiniCardView` row, bottom = `FocusPanelView`
- `layout-top-large`: `1fr auto` rows, top = `FocusPanelView`, bottom = horizontal scrolling `MiniCardView` row

- [ ] **Step 2: Update DashboardDelegate protocol**

```swift
protocol DashboardDelegate: AnyObject {
    func dashboardDidSelectProject(_ project: String, thread: String)
    func dashboardDidRequestEnterProject(_ project: String)
    func dashboardDidReorderCards(order: [String])
    func dashboardDidRequestDeleteWorktree(_ path: String)
}
```

- [ ] **Step 3: Run build to verify**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -30`

- [ ] **Step 4: Commit**

```
feat: rewrite DashboardViewController with 4 layout modes
```

---

## Task 9: Simplify RepoViewController + Restyle Sidebar

**Files:**
- Modify: `Sources/UI/Repo/RepoViewController.swift`
- Modify: `Sources/UI/Repo/SidebarViewController.swift`
- Delete: `Sources/UI/Repo/TerminalSplitView.swift`
- Delete: `Sources/UI/Repo/SearchBarView.swift` (search handled by terminal directly)

### Goal
Simplify Project view: remove split panes, single immersive terminal. Restyle sidebar to match prototype (no color bar, accent border selection, thread name + status + lastMessage 2-line clamp).

- [ ] **Step 1: Simplify RepoViewController**

Replace TerminalSplitView with single terminal container. Remove split/close pane methods. Layout: 300px sidebar + 1fr terminal, matching prototype's `project-shell`.

- [ ] **Step 2: Restyle SidebarViewController**

Update row styling:
- Remove 2px color bar on left edge
- Selected row: accent-mixed border + light background (matching `.thread-item.active`)
- Each row: thread name (bold 12px) + status dot + lastMessage (2-line clamp, muted)
- No path display
- Empty state: "No thread yet. Click New Thread in titlebar."

- [ ] **Step 3: Remove TerminalSplitView.swift**

Delete the file and remove all references.

- [ ] **Step 4: Run build**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -30`

- [ ] **Step 5: Commit**

```
feat: simplify RepoViewController to single terminal, restyle sidebar
```

---

## Task 10: Rewrite MainWindowController — New Shell Layout

**Files:**
- Modify: `Sources/App/MainWindowController.swift`
- Delete: `Sources/UI/TabBar/TabBarView.swift` (replaced by TitleBarView)

### Goal
Replace the old window shell with the prototype's 3-row grid: TitleBar (40px) / Main (1fr) / StatusBar (32px). Integrate all new components: TitleBarView, DashboardVC, RepoVC, StatusBar, Modal, Notification/AI panels, backdrop. Wire up all delegate callbacks.

- [ ] **Step 1: Replace window setup**

- Remove old `TabBarView` usage, replace with `TitleBarView`
- Delete `Sources/UI/TabBar/TabBarView.swift`
- Hide real window traffic lights: `window.standardWindowButton(.closeButton)?.isHidden = true` (and miniaturize, zoom)
- Add `StatusBarView` at bottom
- Add `UnifiedModalView` as overlay (z-index 50 equivalent)
- Add `PanelBackdropView` (z-index 38), `NotificationPanelView`, `AIPanelView` (z-index 40) as overlays
- Panels: 360px wide, slide in/out via trailing constraint animation (0.22s ease)
- Set initial theme from config (`Theme.applyAppearance`)
- Layout: constraints matching `40px / 1fr / 32px`

- [ ] **Step 2: Implement TitleBarDelegate**

Wire all title bar actions:
- `titleBarDidSelectDashboard()`: set `activeTabIndex = 0`, embed dashboardVC
- `titleBarDidSelectProject(name)`: find matching `WorkspaceTab` by `displayName`, set `activeTabIndex`, embed cached/new `RepoViewController`
- `titleBarDidRequestCloseProject(name)`: show close confirmation modal, on confirm: call existing `tabBar(didCloseTabAt:)` logic (kill tmux sessions, remove surfaces, remove tab)
- `titleBarDidRequestAddProject()`: show add project modal, on confirm: extract last path segment as name, call existing `addWorkspace(path:)` logic
- `titleBarDidRequestNewThread()`: show multiline modal, on confirm: for each line, call `WorktreeCreator.createWorktree()`, add to sidebar
- `titleBarDidSelectLayout(layout)`: update `dashboardVC.currentLayout`, persist `config.dashboardLayout = layout.rawValue`, save config
- `titleBarDidToggleNotifications()`: toggle notification panel (close AI if open, toggle backdrop)
- `titleBarDidToggleAI()`: toggle AI panel (close notification if open, toggle backdrop)
- `titleBarDidToggleTheme()`: cycle `config.themeMode` (dark → system → light → dark), call `Theme.applyAppearance()`, save config, update status bar text

**Data mapping for DashboardDelegate:**
- `dashboardDidSelectProject(project, thread)`: find `WorkspaceTab` where `displayName == project`, find worktree where `branch == thread`, navigate to that project tab + select thread in sidebar
- `dashboardDidRequestEnterProject(project)`: same as `titleBarDidSelectProject(project)` — find tab, switch to it
- `dashboardDidReorderCards(order)`: same as existing `didReorderWorktrees` — save to `config.cardOrder`
- `dashboardDidRequestDeleteWorktree(path)`: same as existing delete logic

- [ ] **Step 3: Implement UnifiedModalDelegate**

Handle modal confirm/cancel for all 3 modal types (close project, add project, new thread).

- [ ] **Step 4: Implement panel mutual exclusion**

Opening notification panel closes AI panel and vice versa. Both close layout popover. Backdrop click closes both.

- [ ] **Step 5: Update status bar text**

- Dashboard: `Status: Dashboard ready · Focus {AgentName}`
- Project: `Status: {project} active · Thread {thread}`
- Theme switch: temporary status text update

- [ ] **Step 6: Update menu shortcuts**

- Remove split pane shortcuts (Cmd+Shift+D/E/W)
- Keep: Cmd+N (new branch/thread), Cmd+P (quick switch), Cmd+G (grid), Cmd+W (close tab), Cmd+F (find)
- Add: V key for layout cycle (when not in text field)

- [ ] **Step 7: Run full build and manual test**

Run: `xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -30`

- [ ] **Step 8: Commit**

```
feat: rewrite MainWindowController with new shell layout and panels
```

---

## Task 11: Clean Up and Verify Build

**Files:**
- Modify: `project.yml` (verify new dirs auto-discovered)
- Delete: `Tests/TerminalSplitViewTests.swift`
- Modify: `Tests/TerminalSurfaceReparentTests.swift` (remove TerminalSplitView references)

### Goal
Clean up test files referencing deleted components, verify clean build and tests pass.

Note: File deletions happen in their respective tasks:
- `TerminalCardView.swift` deleted in Task 7
- `TerminalSplitView.swift` deleted in Task 8
- `TabBarView.swift` deleted in Task 9

- [ ] **Step 1: Delete obsolete test files**

- Delete `Tests/TerminalSplitViewTests.swift`
- Update `Tests/TerminalSurfaceReparentTests.swift` to remove `TerminalSplitView` references

- [ ] **Step 2: Add `updateLayer()` to all views with `layer?.backgroundColor`**

All views that set `layer?.backgroundColor` in `setup()` need `updateLayer()` overrides to respond to theme changes: `TitleBarView`, `AgentCardView`, `MiniCardView`, `FocusPanelView`, `NotificationPanelView`, `AIPanelView`, `StatusBarView`.

- [ ] **Step 3: Clean build**

```bash
xcodegen generate
xcodebuild -project amux.xcodeproj -scheme amux clean
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -30
```

- [ ] **Step 4: Run tests**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -30
```

- [ ] **Step 5: Commit**

```
chore: clean up test files and add updateLayer for theme support
```

---

## Task 12: Integration Testing and Polish

**Files:**
- Various UI files for fixes

### Goal
Run the app, verify all interactions match prototype, fix visual/behavioral discrepancies.

- [ ] **Step 1: Verify Dashboard layouts**

Test all 4 layouts switch correctly, cards render with correct data, focus panel shows terminal.

- [ ] **Step 2: Verify Project workspace**

Test thread list selection, terminal display, New Thread modal (multiline).

- [ ] **Step 3: Verify panels**

Test notification panel slide-in/out, AI panel with placeholder responses, mutual exclusion, backdrop dismiss.

- [ ] **Step 4: Verify modals**

Test close project (with confirmation), add project (path → name extraction), new thread (multiline, dedup).

- [ ] **Step 5: Verify theme cycling**

Test dark → system → light → dark cycle, colors update across all components.

- [ ] **Step 6: Verify status bar**

Test status text updates on view/focus changes.

- [ ] **Step 7: Fix issues and commit**

```
fix: polish UI interactions to match prototype
```

---

## Task 13: Comprehensive UI Automation Tests

**Files:**
- Rewrite: `UITests/Pages/TabBarPage.swift` → rename to `TitleBarPage.swift`
- Rewrite: `UITests/Pages/DashboardPage.swift`
- Rewrite: `UITests/Pages/RepoPage.swift`
- Rewrite: `UITests/Pages/SidebarPage.swift`
- Rewrite: `UITests/Pages/DialogPage.swift`
- Create: `UITests/Pages/NotificationPanelPage.swift`
- Create: `UITests/Pages/AIPanelPage.swift`
- Create: `UITests/Pages/StatusBarPage.swift`
- Create: `UITests/Pages/LayoutPopoverPage.swift`
- Create: `UITests/Pages/ModalPage.swift`
- Modify: `UITests/Pages/AppPage.swift`
- Rewrite: `UITests/Tests/NavigationTests.swift`
- Rewrite: `UITests/Tests/TabBarTests.swift` → rename to `TitleBarTests.swift`
- Delete: `UITests/Tests/SplitPaneTests.swift` (feature removed)
- Delete: `UITests/Tests/SearchTests.swift` (search bar removed)
- Create: `UITests/Tests/DashboardLayoutTests.swift`
- Create: `UITests/Tests/ModalTests.swift`
- Create: `UITests/Tests/PanelTests.swift`
- Create: `UITests/Tests/ThemeTests.swift`
- Create: `UITests/Tests/ProjectWorkspaceTests.swift`
- Modify: `UITests/Tests/ShortcutTests.swift`
- Modify: `UITests/Tests/WorktreeTests.swift`

### Goal
Rewrite and extend the UI test suite to cover all new interactions from the prototype. Follow existing Page Object pattern. Each test should verify one specific behavior.

### Accessibility Identifiers Required

Before writing tests, the following identifiers must be set on UI components (add during Task 10 or earlier):

```
// TitleBar
"titlebar"
"titlebar.dashboardTab"
"titlebar.separator"
"titlebar.projectTab.<name>"
"titlebar.projectTab.<name>.statusDot"
"titlebar.projectTab.<name>.close"
"titlebar.addProject"
"titlebar.newThread"
"titlebar.viewMenu"
"titlebar.notifButton"
"titlebar.notifBadge"
"titlebar.aiButton"
"titlebar.themeToggle"

// Layout Popover
"layout.popover"
"layout.item.grid"
"layout.item.left-right"
"layout.item.top-small"
"layout.item.top-large"

// Dashboard
"dashboard.view"
"dashboard.layout.grid"
"dashboard.layout.left-right"
"dashboard.layout.top-small"
"dashboard.layout.top-large"
"dashboard.card.<id>"
"dashboard.miniCard.<id>"
"dashboard.focusPanel"
"dashboard.focusPanel.enterProject"
"dashboard.focusPanel.terminal"

// Project Workspace
"project.view"
"project.threadList"
"project.threadItem.<name>"
"project.terminal"
"project.emptyState"

// Panels
"panel.backdrop"
"panel.notification"
"panel.notification.close"
"panel.notification.item.<index>"
"panel.ai"
"panel.ai.close"
"panel.ai.messages"
"panel.ai.input"
"panel.ai.send"
"panel.ai.bubble.<index>"

// Modal
"modal.overlay"
"modal.title"
"modal.subtitle"
"modal.input"
"modal.cancel"
"modal.confirm"

// Status Bar
"statusbar"
"statusbar.summary"

// Sidebar (updated)
"sidebar.worktreeList"
"sidebar.row.<name>"
"sidebar.row.<name>.statusDot"
```

- [ ] **Step 1: Update Page Objects**

Create/rewrite the following Page Objects:

```swift
// UITests/Pages/TitleBarPage.swift
import XCTest

final class TitleBarPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var titleBar: XCUIElement { app.groups["titlebar"] }
    var dashboardTab: XCUIElement { app.buttons["titlebar.dashboardTab"] }
    var addProjectButton: XCUIElement { app.buttons["titlebar.addProject"] }
    var newThreadButton: XCUIElement { app.buttons["titlebar.newThread"] }
    var viewMenuButton: XCUIElement { app.buttons["titlebar.viewMenu"] }
    var notifButton: XCUIElement { app.buttons["titlebar.notifButton"] }
    var notifBadge: XCUIElement { app.staticTexts["titlebar.notifBadge"] }
    var aiButton: XCUIElement { app.buttons["titlebar.aiButton"] }
    var themeToggle: XCUIElement { app.buttons["titlebar.themeToggle"] }

    func projectTab(named name: String) -> XCUIElement {
        app.groups["titlebar.projectTab.\(name)"]
    }
    func projectTabStatusDot(named name: String) -> XCUIElement {
        app.groups["titlebar.projectTab.\(name).statusDot"]
    }
    func closeProjectTab(named name: String) {
        app.buttons["titlebar.projectTab.\(name).close"].waitAndClick()
    }
    func clickDashboardTab() { dashboardTab.waitAndClick() }
    func clickProjectTab(named name: String) { projectTab(named: name).waitAndClick() }
    func clickAddProject() { addProjectButton.waitAndClick() }
    func clickNewThread() { newThreadButton.waitAndClick() }
    func clickViewMenu() { viewMenuButton.waitAndClick() }
    func clickNotif() { notifButton.waitAndClick() }
    func clickAI() { aiButton.waitAndClick() }
    func clickTheme() { themeToggle.waitAndClick() }
}
```

```swift
// UITests/Pages/LayoutPopoverPage.swift
import XCTest

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
```

```swift
// UITests/Pages/DashboardPage.swift (rewritten)
import XCTest

final class DashboardPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var dashboardView: XCUIElement { app.groups["dashboard.view"] }
    var gridLayout: XCUIElement { app.groups["dashboard.layout.grid"] }
    var leftRightLayout: XCUIElement { app.groups["dashboard.layout.left-right"] }
    var topSmallLayout: XCUIElement { app.groups["dashboard.layout.top-small"] }
    var topLargeLayout: XCUIElement { app.groups["dashboard.layout.top-large"] }
    var focusPanel: XCUIElement { app.groups["dashboard.focusPanel"] }
    var enterProjectButton: XCUIElement { app.buttons["dashboard.focusPanel.enterProject"] }

    var cards: XCUIElementQuery { app.groups.matching(NSPredicate(format: "identifier BEGINSWITH 'dashboard.card.'")) }
    var miniCards: XCUIElementQuery { app.groups.matching(NSPredicate(format: "identifier BEGINSWITH 'dashboard.miniCard.'")) }

    func card(id: String) -> XCUIElement { app.groups["dashboard.card.\(id)"] }
    func miniCard(id: String) -> XCUIElement { app.groups["dashboard.miniCard.\(id)"] }
    func tapCard(id: String) { card(id: id).waitAndClick() }
    func tapMiniCard(id: String) { miniCard(id: id).waitAndClick() }
    func tapEnterProject() { enterProjectButton.waitAndClick() }
}
```

```swift
// UITests/Pages/ModalPage.swift
import XCTest

final class ModalPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var overlay: XCUIElement { app.groups["modal.overlay"] }
    var title: XCUIElement { app.staticTexts["modal.title"] }
    var subtitle: XCUIElement { app.staticTexts["modal.subtitle"] }
    var input: XCUIElement { app.textFields["modal.input"] }
    var textArea: XCUIElement { app.textViews["modal.input"] }
    var cancelButton: XCUIElement { app.buttons["modal.cancel"] }
    var confirmButton: XCUIElement { app.buttons["modal.confirm"] }

    var isVisible: Bool { overlay.waitForExistence(timeout: 3) }

    func typeInInput(_ text: String) {
        input.waitAndClick()
        input.typeText(text)
    }

    func typeInTextArea(_ text: String) {
        textArea.waitAndClick()
        textArea.typeText(text)
    }

    func confirm() { confirmButton.waitAndClick() }
    func cancel() { cancelButton.waitAndClick() }

    func dismissWithEscape() {
        app.typeKey(.escape, modifierFlags: [])
    }
}
```

```swift
// UITests/Pages/NotificationPanelPage.swift
import XCTest

final class NotificationPanelPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var panel: XCUIElement { app.groups["panel.notification"] }
    var closeButton: XCUIElement { app.buttons["panel.notification.close"] }
    var backdrop: XCUIElement { app.groups["panel.backdrop"] }

    var isOpen: Bool { panel.waitForExistence(timeout: 2) && panel.frame.minX < app.frame.maxX }

    func notifItem(at index: Int) -> XCUIElement {
        app.buttons["panel.notification.item.\(index)"]
    }

    func close() { closeButton.waitAndClick() }
    func clickBackdrop() { backdrop.waitAndClick() }
}
```

```swift
// UITests/Pages/AIPanelPage.swift
import XCTest

final class AIPanelPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var panel: XCUIElement { app.groups["panel.ai"] }
    var closeButton: XCUIElement { app.buttons["panel.ai.close"] }
    var inputField: XCUIElement { app.textFields["panel.ai.input"] }
    var sendButton: XCUIElement { app.buttons["panel.ai.send"] }
    var messages: XCUIElement { app.groups["panel.ai.messages"] }

    var isOpen: Bool { panel.waitForExistence(timeout: 2) && panel.frame.minX < app.frame.maxX }

    func sendMessage(_ text: String) {
        inputField.waitAndClick()
        inputField.typeText(text)
        sendButton.waitAndClick()
    }

    func close() { closeButton.waitAndClick() }
}
```

```swift
// UITests/Pages/StatusBarPage.swift
import XCTest

final class StatusBarPage {
    let app: XCUIApplication

    init(_ app: XCUIApplication) { self.app = app }

    var statusBar: XCUIElement { app.groups["statusbar"] }
    var summary: XCUIElement { app.staticTexts["statusbar.summary"] }

    var summaryText: String { summary.label }
}
```

```swift
// UITests/Pages/AppPage.swift (updated)
import XCTest

final class AppPage {
    let app: XCUIApplication

    lazy var titleBar = TitleBarPage(app)
    lazy var layoutPopover = LayoutPopoverPage(app)
    lazy var dashboard = DashboardPage(app)
    lazy var sidebar = SidebarPage(app)
    lazy var settings = SettingsPage(app)
    lazy var modal = ModalPage(app)
    lazy var notifPanel = NotificationPanelPage(app)
    lazy var aiPanel = AIPanelPage(app)
    lazy var statusBar = StatusBarPage(app)

    init(_ app: XCUIApplication) { self.app = app }

    func launch() {
        app.launchArguments += ["-UITestConfig", "/tmp/amux-uitest-config.json"]
        app.launch()
    }

    func terminate() { app.terminate() }
}
```

- [ ] **Step 2: Write TitleBarTests**

```swift
// UITests/Tests/TitleBarTests.swift
import XCTest

final class TitleBarTests: AmuxUITestCase {
    // MARK: - Dashboard Tab
    func testDashboardTabAlwaysVisible() {
        XCTAssertTrue(page.titleBar.dashboardTab.exists)
    }

    func testDashboardTabIsActiveOnLaunch() {
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5))
    }

    func testDashboardTabHasNoCLoseButton() {
        // Dashboard tab should not have a close button
        XCTAssertFalse(page.titleBar.dashboardTab.buttons["×"].exists)
    }

    // MARK: - Project Tabs
    func testProjectTabsShowStatusDot() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        XCTAssertTrue(page.titleBar.projectTabStatusDot(named: "project 1").exists)
    }

    func testClickProjectTabSwitchesToProjectView() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }

    func testClickDashboardTabReturnsToDashboard() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5))
    }

    // MARK: - Close Project
    func testCloseProjectTabShowsConfirmation() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.closeProjectTab(named: "project 1")
        XCTAssertTrue(page.modal.isVisible)
        XCTAssertEqual(page.modal.title.label, "关闭 Project")
    }

    func testCloseProjectCancelKeepsTab() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.closeProjectTab(named: "project 1")
        page.modal.cancel()
        XCTAssertTrue(page.titleBar.projectTab(named: "project 1").exists)
    }

    func testCloseProjectConfirmRemovesTab() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.closeProjectTab(named: "project 1")
        page.modal.confirm()
        page.titleBar.projectTab(named: "project 1").waitForNonExistence(timeout: 5)
        XCTAssertFalse(page.titleBar.projectTab(named: "project 1").exists)
    }

    func testCloseActiveProjectFallsToDashboard() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        page.titleBar.closeProjectTab(named: "project 1")
        page.modal.confirm()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5))
    }

    // MARK: - Add Project
    func testAddProjectButtonShowsModal() {
        page.titleBar.clickAddProject()
        XCTAssertTrue(page.modal.isVisible)
        XCTAssertEqual(page.modal.title.label, "添加 Project")
    }

    func testAddProjectCreatesNewTab() {
        page.titleBar.clickAddProject()
        page.modal.typeInInput("/Users/me/workspace/new-project")
        page.modal.confirm()
        XCTAssertTrue(page.titleBar.projectTab(named: "new-project").waitForExistence(timeout: 5))
    }

    // MARK: - New Thread (only visible in Project view)
    func testNewThreadHiddenInDashboard() {
        XCTAssertFalse(page.titleBar.newThreadButton.isHittable)
    }

    func testNewThreadVisibleInProjectView() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        XCTAssertTrue(page.titleBar.newThreadButton.waitForExistence(timeout: 5))
    }

    // MARK: - Notification Badge
    func testNotificationBadgeVisible() {
        XCTAssertTrue(page.titleBar.notifBadge.exists)
    }
}
```

- [ ] **Step 3: Write DashboardLayoutTests**

```swift
// UITests/Tests/DashboardLayoutTests.swift
import XCTest

final class DashboardLayoutTests: AmuxUITestCase {
    // MARK: - Default Layout
    func testDefaultLayoutIsLeftRight() {
        XCTAssertTrue(page.dashboard.leftRightLayout.waitForExistence(timeout: 5))
        XCTAssertTrue(page.dashboard.focusPanel.exists)
    }

    // MARK: - Layout Switching
    func testViewMenuOpensPopover() {
        page.titleBar.clickViewMenu()
        XCTAssertTrue(page.layoutPopover.popover.waitForExistence(timeout: 3))
    }

    func testSwitchToGrid() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectGrid()
        XCTAssertTrue(page.dashboard.gridLayout.waitForExistence(timeout: 3))
    }

    func testSwitchToTopSmall() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectTopSmall()
        XCTAssertTrue(page.dashboard.topSmallLayout.waitForExistence(timeout: 3))
    }

    func testSwitchToTopLarge() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectTopLarge()
        XCTAssertTrue(page.dashboard.topLargeLayout.waitForExistence(timeout: 3))
    }

    func testSwitchBackToLeftRight() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectGrid()
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectLeftRight()
        XCTAssertTrue(page.dashboard.leftRightLayout.waitForExistence(timeout: 3))
    }

    // MARK: - Grid Layout
    func testGridShowsCards() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectGrid()
        XCTAssertGreaterThan(page.dashboard.cards.count, 0)
    }

    func testGridCardClickEntersProject() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectGrid()
        guard page.dashboard.cards.firstMatch.waitForExistence(timeout: 5) else { return }
        page.dashboard.cards.firstMatch.click()
        // Should navigate to Project view
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }

    // MARK: - Non-Grid Layout (card clicks change focus)
    func testLeftRightMiniCardClickChangesFocus() {
        guard page.dashboard.miniCards.firstMatch.waitForExistence(timeout: 5) else { return }
        let firstMiniCard = page.dashboard.miniCards.firstMatch
        firstMiniCard.click()
        // Focus panel should update (stays on dashboard)
        XCTAssertTrue(page.dashboard.focusPanel.exists)
        XCTAssertTrue(page.dashboard.dashboardView.exists) // Still on dashboard
    }

    // MARK: - Focus Panel
    func testFocusPanelHasEnterProjectButton() {
        XCTAssertTrue(page.dashboard.focusPanel.waitForExistence(timeout: 5))
        XCTAssertTrue(page.dashboard.enterProjectButton.exists)
    }

    func testFocusPanelEnterProjectNavigates() {
        guard page.dashboard.focusPanel.waitForExistence(timeout: 5) else { return }
        page.dashboard.tapEnterProject()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }

    // MARK: - Layout popover dismissal
    func testPopoverClosesOnSelection() {
        page.titleBar.clickViewMenu()
        XCTAssertTrue(page.layoutPopover.popover.waitForExistence(timeout: 3))
        page.layoutPopover.selectGrid()
        page.layoutPopover.popover.waitForNonExistence(timeout: 3)
    }

    func testPopoverClosesOnClickOutside() {
        page.titleBar.clickViewMenu()
        XCTAssertTrue(page.layoutPopover.popover.waitForExistence(timeout: 3))
        page.dashboard.dashboardView.click() // click outside
        page.layoutPopover.popover.waitForNonExistence(timeout: 3)
    }

    // MARK: - View menu hidden in Project
    func testViewMenuHiddenInProjectView() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        XCTAssertFalse(page.titleBar.viewMenuButton.isEnabled)
    }
}
```

- [ ] **Step 4: Write PanelTests**

```swift
// UITests/Tests/PanelTests.swift
import XCTest

final class PanelTests: AmuxUITestCase {
    // MARK: - Notification Panel
    func testNotifButtonOpensPanel() {
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen)
    }

    func testNotifPanelCloseButton() {
        page.titleBar.clickNotif()
        page.notifPanel.close()
        XCTAssertFalse(page.notifPanel.isOpen)
    }

    func testNotifPanelBackdropDismiss() {
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.backdrop.waitForExistence(timeout: 3))
        page.notifPanel.clickBackdrop()
        XCTAssertFalse(page.notifPanel.isOpen)
    }

    func testNotifPanelHasItems() {
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.notifItem(at: 0).waitForExistence(timeout: 3))
    }

    // MARK: - AI Panel
    func testAIButtonOpensPanel() {
        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen)
    }

    func testAIPanelCloseButton() {
        page.titleBar.clickAI()
        page.aiPanel.close()
        XCTAssertFalse(page.aiPanel.isOpen)
    }

    func testAIPanelSendMessage() {
        page.titleBar.clickAI()
        page.aiPanel.sendMessage("Hello")
        // Should see user bubble + assistant response after ~450ms
        sleep(1)
        let bubbles = page.aiPanel.messages.staticTexts
        XCTAssertGreaterThanOrEqual(bubbles.count, 3) // welcome + user + assistant
    }

    // MARK: - Mutual Exclusion
    func testOpeningNotifClosesAI() {
        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen)
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen)
        XCTAssertFalse(page.aiPanel.isOpen)
    }

    func testOpeningAIClosesNotif() {
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen)
        page.titleBar.clickAI()
        XCTAssertTrue(page.aiPanel.isOpen)
        XCTAssertFalse(page.notifPanel.isOpen)
    }

    func testOpeningPanelClosesLayoutPopover() {
        page.titleBar.clickViewMenu()
        XCTAssertTrue(page.layoutPopover.popover.waitForExistence(timeout: 3))
        page.titleBar.clickNotif()
        page.layoutPopover.popover.waitForNonExistence(timeout: 3)
    }
}
```

- [ ] **Step 5: Write ModalTests**

```swift
// UITests/Tests/ModalTests.swift
import XCTest

final class ModalTests: AmuxUITestCase {
    // MARK: - Close Project Modal
    func testCloseProjectModalElements() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.closeProjectTab(named: "project 1")
        XCTAssertTrue(page.modal.isVisible)
        XCTAssertEqual(page.modal.title.label, "关闭 Project")
        XCTAssertTrue(page.modal.cancelButton.exists)
        XCTAssertTrue(page.modal.confirmButton.exists)
    }

    func testCloseProjectEscapeDismisses() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.closeProjectTab(named: "project 1")
        page.modal.dismissWithEscape()
        page.modal.overlay.waitForNonExistence(timeout: 3)
    }

    // MARK: - Add Project Modal
    func testAddProjectModalElements() {
        page.titleBar.clickAddProject()
        XCTAssertTrue(page.modal.isVisible)
        XCTAssertEqual(page.modal.title.label, "添加 Project")
        XCTAssertTrue(page.modal.input.exists)
    }

    func testAddProjectEmptyInputNoAction() {
        page.titleBar.clickAddProject()
        page.modal.confirm()
        // Modal should stay open (empty input)
        XCTAssertTrue(page.modal.isVisible)
    }

    // MARK: - New Thread Modal (multiline)
    func testNewThreadModalIsMultiline() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        page.titleBar.clickNewThread()
        XCTAssertTrue(page.modal.isVisible)
        XCTAssertEqual(page.modal.title.label, "New Thread")
        XCTAssertTrue(page.modal.textArea.exists) // multiline textarea
    }

    // MARK: - Modal closes panels
    func testModalClosesNotificationPanel() {
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen)
        page.titleBar.clickAddProject()
        XCTAssertFalse(page.notifPanel.isOpen)
        XCTAssertTrue(page.modal.isVisible)
    }
}
```

- [ ] **Step 6: Write ProjectWorkspaceTests**

```swift
// UITests/Tests/ProjectWorkspaceTests.swift
import XCTest

final class ProjectWorkspaceTests: AmuxUITestCase {
    private func enterProject() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
    }

    // MARK: - Layout
    func testProjectViewShowsThreadListAndTerminal() {
        enterProject()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
        // Terminal should also exist
        let terminal = page.app.groups["project.terminal"]
        XCTAssertTrue(terminal.waitForExistence(timeout: 5))
    }

    // MARK: - Thread Selection
    func testClickingThreadSwitchesTerminal() {
        enterProject()
        guard page.sidebar.worktreeList.waitForExistence(timeout: 5) else { return }
        let rows = page.sidebar.worktreeList.cells
        guard rows.count > 1 else { return }
        rows.element(boundBy: 1).click()
        // Terminal should update (content changes)
        let terminal = page.app.groups["project.terminal"]
        XCTAssertTrue(terminal.exists)
    }

    // MARK: - New Thread creates thread
    func testNewThreadAddsToList() {
        enterProject()
        let initialCount = page.sidebar.worktreeList.cells.count
        page.titleBar.clickNewThread()
        page.modal.typeInTextArea("feature/new-test-branch")
        page.modal.confirm()
        sleep(2) // wait for creation
        let newCount = page.sidebar.worktreeList.cells.count
        XCTAssertGreaterThan(newCount, initialCount)
    }

    // MARK: - Empty state
    func testEmptyProjectShowsHint() {
        // Add a new empty project
        page.titleBar.clickAddProject()
        page.modal.typeInInput("/tmp/empty-test-project")
        page.modal.confirm()
        let emptyState = page.app.staticTexts["project.emptyState"]
        XCTAssertTrue(emptyState.waitForExistence(timeout: 5))
    }
}
```

- [ ] **Step 7: Write ThemeTests**

```swift
// UITests/Tests/ThemeTests.swift
import XCTest

final class ThemeTests: AmuxUITestCase {
    func testThemeToggleExists() {
        XCTAssertTrue(page.titleBar.themeToggle.exists)
    }

    func testThemeToggleUpdatesStatusBar() {
        page.titleBar.clickTheme()
        let summary = page.statusBar.summary
        XCTAssertTrue(summary.waitForExistence(timeout: 3))
        // Status bar should mention theme change
        let text = summary.label
        XCTAssertTrue(text.contains("Theme") || text.contains("theme"))
    }

    func testThemeCyclesThreeStates() {
        // Click 3 times to cycle: system → light → dark → system
        page.titleBar.clickTheme()
        sleep(1)
        page.titleBar.clickTheme()
        sleep(1)
        page.titleBar.clickTheme()
        sleep(1)
        // Should be back to initial state — app still running
        XCTAssertTrue(page.dashboard.dashboardView.exists)
    }
}
```

- [ ] **Step 8: Update ShortcutTests**

Remove split pane and search shortcuts, add new shortcuts:

```swift
// UITests/Tests/ShortcutTests.swift (updated)
import XCTest

final class ShortcutTests: AmuxUITestCase {
    func testCmdCommaOpensSettings() {
        page.app.typeKey(",", modifierFlags: .command)
        XCTAssertTrue(page.settings.sheet.waitForExistence(timeout: 5))
    }

    func testCmdPOpensQuickSwitcher() {
        page.app.typeKey("p", modifierFlags: .command)
        let qs = page.app.groups["quickSwitcher"]
        XCTAssertTrue(qs.waitForExistence(timeout: 5))
    }

    func testCmdNOpensNewBranchDialog() {
        page.app.typeKey("n", modifierFlags: .command)
        // Should open new branch or new thread dialog
        XCTAssertTrue(page.modal.isVisible || page.app.sheets.firstMatch.waitForExistence(timeout: 5))
    }

    func testEscClosesModal() {
        page.titleBar.clickAddProject()
        XCTAssertTrue(page.modal.isVisible)
        page.app.typeKey(.escape, modifierFlags: [])
        page.modal.overlay.waitForNonExistence(timeout: 3)
    }

    func testEscClosesNotificationPanel() {
        page.titleBar.clickNotif()
        XCTAssertTrue(page.notifPanel.isOpen)
        page.app.typeKey(.escape, modifierFlags: [])
        sleep(1)
        XCTAssertFalse(page.notifPanel.isOpen)
    }
}
```

- [ ] **Step 9: Update NavigationTests**

```swift
// UITests/Tests/NavigationTests.swift (updated)
import XCTest

final class NavigationTests: AmuxUITestCase {
    func testDashboardAppearsOnLaunch() {
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 10))
    }

    func testTitleBarExists() {
        XCTAssertTrue(page.titleBar.titleBar.waitForExistence(timeout: 5))
    }

    func testStatusBarExists() {
        XCTAssertTrue(page.statusBar.statusBar.waitForExistence(timeout: 5))
    }

    func testStatusBarShowsDashboardStatus() {
        let text = page.statusBar.summaryText
        XCTAssertTrue(text.contains("Dashboard"))
    }

    func testNavigateToProjectAndBack() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))

        page.titleBar.clickDashboardTab()
        XCTAssertTrue(page.dashboard.dashboardView.waitForExistence(timeout: 5))
    }

    func testStatusBarUpdatesOnViewSwitch() {
        guard page.titleBar.projectTab(named: "project 1").waitForExistence(timeout: 5) else { return }
        page.titleBar.clickProjectTab(named: "project 1")
        sleep(1)
        let text = page.statusBar.summaryText
        XCTAssertTrue(text.contains("project 1"))
    }

    func testGridCardNavigatesToProject() {
        page.titleBar.clickViewMenu()
        page.layoutPopover.selectGrid()
        guard page.dashboard.cards.firstMatch.waitForExistence(timeout: 5) else { return }
        page.dashboard.cards.firstMatch.click()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }

    func testFocusPanelEnterProjectNavigates() {
        guard page.dashboard.focusPanel.waitForExistence(timeout: 5) else { return }
        page.dashboard.tapEnterProject()
        XCTAssertTrue(page.sidebar.worktreeList.waitForExistence(timeout: 5))
    }
}
```

- [ ] **Step 10: Delete obsolete tests and commit**

Delete:
- `UITests/Tests/SplitPaneTests.swift`
- `UITests/Tests/SearchTests.swift`
- `UITests/Pages/TabBarPage.swift` (replaced by TitleBarPage)

```
test: comprehensive UI automation tests for redesigned dashboard
```

- [ ] **Step 11: Run UI tests**

```bash
xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxUITests -configuration Debug test 2>&1 | tail -40
```

Fix any failures and re-run until green.

- [ ] **Step 12: Commit fixes**

```
fix: resolve UI test failures
```
