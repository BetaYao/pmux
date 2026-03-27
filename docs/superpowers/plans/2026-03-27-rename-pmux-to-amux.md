# Rename amux → AMUX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the project from "amux" to "amux" (AMUX — Agent Multiplexer) across all code, config, build files, and assets.

**Architecture:** Mechanical find-and-replace across 4 categories: (1) build system files, (2) source code identifiers and strings, (3) tests and UI tests, (4) scripts and docs. Plus icon asset generation from the new SVG draft.

**Tech Stack:** XcodeGen, xcodebuild, Quick Look (qlmanage) for SVG→PNG, sips for resizing.

---

### Task 1: Rename bridging header file

**Files:**
- Rename: `amux-Bridging-Header.h` → `amux-Bridging-Header.h`

- [ ] **Step 1: Rename the file**

```bash
git mv amux-Bridging-Header.h amux-Bridging-Header.h
```

- [ ] **Step 2: Commit**

```bash
git add amux-Bridging-Header.h
git commit -m "chore: rename bridging header amux → amux"
```

---

### Task 2: Rename UITest helper file

**Files:**
- Rename: `UITests/Helpers/AmuxUITestCase.swift` → `UITests/Helpers/AmuxUITestCase.swift`

- [ ] **Step 1: Rename the file**

```bash
git mv UITests/Helpers/AmuxUITestCase.swift UITests/Helpers/AmuxUITestCase.swift
```

- [ ] **Step 2: Rename the class inside the file**

In `UITests/Helpers/AmuxUITestCase.swift`, replace:
```swift
class AmuxUITestCase: XCTestCase {
```
with:
```swift
class AmuxUITestCase: XCTestCase {
```

- [ ] **Step 3: Update all references to AmuxUITestCase**

In these files, replace `AmuxUITestCase` → `AmuxUITestCase`:
- `UITests/Tests/PasteRegressionTests.swift`
- `UITests/Tests/CoreTests.swift`
- `UITests/Tests/RegressionTests.swift`
- `UITests/Tests/SplitPaneTests.swift`
- `UITests/Tests/SmokeTests.swift`

- [ ] **Step 4: Commit**

```bash
git add UITests/
git commit -m "chore: rename AmuxUITestCase → AmuxUITestCase"
```

---

### Task 3: Update project.yml

**Files:**
- Modify: `project.yml`

- [ ] **Step 1: Update project.yml**

Apply these replacements throughout `project.yml`:

| Old | New |
|-----|-----|
| `name: amux` | `name: amux` |
| `bundleIdPrefix: com.amux` | `bundleIdPrefix: com.amux` |
| `amux:` (target name, line 11) | `amux:` |
| `PRODUCT_BUNDLE_IDENTIFIER: com.amux.app` | `PRODUCT_BUNDLE_IDENTIFIER: com.amux.app` |
| `PRODUCT_NAME: amux` | `PRODUCT_NAME: amux` |
| `SWIFT_OBJC_BRIDGING_HEADER: "$(PROJECT_DIR)/amux-Bridging-Header.h"` | `SWIFT_OBJC_BRIDGING_HEADER: "$(PROJECT_DIR)/amux-Bridging-Header.h"` |
| `amuxTests:` (target name, line 56) | `amuxTests:` |
| `PRODUCT_BUNDLE_IDENTIFIER: com.amux.tests` | `PRODUCT_BUNDLE_IDENTIFIER: com.amux.tests` |
| `- target: amux` (dependency lines) | `- target: amux` |
| `amuxUITests:` (target name, line 69) | `amuxUITests:` |
| `PRODUCT_BUNDLE_IDENTIFIER: com.amux.uitests` | `PRODUCT_BUNDLE_IDENTIFIER: com.amux.uitests` |
| `TEST_TARGET_NAME: amux` | `TEST_TARGET_NAME: amux` |

- [ ] **Step 2: Regenerate Xcode project**

```bash
xcodegen generate
```

- [ ] **Step 3: Commit**

```bash
git add project.yml amux.xcodeproj
git commit -m "chore: rename project.yml targets amux → amux"
```

---

### Task 4: Update source code references

