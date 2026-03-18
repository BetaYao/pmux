# E2E Testing for Scheduled Tasks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add keyboard shortcuts for scheduled task operations and build an E2E test framework using AppleScript keystrokes + screencapture + macOS Vision OCR.

**Architecture:** Extend the existing `ShortcutAction` enum and `handle_shortcut()` dispatch with 3 new task shortcuts. Task selection/focus state lives on `AppRoot` (since `Sidebar` is `RenderOnce`) and is passed to `Sidebar` via builder methods. E2E tests are shell scripts that launch pmux, send keystrokes via `osascript`, take screenshots via `screencapture`, and verify text via a Swift Vision OCR CLI tool.

**Tech Stack:** Rust/GPUI (shortcuts + UI), Swift (OCR tool), Bash (test harness), AppleScript (keystroke sending), macOS Vision framework (OCR)

---

### Task 1: Add New ShortcutAction Variants and Bindings

**Files:**
- Modify: `src/keyboard_shortcuts.rs:6-12` (ShortcutCategory enum)
- Modify: `src/keyboard_shortcuts.rs:15-51` (ShortcutAction enum)
- Modify: `src/keyboard_shortcuts.rs:82-278` (KeyBinding::all_defaults)

- [ ] **Step 1: Add `Tasks` to `ShortcutCategory` enum**

In `src/keyboard_shortcuts.rs`, add `Tasks` variant:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShortcutCategory {
    General,
    Navigation,
    Workspace,
    View,
    Tasks,
}
```

- [ ] **Step 2: Add task actions to `ShortcutAction` enum**

Add after the `// View` section:

```rust
    // Tasks
    ToggleTaskList,
    NewTask,
    DeleteTask,
```

- [ ] **Step 3: Add default bindings**

Add to `KeyBinding::all_defaults()` vec, after the ViewDiff entry:

```rust
            // Tasks
            Self::new(
                ShortcutAction::ToggleTaskList,
                "Toggle Task List",
                "⌘⇧L",
                "Show/hide and focus the scheduled tasks list",
                ShortcutCategory::Tasks,
            ),
            Self::new(
                ShortcutAction::NewTask,
                "New Task",
                "⌘⇧T",
                "Create a new scheduled task",
                ShortcutCategory::Tasks,
            ),
            Self::new(
                ShortcutAction::DeleteTask,
                "Delete Task",
                "⌘⇧⌫",
                "Delete the selected scheduled task",
                ShortcutCategory::Tasks,
            ),
```

- [ ] **Step 4: Update default bindings count test**

In `src/keyboard_shortcuts.rs` test `test_default_bindings_count`, update expected count from 27 to 30:

```rust
    fn test_default_bindings_count() {
        let bindings = KeyBinding::all_defaults();
        // General(6) + Navigation(9) + Workspace(7) + View(5) + Tasks(3) = 30
        assert_eq!(bindings.len(), 30, "VerticalSplit/HorizontalSplit and other defaults");
    }
```

- [ ] **Step 5: Update test_category_variants test**

In `src/keyboard_shortcuts.rs` test `test_category_variants`, add `Tasks` and update count from 4 to 5:

```rust
    fn test_category_variants() {
        let cats = vec![
            ShortcutCategory::General,
            ShortcutCategory::Navigation,
            ShortcutCategory::Workspace,
            ShortcutCategory::View,
            ShortcutCategory::Tasks,
        ];
        assert_eq!(cats.len(), 5);
    }
```

- [ ] **Step 6: Run tests and verify**

Run: `RUSTUP_TOOLCHAIN=stable cargo test keyboard_shortcuts`
Expected: All tests pass

- [ ] **Step 7: Commit**

```bash
git add src/keyboard_shortcuts.rs
git commit -m "feat: add ToggleTaskList, NewTask, DeleteTask shortcut actions"
```

---

### Task 2: Add Task Selection and Focus State to AppRoot

**Files:**
- Modify: `src/ui/app_root.rs:170-200` (struct fields)
- Modify: `src/ui/app_root.rs:440-530` (constructor defaults)

- [ ] **Step 1: Add state fields to AppRoot struct**

