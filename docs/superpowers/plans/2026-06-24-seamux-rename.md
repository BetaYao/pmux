# Seamux 改名 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把项目从 `amux` 改名为 `Seamux`(sea + multiplexer),覆盖构建身份、配置目录、测试与文档,且不破坏现有运行中的后端 session。

**Architecture:** 机械改名为主,唯一有逻辑的是配置目录迁移(`~/.config/amux` → `~/.config/seamux`,首启从旧路径复制)。Xcode 工程由 XcodeGen 从 `project.yml` 生成,改 `project.yml` 后 regenerate。

**Tech Stack:** Swift 5.10, AppKit, XcodeGen, xcodebuild, XCTest。

## Global Constraints

- macOS 14.0+,Swift 5.10,AppKit(非 SwiftUI)。
- 构建命令必须带 `-skipPackagePluginValidation -skipMacroValidation`(CodeEditSourceEditor 的 SwiftLint 插件需信任校验)。
- **内部后端 session 前缀 `amux-<parent>-<name>` 不改**(改了会丢失运行中 session 的恢复)。本计划只改用户可见身份。
- 新名:产品名 / scheme / target = `seamux`;bundle id 前缀 = `com.seamux`;显示名 = `Seamux`。
- 配置目录迁移:新路径不存在且旧路径存在时,复制旧目录内容到新路径(不删旧的,保证可回滚)。

---

### Task 1: project.yml 改名 + 重命名 bridging header,regenerate 并构建通过

**Files:**
- Modify: `project.yml`(name / bundleIdPrefix / targets 名 / PRODUCT_NAME / PRODUCT_BUNDLE_IDENTIFIER / SWIFT_OBJC_BRIDGING_HEADER / TEST_TARGET_NAME)
- Rename: `amux-Bridging-Header.h` → `seamux-Bridging-Header.h`

**Interfaces:**
- Produces: Swift 模块名变为 `seamux`(后续 Task 3 的 `@testable import seamux` 依赖此);scheme 名 `seamux`、测试 scheme `seamuxTests`/`seamuxUITests`(后续任务构建命令依赖)。

- [ ] **Step 1: 重命名 bridging header 文件**

```bash
git mv amux-Bridging-Header.h seamux-Bridging-Header.h
```

- [ ] **Step 2: 改 project.yml**

把以下值逐一替换(其余内容不动):

```yaml
name: seamux
# settings:
  bundleIdPrefix: com.seamux
# targets:
  seamux:                         # was: amux
      # bridging header path:
      - path: seamux-Bridging-Header.h
      PRODUCT_BUNDLE_IDENTIFIER: com.seamux.app
      PRODUCT_NAME: seamux
      SWIFT_OBJC_BRIDGING_HEADER: "$(PROJECT_DIR)/seamux-Bridging-Header.h"
  seamuxTests:                    # was: amuxTests
      # dependencies target:
      - target: seamux
      PRODUCT_BUNDLE_IDENTIFIER: com.seamux.tests
  seamuxUITests:                  # was: amuxUITests
      - target: seamux
      PRODUCT_BUNDLE_IDENTIFIER: com.seamux.uitests
      TEST_TARGET_NAME: seamux
```

- [ ] **Step 3: regenerate Xcode 工程**

```bash
xcodegen generate
```
Expected: 生成 `seamux.xcodeproj`(旧 `amux.xcodeproj` 可能仍在,见 Step 4)。

- [ ] **Step 4: 删除旧的 .xcodeproj 并确认新工程名**

```bash
rm -rf amux.xcodeproj
ls -d *.xcodeproj
```
Expected: 仅 `seamux.xcodeproj`。

- [ ] **Step 5: 构建(此时测试文件仍是旧 import,主 target 应能构建)**