**Files:**
- Modify: `Sources/Core/Config.swift` (config path)
- Modify: `Sources/Core/SessionManager.swift` (tmux prefix)
- Modify: `Sources/App/MainWindowController.swift` (window title, class name, toolbar ID, GitHub URL, UI testing flag)
- Modify: `Sources/App/MenuBuilder.swift` (menu titles)
- Modify: `Sources/App/TabCoordinator.swift` (log message)
- Modify: `Sources/Core/ClaudeHooksSetup.swift` (comments)
- Modify: `Sources/Status/StatusPublisher.swift` (queue label)
- Modify: `Sources/Status/WebhookServer.swift` (queue label)
- Modify: `Sources/Status/WebhookStatusProvider.swift` (queue label)
- Modify: `Sources/Status/NotificationManager.swift` (notification names, identifiers)
- Modify: `Sources/Git/WorktreeDiscovery.swift` (queue label)
- Modify: `Sources/UI/Dashboard/DashboardViewController.swift` (pasteboard type)
- Modify: `Sources/UI/Panel/AIPanelView.swift` (source string)
- Modify: `Sources/Update/UpdateChecker.swift` (repo name, comment)
- Modify: `Sources/Update/UpdateManager.swift` (temp dir names)

- [ ] **Step 1: Update Config.swift — config path**

In `Sources/Core/Config.swift`, replace:
```swift
.appendingPathComponent(".config/amux")
```
with:
```swift
.appendingPathComponent(".config/amux")
```

And replace:
```swift
DispatchQueue(label: "com.amux.config-save", qos: .utility)
```
with:
```swift
DispatchQueue(label: "com.amux.config-save", qos: .utility)
```

- [ ] **Step 2: Update SessionManager.swift — tmux session prefix**

In `Sources/Core/SessionManager.swift`, replace:
```swift
/// Format: amux-<parent>-<name>, with dots and colons replaced by underscores.
```
with:
```swift
/// Format: amux-<parent>-<name>, with dots and colons replaced by underscores.
```

And replace:
```swift
return "amux-\(parent)-\(name)"
```
with:
```swift
return "amux-\(parent)-\(name)"
```

- [ ] **Step 3: Update MainWindowController.swift**

Replace `AmuxWindow` class name → `AmuxWindow` (both the class definition and instantiation):
```swift
// Line ~507
class AmuxWindow: NSWindow {
```
```swift
// Line ~110
let window = AmuxWindow(
```

Replace window title:
```swift
window.title = "amux"
```
with:
```swift
window.title = "amux"
```

Replace frame autosave name:
```swift
window.setFrameAutosaveName("PmuxMainWindow")
```
with:
```swift
window.setFrameAutosaveName("AmuxMainWindow")
```

Replace UI testing flag:
```swift
if arguments.contains("-PmuxUITesting") {
```
with:
```swift
if arguments.contains("-AmuxUITesting") {
```

Replace GitHub URL:
```swift
URL(string: "https://github.com/nicematt/amux")
```
with:
```swift
URL(string: "https://github.com/nicematt/amux")
```

Replace toolbar identifier:
```swift
NSToolbar(identifier: "amux.mainToolbar")
```
with:
```swift
NSToolbar(identifier: "amux.mainToolbar")
```

- [ ] **Step 4: Update MenuBuilder.swift**

Replace:
```swift
"Quit amux"
```
with:
```swift
"Quit amux"
```

Replace:
```swift
"amux Documentation"
```
with:
```swift
"amux Documentation"
```

- [ ] **Step 5: Update TabCoordinator.swift**

Replace:
```swift
NSLog("No workspaces configured. Add paths to ~/.config/amux/config.json")
```
with:
```swift
NSLog("No workspaces configured. Add paths to ~/.config/amux/config.json")
```

- [ ] **Step 6: Update ClaudeHooksSetup.swift**

Replace all 3 occurrences of `amux` in comments with `amux`.

- [ ] **Step 7: Update Status module queue labels**

In `Sources/Status/StatusPublisher.swift`:
```swift
DispatchQueue(label: "com.amux.status-poll", qos: .utility)
```

In `Sources/Status/WebhookServer.swift`:
```swift
DispatchQueue(label: "amux.webhook-server")
```

In `Sources/Status/WebhookStatusProvider.swift`:
```swift
DispatchQueue(label: "amux.webhook-status")
```

In `Sources/Status/NotificationManager.swift`:
```swift
Notification.Name("amux.navigateToWorktree")
```
```swift
categoryIdentifier = "amux.agentStatus"
```
```swift
identifier: "amux-\(worktreePath.hashValue)-\(paneIndex)"
```
```swift
identifier: "amux-\(worktreePath.hashValue)"
```