In `src/ui/app_root.rs`, after `pub(crate) tasks_expanded: bool,` (line 177), add:

```rust
    /// Index of currently selected task in the task list (for keyboard navigation)
    pub(crate) selected_task_index: Option<usize>,
    /// Whether the task list area has keyboard focus (arrow keys navigate tasks)
    pub(crate) task_list_focused: bool,
    /// Task ID pending deletion (waiting for Enter/Escape confirmation)
    pub(crate) task_pending_delete: Option<uuid::Uuid>,
```

- [ ] **Step 2: Initialize new fields in constructor**

In the `AppRoot::new()` or equivalent constructor, after `tasks_expanded: true,` add:

```rust
            selected_task_index: None,
            task_list_focused: false,
            task_pending_delete: None,
```

- [ ] **Step 3: Verify it compiles**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: add task selection/focus/pending-delete state to AppRoot"
```

---

### Task 3: Add Task Dialog Modal Check to handle_key_down

**Files:**
- Modify: `src/ui/app_root.rs:2332-2363` (handle_key_down modal checks)

- [ ] **Step 1: Block keys when TaskDialog is open**

In `handle_key_down()`, after the new branch dialog check (line ~2362) and before the Alt+Cmd+arrows check, add:

```rust
        // Modal: when task dialog is open, block all keys (TaskDialog handles its own keys)
        if self.task_dialog.is_some() {
            return;
        }
```

- [ ] **Step 2: Verify it compiles**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "fix: block key forwarding when TaskDialog modal is open"
```

---

### Task 4: Wire Shortcuts in handle_shortcut()

**Files:**
- Modify: `src/ui/app_root_render.rs:89-161` (handle_shortcut method)

- [ ] **Step 1: Handle `Cmd+Shift+L` — toggle task list**

In `handle_shortcut()`, add a new match arm. Since `handle_shortcut` is called when `event.keystroke.modifiers.platform` is true, we check `shift` inside. Add before the `_ => {}` catch-all:

```rust
            "l" => {
                if event.keystroke.modifiers.shift {
                    self.tasks_expanded = !self.tasks_expanded;
                    if self.tasks_expanded {
                        self.task_list_focused = true;
                        // Select first task if none selected
                        if self.selected_task_index.is_none() {
                            let task_count = self.scheduler_manager.as_ref()
                                .map(|m| m.read(cx).tasks().len())
                                .unwrap_or(0);
                            if task_count > 0 {
                                self.selected_task_index = Some(0);
                            }
                        }
                    } else {
                        self.task_list_focused = false;
                    }
                    cx.notify();
                }
            }
```

- [ ] **Step 2: Handle `Cmd+Shift+T` — new task**

Add match arm for `"t"`:

```rust
            "t" => {
                if event.keystroke.modifiers.shift {
                    self.open_task_dialog(cx);
                }
            }
```

- [ ] **Step 3: Handle `Cmd+Shift+Backspace` — delete task**

Add match arm for `"backspace"`:

```rust
            "backspace" => {
                if event.keystroke.modifiers.shift && self.task_list_focused {
                    if let Some(idx) = self.selected_task_index {
                        let task_id = self.scheduler_manager.as_ref()
                            .and_then(|m| m.read(cx).tasks().get(idx).map(|t| t.id));
                        if let Some(id) = task_id {
                            self.task_pending_delete = Some(id);
                            cx.notify();
                        }
                    }
                }
            }
```