```bash
xcodebuild -project seamux.xcodeproj -scheme seamux -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation build
```
Expected: BUILD SUCCEEDED(主 app target 不引用测试)。

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "build: rename target amux -> seamux in project.yml + bridging header"
```

---

### Task 2: 配置目录迁移(TDD)

**Files:**
- Modify: `Sources/Core/Config.swift`(`configDir` 常量 + 新增迁移函数,在 `load()` 开头调用)
- Test: `Tests/ConfigTests.swift`(新增迁移测试)

**Interfaces:**
- Consumes: 现有 `Config.configDir` / `Config.configPath` / `Config.load()`。
- Produces: `static func migrateLegacyConfigDirIfNeeded(home:fileManager:)`(可注入 home 目录与 FileManager 以便测试),`load()` 在读取前调用它。`configDir` 改指向 `~/.config/seamux`。

- [ ] **Step 1: 写失败测试**

在 `Tests/ConfigTests.swift` 增加:

```swift
func testMigratesLegacyConfigDirWhenNewMissing() throws {
    let fm = FileManager.default
    let tmp = fm.temporaryDirectory.appendingPathComponent("seamux-migrate-\(UUID().uuidString)")
    let legacy = tmp.appendingPathComponent(".config/amux")
    try fm.createDirectory(at: legacy, withIntermediateDirectories: true)
    try "{\"foo\":1}".write(to: legacy.appendingPathComponent("config.json"), atomically: true, encoding: .utf8)

    Config.migrateLegacyConfigDirIfNeeded(home: tmp, fileManager: fm)

    let migrated = tmp.appendingPathComponent(".config/seamux/config.json")
    XCTAssertTrue(fm.fileExists(atPath: migrated.path), "new config.json should exist after migration")
    XCTAssertTrue(fm.fileExists(atPath: legacy.appendingPathComponent("config.json").path), "legacy must be kept for rollback")
}

func testMigrationNoopWhenNewExists() throws {
    let fm = FileManager.default
    let tmp = fm.temporaryDirectory.appendingPathComponent("seamux-migrate-\(UUID().uuidString)")
    let legacy = tmp.appendingPathComponent(".config/amux")
    let new = tmp.appendingPathComponent(".config/seamux")
    try fm.createDirectory(at: legacy, withIntermediateDirectories: true)
    try fm.createDirectory(at: new, withIntermediateDirectories: true)
    try "OLD".write(to: legacy.appendingPathComponent("config.json"), atomically: true, encoding: .utf8)
    try "NEW".write(to: new.appendingPathComponent("config.json"), atomically: true, encoding: .utf8)

    Config.migrateLegacyConfigDirIfNeeded(home: tmp, fileManager: fm)

    let content = try String(contentsOf: new.appendingPathComponent("config.json"), encoding: .utf8)
    XCTAssertEqual(content, "NEW", "existing new config must not be overwritten")
}
```

- [ ] **Step 2: 运行测试确认失败**

```bash
xcodebuild -project seamux.xcodeproj -scheme seamuxTests -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation \
  test -only-testing:seamuxTests/ConfigTests/testMigratesLegacyConfigDirWhenNewMissing
```
Expected: 编译失败 "type 'Config' has no member 'migrateLegacyConfigDirIfNeeded'"。

- [ ] **Step 3: 实现**

在 `Sources/Core/Config.swift` 把 `configDir` 改为指向 `seamux`,并加迁移函数:

```swift
static let configDir = FileManager.default.homeDirectoryForCurrentUser
    .appendingPathComponent(".config/seamux")
static let configPath = configDir.appendingPathComponent("config.json")