- [ ] **Step 8: Update remaining source files**

In `Sources/Git/WorktreeDiscovery.swift`:
```swift
DispatchQueue(label: "com.amux.git-discovery", qos: .userInitiated, attributes: .concurrent)
```

In `Sources/UI/Dashboard/DashboardViewController.swift`:
```swift
NSPasteboard.PasteboardType("com.amux.terminalCard")
```

In `Sources/UI/Panel/AIPanelView.swift`, replace both `"amux"` source strings with `"amux"`.

In `Sources/Update/UpdateChecker.swift`:
```swift
/// Checks GitHub Releases API for new versions of amux.
```
```swift
static let repoName = "amux"
```

In `Sources/Update/UpdateManager.swift`:
```swift
.appendingPathComponent("amux-update-\(UUID().uuidString)")
```
```swift
.appendingPathComponent("amux-updater-\(UUID().uuidString).sh")
```

- [ ] **Step 9: Verify build compiles**

```bash
xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 10: Commit**

```bash
git add Sources/ amux.xcodeproj
git commit -m "chore: rename all source code references amux → amux"
```

---

### Task 5: Update tests

**Files:**
- Modify: All files in `tests/` — replace `@testable import amux` with `@testable import amux`
- Modify: `Benchmarks/PerformanceTests.swift`
- Modify: `IntegrationTests/TerminalFullscreenVisualTest.swift`
- Modify: `UITests/Pages/AppPage.swift`

- [ ] **Step 1: Bulk replace @testable import in all test files**

Use sed to replace across all test files:
```bash
find tests/ Benchmarks/ IntegrationTests/ UITests/ -name "*.swift" -exec sed -i '' 's/@testable import amux/@testable import amux/g' {} +
```

- [ ] **Step 2: Update UITests/Pages/AppPage.swift**

Check for any `amux` references (e.g., `-PmuxUITesting` launch argument) and replace with `amux` equivalents.

- [ ] **Step 3: Update IntegrationTests/TerminalFullscreenVisualTest.swift**

Replace any `amux` references with `amux`.

- [ ] **Step 4: Verify tests compile**

```bash
xcodegen generate && xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug build 2>&1 | tail -5
```

- [ ] **Step 5: Commit**

```bash
git add tests/ Benchmarks/ IntegrationTests/ UITests/ amux.xcodeproj
git commit -m "chore: rename test imports amux → amux"
```

---

### Task 6: Update scripts

**Files:**
- Modify: `run.sh`
- Modify: `run_ui_tests.sh`
- Modify: `run_visual_test.sh`
- Modify: `scripts/setup.sh`
- Modify: `scripts/generate_icon.py`

- [ ] **Step 1: Update run.sh**

Replace all `amux` → `amux`:
- `"Building amux"` → `"Building amux"`
- `amux.xcodeproj` → `amux.xcodeproj`
- `-scheme amux` → `-scheme amux`
- `Debug/amux.app` → `Debug/amux.app`
- `"Killing existing amux"` → `"Killing existing amux"`
- `killall amux` → `killall amux`
- `"Launching amux"` → `"Launching amux"`

- [ ] **Step 2: Update run_ui_tests.sh**

Replace all `amux` → `amux`:
- `SCHEME="amux"` → `SCHEME="amux"`
- `amux.xcodeproj` → `amux.xcodeproj`
- `-only-testing:amuxUITests` → `-only-testing:amuxUITests`

- [ ] **Step 3: Update run_visual_test.sh**

Replace all `amux` → `amux`:
- `Debug/amux.app` → `Debug/amux.app`
- `"Building amux"` → `"Building amux"`
- `amux.xcodeproj` → `amux.xcodeproj`
- `-scheme amux` → `-scheme amux`
- `"Launching amux"` → `"Launching amux"`
- `pkill -f "amux.app"` → `pkill -f "amux.app"`
- `Contents/MacOS/amux` → `Contents/MacOS/amux`
- `process "amux"` → `process "amux"`
- `grep 'amux-'` → `grep 'amux-'`
- `"No amux tmux sessions"` → `"No amux tmux sessions"`
- `/tmp/amux-visual-test.png` → `/tmp/amux-visual-test.png`

- [ ] **Step 4: Update scripts/setup.sh**

Replace:
```bash
CACHE_DIR="$HOME/.cache/amux/ghosttykit"
```
with:
```bash
CACHE_DIR="$HOME/.cache/amux/ghosttykit"
```

Replace:
```bash
echo "    Open amux.xcodeproj in Xcode and build."
```
with:
```bash
echo "    Open amux.xcodeproj in Xcode and build."
```

- [ ] **Step 5: Update scripts/generate_icon.py docstring**

Replace:
```python
"""Generate amux app icon: 2x2 terminal grid, cyan + deep blue."""
```
with:
```python
"""Generate amux app icon: stacked terminal cards, cyan + deep blue."""
```

- [ ] **Step 6: Commit**

```bash
git add run.sh run_ui_tests.sh run_visual_test.sh scripts/
git commit -m "chore: rename scripts amux → amux"
```

---

### Task 7: Update CLAUDE.md and docs

**Files:**
- Modify: `CLAUDE.md`
- Modify: All `docs/**/*.md` files (bulk replace `amux` → `amux`, `AmuxWindow` → `AmuxWindow`, `AmuxUITestCase` → `AmuxUITestCase`)

- [ ] **Step 1: Update CLAUDE.md**

Replace all occurrences of `amux` with `amux` and `AmuxWindow` with `AmuxWindow`. Key replacements:
- Project description: "amux is a native macOS terminal multiplexer" → "amux (AMUX — Agent Multiplexer) is a native macOS terminal multiplexer"
- All `amux.xcodeproj` → `amux.xcodeproj`
- All `-scheme amux` → `-scheme amux`
- All `amuxTests` → `amuxTests`
- All `amuxUITests` → `amuxUITests`
- `~/.config/amux/config.json` → `~/.config/amux/config.json`
- `amux-<parent>-<name>` → `amux-<parent>-<name>`
- `AmuxWindow` → `AmuxWindow`
- `@testable import amux` → `@testable import amux`

- [ ] **Step 2: Bulk replace in docs/**

```bash
find docs/ -name "*.md" -exec sed -i '' 's/amux/amux/g; s/AmuxWindow/AmuxWindow/g; s/AmuxUITestCase/AmuxUITestCase/g' {} +
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md docs/
git commit -m "chore: rename docs amux → amux"
```

---

### Task 8: Generate icon assets from SVG

**Files:**
- Source: `Assets.xcassets/icon-draft-amux.svg`
- Output: `Assets.xcassets/AppIcon.appiconset/icon_*.png` (all sizes)

- [ ] **Step 1: Generate 1024px PNG from SVG**

```bash
qlmanage -t -s 1024 -o /tmp/ Assets.xcassets/icon-draft-amux.svg
cp /tmp/icon-draft-amux.svg.png Assets.xcassets/AppIcon.appiconset/icon_1024x1024.png
```

- [ ] **Step 2: Generate all required sizes using sips**

```bash
for size in 512 256 128 64 32 16; do
  sips -z $size $size Assets.xcassets/AppIcon.appiconset/icon_1024x1024.png \
    --out Assets.xcassets/AppIcon.appiconset/icon_${size}x${size}.png
done
```

- [ ] **Step 3: Clean up SVG draft**

```bash
rm Assets.xcassets/icon-draft-amux.svg
```

- [ ] **Step 4: Verify icon renders correctly**

```bash
open Assets.xcassets/AppIcon.appiconset/icon_1024x1024.png
```

- [ ] **Step 5: Commit**

```bash
git add Assets.xcassets/
git commit -m "feat: new AMUX icon — stacked terminal cards with Claude Code prompt"
```

---

### Task 9: Delete old xcodeproj and regenerate

**Files:**
- Delete: `amux.xcodeproj/`
- Create: `amux.xcodeproj/` (via xcodegen)

- [ ] **Step 1: Remove old Xcode project**

```bash
rm -rf amux.xcodeproj
```

- [ ] **Step 2: Regenerate**

```bash
xcodegen generate
```

- [ ] **Step 3: Verify full build**

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build 2>&1 | tail -5
```

Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 4: Run tests**

```bash
xcodebuild -project amux.xcodeproj -scheme amuxTests -configuration Debug test 2>&1 | tail -10
```

Expected: `** TEST SUCCEEDED **`

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: regenerate Xcode project as amux"
```