- [ ] **Step 4: Verify it compiles**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src/ui/app_root_render.rs
git commit -m "feat: wire Cmd+Shift+L/T/Backspace shortcuts for scheduled tasks"
```

---

### Task 5: Add Arrow Key Navigation in handle_key_down()

**Files:**
- Modify: `src/ui/app_root.rs:2412-2416` (handle_key_down, after search check, before Cmd+key shortcuts)

- [ ] **Step 1: Add task list navigation dispatch**

In `handle_key_down()`, insert **after** the search check block (line ~2410) and **before** the `if event.keystroke.modifiers.platform {` block (line ~2412):

```rust
        // Task list focused: arrow keys navigate tasks, Enter/Escape handle confirmation
        if self.task_list_focused {
            match event.keystroke.key.as_str() {
                "up" => {
                    if let Some(idx) = self.selected_task_index {
                        if idx > 0 {
                            self.selected_task_index = Some(idx - 1);
                            self.task_pending_delete = None; // cancel pending delete on nav
                            cx.notify();
                        }
                    }
                    return;
                }
                "down" => {
                    let task_count = self.scheduler_manager.as_ref()
                        .map(|m| m.read(cx).tasks().len())
                        .unwrap_or(0);
                    if let Some(idx) = self.selected_task_index {
                        if idx + 1 < task_count {
                            self.selected_task_index = Some(idx + 1);
                            self.task_pending_delete = None;
                            cx.notify();
                        }
                    }
                    return;
                }
                "enter" => {
                    // Confirm pending delete
                    if let Some(id) = self.task_pending_delete.take() {
                        if let Some(ref manager) = self.scheduler_manager {
                            manager.update(cx, |m, cx| {
                                let _ = m.remove_task(id, cx);
                            });
                        }
                        // Adjust selected index
                        let task_count = self.scheduler_manager.as_ref()
                            .map(|m| m.read(cx).tasks().len())
                            .unwrap_or(0);
                        if task_count == 0 {
                            self.selected_task_index = None;
                        } else if let Some(idx) = self.selected_task_index {
                            if idx >= task_count {
                                self.selected_task_index = Some(task_count - 1);
                            }
                        }
                        cx.notify();
                    }
                    return;
                }
                "escape" => {
                    if self.task_pending_delete.is_some() {
                        self.task_pending_delete = None;
                    } else {
                        self.task_list_focused = false;
                        self.selected_task_index = None;
                    }
                    cx.notify();
                    return;
                }
                _ => {}
            }
        }
```

- [ ] **Step 2: Verify it compiles**

Run: `RUSTUP_TOOLCHAIN=stable cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/ui/app_root.rs
git commit -m "feat: arrow key navigation and delete confirmation in task list"
```

---

### Task 6: Pass Selection State to Sidebar and Render Highlights

**Files:**
- Modify: `src/ui/sidebar.rs:65-101` (Sidebar struct fields)
- Modify: `src/ui/sidebar.rs:104-136` (Sidebar::new constructor)
- Modify: `src/ui/sidebar.rs:270-315` (builder methods area)
- Modify: `src/ui/sidebar.rs:377-462` (render_task_item)
- Modify: `src/ui/app_root_render.rs:911-916` (sidebar builder chain)

- [ ] **Step 1: Add builder fields to Sidebar**

In `src/ui/sidebar.rs`, add to struct after `on_add_task` field (line ~101):

```rust
    on_delete_task: Option<Arc<dyn Fn(Uuid, &mut Window, &mut App) + Send + Sync>>,
    selected_task_index: Option<usize>,
    task_list_focused: bool,
    task_pending_delete: Option<Uuid>,
```

Initialize in `Sidebar::new()` after `on_add_task: None,`:

```rust
            on_delete_task: None,
            selected_task_index: None,
            task_list_focused: false,
            task_pending_delete: None,
```

- [ ] **Step 2: Add builder methods**

Add after existing builder methods (near the `on_add_task` builder):

```rust
    pub fn on_delete_task<F: Fn(Uuid, &mut Window, &mut App) + Send + Sync + 'static>(
        mut self,
        f: F,
    ) -> Self {
        self.on_delete_task = Some(Arc::new(f));
        self
    }

    pub fn with_selected_task_index(mut self, index: Option<usize>) -> Self {
        self.selected_task_index = index;
        self
    }

    pub fn with_task_list_focused(mut self, focused: bool) -> Self {
        self.task_list_focused = focused;
        self
    }

    pub fn with_task_pending_delete(mut self, id: Option<Uuid>) -> Self {
        self.task_pending_delete = id;
        self
    }