/// 首启从旧 ~/.config/amux 复制到 ~/.config/seamux(保留旧目录以便回滚)。
static func migrateLegacyConfigDirIfNeeded(
    home: URL = FileManager.default.homeDirectoryForCurrentUser,
    fileManager fm: FileManager = .default
) {
    let new = home.appendingPathComponent(".config/seamux")
    let legacy = home.appendingPathComponent(".config/amux")
    guard !fm.fileExists(atPath: new.path),
          fm.fileExists(atPath: legacy.path) else { return }
    try? fm.copyItem(at: legacy, to: new)
}
```

在 `load()` 第一行(任何文件读取之前)调用:

```swift
static func load() -> Config {
    migrateLegacyConfigDirIfNeeded()
    // ...existing body...
```

- [ ] **Step 4: 运行测试确认通过**

```bash
xcodebuild -project seamux.xcodeproj -scheme seamuxTests -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation \
  test -only-testing:seamuxTests/ConfigTests/testMigratesLegacyConfigDirWhenNewMissing \
  -only-testing:seamuxTests/ConfigTests/testMigrationNoopWhenNewExists
```
Expected: 两个测试 PASS。

- [ ] **Step 5: 更新注释里的旧路径**

把这几处文档注释 `~/.config/amux/...` 改为 `~/.config/seamux/...`:
- `Sources/Core/WorktreeTaskStore.swift:6`
- `Sources/Core/WorktreeAgentTypeStore.swift:6`
- `Sources/App/TabCoordinator.swift:346`(用户可见日志字符串)
- `Sources/Terminal/GhosttyBridge.swift:34`(`ghostty.conf` 路径 —— 见注意)

> 注意 `GhosttyBridge.swift:34` 是真实读取路径,不是注释。改成 `.config/seamux/ghostty.conf`。迁移函数已整目录复制,故旧 `ghostty.conf` 会被带到新目录。

- [ ] **Step 6: Commit**

```bash
git add Sources/Core/Config.swift Tests/ConfigTests.swift \
  Sources/Core/WorktreeTaskStore.swift Sources/Core/WorktreeAgentTypeStore.swift \
  Sources/App/TabCoordinator.swift Sources/Terminal/GhosttyBridge.swift
git commit -m "feat: migrate config dir ~/.config/amux -> ~/.config/seamux with backward-compat"
```

---

### Task 3: 更新测试 target 的模块 import 并跑通全部测试

**Files:**
- Modify: `Tests/*.swift`(76 处 `@testable import amux` → `@testable import seamux`)
- Modify: `UITests/*.swift`(如有 `import amux` / `@testable import amux`)

**Interfaces:**
- Consumes: Task 1 产出的模块名 `seamux`。

- [ ] **Step 1: 批量替换 import**

```bash
grep -rl "@testable import amux" Tests UITests 2>/dev/null \
  | xargs sed -i '' 's/@testable import amux/@testable import seamux/g'
grep -rl "^import amux$" Tests UITests 2>/dev/null \
  | xargs sed -i '' 's/^import amux$/import seamux/g'
```

- [ ] **Step 2: 确认无残留旧 import**

```bash
grep -rn "import amux" Tests UITests 2>/dev/null || echo "clean"
```
Expected: `clean`。

- [ ] **Step 3: 跑全部单元测试**

```bash
xcodebuild -project seamux.xcodeproj -scheme seamuxTests -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation test
```
Expected: TEST SUCCEEDED,所有测试通过。

- [ ] **Step 4: Commit**

```bash
git add Tests UITests
git commit -m "test: update module import amux -> seamux"
```

---

### Task 4: run.sh + 文档改名

**Files:**
- Modify: `run.sh`(7 处 `amux`)
- Modify: `CLAUDE.md`(项目名、构建命令里的 project/scheme)
- Modify: `README*`(如存在)

**Interfaces:** 无代码接口(脚本与文档)。

- [ ] **Step 1: 改 run.sh**

把 `run.sh` 中:
- `xcodebuild -project amux.xcodeproj -scheme amux` → `-project seamux.xcodeproj -scheme seamux`
- `$BUILD_DIR/Build/Products/Debug/amux.app` → `seamux.app`
- `killall amux` → `killall seamux`
- `"$APP/Contents/MacOS/amux"` → `seamux`
- echo 文案 `amux` → `Seamux`

- [ ] **Step 2: 跑 run.sh 验证构建+启动**

```bash
./run.sh
```
Expected: 构建成功,Seamux 应用启动(Ctrl+C 退出)。

- [ ] **Step 3: 改 CLAUDE.md**

把构建命令段所有 `amux.xcodeproj` → `seamux.xcodeproj`、`-scheme amux`/`amuxTests`/`amuxUITests` → `seamux`/`seamuxTests`/`seamuxUITests`;首段项目名 `amux (AMUX — Agent Multiplexer)` → `Seamux (sea + multiplexer)`,保留 Agent Multiplexer 说明。

- [ ] **Step 4: Commit**

```bash
git add run.sh CLAUDE.md README* 2>/dev/null
git commit -m "docs: rename amux -> seamux in run.sh and docs"
```

---

## Self-Review

**Spec coverage:** 改名 spec 节列的范围逐条对照 —— project.yml/scheme/product/bundle(Task 1)、bridging header(Task 1)、测试 target + `@testable import`(Task 3)、`run.sh`/`CLAUDE.md`(Task 4)、配置目录迁移含数据风险点(Task 2)。全部覆盖。内部 session 前缀按 Global Constraints 明确不改。

**Placeholder scan:** 无 TBD/TODO;每个改值都给了具体前后值或命令。

**Type consistency:** 模块名 `seamux` 在 Task 1 定义、Task 3 消费,一致;迁移函数签名 `migrateLegacyConfigDirIfNeeded(home:fileManager:)` 在 Task 2 测试与实现中一致。