```

- [ ] **Step 3: Update render_task_item to show selection + pending delete**

Replace the `render_task_item` method signature to accept index:

```rust
    fn render_task_item(&self, task: &ScheduledTask, index: usize) -> impl IntoElement {
```

Add selection/pending-delete background at the start of the item div (replace the existing `.hover(|style| style.bg(rgb(0x2a2a2a)))` line):

```rust
        let is_selected = self.task_list_focused && self.selected_task_index == Some(index);
        let is_pending_delete = self.task_pending_delete == Some(task.id);

        let bg = if is_pending_delete {
            rgb(0x4a1c1c) // red tint for pending delete
        } else if is_selected {
            rgb(0x3a3a3a) // highlight for selected
        } else {
            rgb(0x00000000) // transparent
        };
```

Apply to the item div — replace `.hover(|style| style.bg(rgb(0x2a2a2a)))` with:

```rust
            .bg(bg)
            .when(!is_pending_delete, |el| el.hover(|style| style.bg(rgb(0x2a2a2a))))
```

After the existing item content, add a "Delete? Enter/Esc" label when pending:

```rust
        if is_pending_delete {
            item = item.child(
                div()
                    .text_color(rgb(0xf87171))
                    .text_xs()
                    .child("Delete? Enter/Esc"),
            );
        }
```

- [ ] **Step 4: Update render_tasks_section to pass index**

In `render_tasks_section()`, update the `.map()` call (line ~517-518) to use `enumerate`:

```rust
                el.children(
                    self.scheduled_tasks
                        .iter()
                        .enumerate()
                        .map(|(i, task)| self.render_task_item(task, i)),
                )
```

- [ ] **Step 5: Pass state from AppRoot to Sidebar**

In `src/ui/app_root_render.rs`, after `.with_tasks_expanded(self.tasks_expanded)` (line ~916), add:

```rust
            .with_selected_task_index(self.selected_task_index)
            .with_task_list_focused(self.task_list_focused)
            .with_task_pending_delete(self.task_pending_delete)
```

- [ ] **Step 6: Verify it compiles and run tests**

Run: `RUSTUP_TOOLCHAIN=stable cargo check && RUSTUP_TOOLCHAIN=stable cargo test`
Expected: Compiles and all tests pass

- [ ] **Step 7: Commit**

```bash
git add src/ui/sidebar.rs src/ui/app_root_render.rs
git commit -m "feat: render task selection highlight and delete confirmation in sidebar"
```

---

### Task 7: Build OCR Tool (Swift Vision CLI)

**Files:**
- Create: `tests/e2e/ocr_tool.swift`

- [ ] **Step 1: Create tests/e2e directory**

```bash
mkdir -p tests/e2e/results
```

- [ ] **Step 2: Write ocr_tool.swift**

Create `tests/e2e/ocr_tool.swift`:

```swift
import Foundation
import Vision

guard CommandLine.arguments.count > 1 else {
    fputs("Usage: ocr_tool <image_path>\n", stderr)
    exit(1)
}

let path = CommandLine.arguments[1]
guard FileManager.default.fileExists(atPath: path) else {
    fputs("Error: file not found: \(path)\n", stderr)
    exit(1)
}

guard let image = CGImage.from(path: path) else {
    fputs("Error: could not load image: \(path)\n", stderr)
    exit(2)
}

let semaphore = DispatchSemaphore(value: 0)
var recognizedText: [String] = []
var ocrError: Error?

let request = VNRecognizeTextRequest { request, error in
    if let error = error {
        ocrError = error
    } else if let observations = request.results as? [VNRecognizedTextObservation] {
        for observation in observations {
            if let candidate = observation.topCandidates(1).first {
                recognizedText.append(candidate.string)
            }
        }
    }
    semaphore.signal()
}

request.recognitionLevel = .accurate
request.recognitionLanguages = ["en-US", "zh-Hans"]
request.usesLanguageCorrection = true

let handler = VNImageRequestHandler(cgImage: image, options: [:])
do {
    try handler.perform([request])
} catch {
    fputs("Error: OCR failed: \(error)\n", stderr)
    exit(2)
}

semaphore.wait()

if let error = ocrError {
    fputs("Error: OCR failed: \(error)\n", stderr)
    exit(2)
}

for line in recognizedText {
    print(line)
}

// Helper extension to load CGImage from file path
extension CGImage {
    static func from(path: String) -> CGImage? {
        let url = URL(fileURLWithPath: path)
        guard let dataProvider = CGDataProvider(url: url as CFURL) else { return nil }
        let lowercasePath = path.lowercased()
        if lowercasePath.hasSuffix(".png") {
            return CGImage(pngDataProviderSource: dataProvider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        } else if lowercasePath.hasSuffix(".jpg") || lowercasePath.hasSuffix(".jpeg") {
            return CGImage(jpegDataProviderSource: dataProvider, decode: nil, shouldInterpolate: true, intent: .defaultIntent)
        }
        // Fallback: try using ImageIO for other formats
        guard let source = CGImageSourceCreateWithDataProvider(dataProvider, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }
}
```

- [ ] **Step 3: Compile and verify**

Run: `swiftc -o tests/e2e/ocr_tool tests/e2e/ocr_tool.swift -framework Vision -framework CoreGraphics -framework ImageIO`
Expected: Binary created at `tests/e2e/ocr_tool`

- [ ] **Step 4: Quick smoke test with a screenshot**

```bash
screencapture -x /tmp/pmux_ocr_test.png && tests/e2e/ocr_tool /tmp/pmux_ocr_test.png | head -5
```
Expected: Some recognized text lines from the current screen

- [ ] **Step 5: Commit**

```bash
echo "tests/e2e/ocr_tool" >> .gitignore
echo "tests/e2e/results/" >> .gitignore
git add tests/e2e/ocr_tool.swift .gitignore
git commit -m "feat: add macOS Vision OCR CLI tool for E2E tests"
```

---

### Task 8: Build Test Helpers (helpers.sh)

**Files:**
- Create: `tests/e2e/helpers.sh`

- [ ] **Step 1: Write helpers.sh**

Create `tests/e2e/helpers.sh`:

```bash
#!/bin/bash
# E2E test helpers for pmux
# Requires: macOS, Accessibility permissions for terminal app

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_DIR="$SCRIPT_DIR/results"
OCR_TOOL="$SCRIPT_DIR/ocr_tool"
OCR_SOURCE="$SCRIPT_DIR/ocr_tool.swift"
PMUX_PID=""
WINDOW_ID=""
PASS_COUNT=0
FAIL_COUNT=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

ensure_ocr_tool() {
    if [ ! -f "$OCR_TOOL" ] || [ "$OCR_SOURCE" -nt "$OCR_TOOL" ]; then
        echo "Compiling ocr_tool..."
        swiftc -o "$OCR_TOOL" "$OCR_SOURCE" -framework Vision -framework CoreGraphics -framework ImageIO
    fi
}

start_pmux() {
    echo "Building and launching pmux..."
    mkdir -p "$RESULTS_DIR"
    cd "$SCRIPT_DIR/../.."
    RUSTUP_TOOLCHAIN=stable cargo run &
    PMUX_PID=$!
    cd "$SCRIPT_DIR"
    echo "pmux PID: $PMUX_PID"
}

wait_for_window() {
    local timeout=${1:-30}
    local elapsed=0
    echo "Waiting for pmux window (timeout: ${timeout}s)..."
    while [ $elapsed -lt $timeout ]; do
        WINDOW_ID=$(osascript -e '
            tell application "System Events"
                set pmuxProcs to (every process whose name is "pmux")
                if (count of pmuxProcs) > 0 then
                    tell (first process whose name is "pmux")
                        if (count of windows) > 0 then
                            return id of front window
                        end if
                    end tell
                end if
            end tell
            return ""
        ' 2>/dev/null || true)
        if [ -n "$WINDOW_ID" ] && [ "$WINDOW_ID" != "" ]; then
            echo "Found pmux window: $WINDOW_ID"
            sleep 1  # extra settle time
            return 0
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    echo "ERROR: pmux window not found after ${timeout}s"
    return 1
}

send_key() {
    local modifiers="$1"
    local key="$2"
    local using_clause=""

    for mod in $modifiers; do
        case "$mod" in
            cmd)   using_clause="${using_clause}command down, " ;;
            shift) using_clause="${using_clause}shift down, " ;;
            alt)   using_clause="${using_clause}option down, " ;;
            ctrl)  using_clause="${using_clause}control down, " ;;
        esac
    done

    # Remove trailing ", "
    using_clause="${using_clause%, }"

    osascript -e "
        tell application \"System Events\"
            tell (first process whose name is \"pmux\")
                set frontmost to true
            end tell
            delay 0.1
            keystroke \"$key\" using {$using_clause}
        end tell
    "
    sleep 0.5
}

send_special_key() {
    local key_code="$1"
    local modifiers="${2:-}"
    local using_clause=""

    for mod in $modifiers; do
        case "$mod" in
            cmd)   using_clause="${using_clause}command down, " ;;
            shift) using_clause="${using_clause}shift down, " ;;
            alt)   using_clause="${using_clause}option down, " ;;
            ctrl)  using_clause="${using_clause}control down, " ;;
        esac
    done
    using_clause="${using_clause%, }"

    if [ -n "$using_clause" ]; then
        osascript -e "
            tell application \"System Events\"
                key code $key_code using {$using_clause}
            end tell
        "
    else
        osascript -e "
            tell application \"System Events\"
                key code $key_code
            end tell
        "
    fi
    sleep 0.5
}

send_text() {
    local text="$1"
    osascript -e "
        tell application \"System Events\"
            tell (first process whose name is \"pmux\")
                set frontmost to true
            end tell
            delay 0.1
        end tell
    "
    for (( i=0; i<${#text}; i++ )); do
        local char="${text:$i:1}"
        osascript -e "
            tell application \"System Events\"
                keystroke \"$char\"
            end tell
        "
        sleep 0.05
    done
    sleep 0.3
}

take_screenshot() {
    local name="$1"
    local output="$RESULTS_DIR/${name}.png"
    # Use screencapture with window selection by process
    # -l requires CGWindowID; fall back to capturing the front window
    screencapture -o -x -l "$WINDOW_ID" "$output" 2>/dev/null || \
        screencapture -o -x "$output"
    echo "$output"
}

ocr() {
    local image="$1"
    "$OCR_TOOL" "$image" 2>/dev/null || true
}

assert_contains() {
    local step_name="$1"
    local expected="$2"
    local max_retries=3
    local retry=0

    while [ $retry -lt $max_retries ]; do
        local screenshot
        screenshot=$(take_screenshot "${step_name}_attempt${retry}")
        local text
        text=$(ocr "$screenshot")

        if echo "$text" | grep -qi "$expected"; then
            echo -e "${GREEN}[PASS]${NC} $step_name — found \"$expected\""
            PASS_COUNT=$((PASS_COUNT + 1))
            return 0
        fi

        retry=$((retry + 1))
        if [ $retry -lt $max_retries ]; then
            echo "  Retry $retry/$max_retries for \"$expected\"..."
            sleep 1
        fi
    done

    echo -e "${RED}[FAIL]${NC} $step_name — \"$expected\" not found"
    echo "  OCR output was:"
    echo "$text" | sed 's/^/    /'
    FAIL_COUNT=$((FAIL_COUNT + 1))
    return 1
}

assert_not_contains() {
    local step_name="$1"
    local unexpected="$2"
    local max_retries=3
    local retry=0

    while [ $retry -lt $max_retries ]; do
        local screenshot
        screenshot=$(take_screenshot "${step_name}_attempt${retry}")
        local text
        text=$(ocr "$screenshot")

        if ! echo "$text" | grep -qi "$unexpected"; then
            echo -e "${GREEN}[PASS]${NC} $step_name — \"$unexpected\" not found (as expected)"
            PASS_COUNT=$((PASS_COUNT + 1))
            return 0
        fi

        retry=$((retry + 1))
        if [ $retry -lt $max_retries ]; then
            echo "  Retry $retry/$max_retries waiting for \"$unexpected\" to disappear..."
            sleep 1
        fi
    done

    echo -e "${RED}[FAIL]${NC} $step_name — \"$unexpected\" still present"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    return 1
}

report() {
    local total=$((PASS_COUNT + FAIL_COUNT))
    echo ""
    echo "=== ${PASS_COUNT}/${total} tests passed ==="
    if [ $FAIL_COUNT -gt 0 ]; then
        echo "Screenshots saved to: $RESULTS_DIR"
        return 1
    fi
    return 0
}

cleanup() {
    if [ -n "$PMUX_PID" ] && kill -0 "$PMUX_PID" 2>/dev/null; then
        echo "Stopping pmux (PID: $PMUX_PID)..."
        kill "$PMUX_PID" 2>/dev/null || true
        wait "$PMUX_PID" 2>/dev/null || true
    fi
}
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x tests/e2e/helpers.sh
```

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/helpers.sh
git commit -m "feat: add E2E test helpers (send_key, screenshot, OCR, assertions)"
```

---

### Task 9: Write the Scheduled Tasks E2E Test

**Files:**
- Create: `tests/e2e/test_scheduled_tasks.sh`

- [ ] **Step 1: Write test_scheduled_tasks.sh**

Create `tests/e2e/test_scheduled_tasks.sh`:

```bash
#!/bin/bash
# E2E test: Create and delete a scheduled task via keyboard shortcuts
# Requires: macOS, Accessibility permissions, pmux buildable

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/helpers.sh"

# Cleanup on exit
trap cleanup EXIT

echo "=== E2E Test: Scheduled Tasks Create & Delete ==="
echo ""

# Setup
ensure_ocr_tool
start_pmux
wait_for_window 30

sleep 2  # let UI fully render

# Test 1: Toggle task list with Cmd+Shift+L
echo "--- Test 1: Toggle task list ---"
send_key "cmd shift" "l"
sleep 1
assert_contains "task_list_visible" "Scheduled Tasks" || true

# Test 2: Open TaskDialog with Cmd+Shift+T
echo "--- Test 2: Open TaskDialog ---"
send_key "cmd shift" "t"
sleep 1
assert_contains "task_dialog_open" "New Scheduled Task" || true

# Test 3: Create a task
echo "--- Test 3: Create task ---"
send_text "E2E_Test_Task"
# Tab to skip Cron field (keeps default "0 2 * * *")
send_special_key 48  # Tab key code
sleep 0.3
send_special_key 48  # Tab again to Command field
sleep 0.3
send_text "echo hello"
# Enter to save
send_special_key 36  # Return key code
sleep 1
assert_contains "task_created" "E2E_Test_Task" || true

# Test 4: Delete the task
echo "--- Test 4: Delete task ---"
# Focus task list
send_key "cmd shift" "l"
sleep 0.5
# Arrow down to select the task (it may be the first/only one)
send_special_key 125  # Down arrow key code
sleep 0.3
# Cmd+Shift+Backspace to initiate delete
send_special_key 51 "cmd shift"  # Backspace key code = 51
sleep 0.5
# Enter to confirm
send_special_key 36  # Return key code
sleep 1
assert_not_contains "task_deleted" "E2E_Test_Task" || true

# Report
echo ""
report
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x tests/e2e/test_scheduled_tasks.sh
```

- [ ] **Step 3: Commit**

```bash
git add tests/e2e/test_scheduled_tasks.sh
git commit -m "feat: add E2E test for scheduled task create and delete"
```

---

### Task 10: Integration Verification

- [ ] **Step 1: Run full test suite**

Run: `RUSTUP_TOOLCHAIN=stable cargo test`
Expected: All existing tests pass

- [ ] **Step 2: Manual smoke test**

Run: `RUSTUP_TOOLCHAIN=stable cargo run`

Verify manually:
1. `Cmd+Shift+L` expands/collapses task list in sidebar
2. `Cmd+Shift+T` opens the new task dialog
3. Create a task, see it in the sidebar
4. Arrow keys highlight tasks when task list is focused
5. `Cmd+Shift+Backspace` shows "Delete? Enter/Esc" on selected task
6. `Enter` confirms deletion, `Escape` cancels
7. `Escape` exits task list focus mode

- [ ] **Step 3: Run E2E test**

Run: `tests/e2e/test_scheduled_tasks.sh`
Expected: 4/4 tests pass

- [ ] **Step 4: Final commit**

```bash
git status
# Only commit if there are meaningful changes not already committed
# git add <specific files> && git commit -m "test: verify E2E scheduled tasks test passes"
```
