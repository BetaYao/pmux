# pmux - AI Agent 多分支开发工作台

pmux 是一个原生桌面应用，用于管理多个 AI Agent 并行工作（每个 agent 一个 git worktree），实时监控 agent 状态，主动通知，以及快速 Review Diff。

## 当前状态：规格 1 完成 ✅

规格 1 实现了默认启动页（选择工作区）功能：

- ✅ 首次启动时显示友好的启动页面
- ✅ 支持选择 Git 仓库作为工作区
- ✅ 自动验证所选目录是否为 Git 仓库
- ✅ 保存工作区配置，下次启动自动加载
- ✅ 完整的错误处理和用户提示

## 快速开始

### macOS 构建依赖

在 Apple Silicon (M1/M2/M3) 上首次构建需要：

1. **Xcode**：完整安装（非仅 Command Line Tools）
2. **Metal Toolchain**：若 `cargo run` 报错 `cannot execute tool 'metal'`，运行：
   ```bash
   xcodebuild -downloadComponent MetalToolchain
   ```
   或使用 `./scripts/run.sh` 自动检测并下载。

### 运行测试

```bash
cargo test
```

### 运行应用程序

```bash
cargo run
```

或使用脚本（会自动检查 Metal 依赖）：

```bash
./scripts/run.sh
```

这将启动 pmux GUI，您可以：
1. 选择工作区（打开文件选择器）
2. 查看当前工作区
3. 更换工作区
4. 退出应用

### 构建发布版本

```bash
cargo build --release
```

### 应用图标与打包

应用图标使用布丁 logo（`布丁logo_极简线条.png`）。打包前需先生成图标：

```bash
# 安装依赖（首次）
python3 -m venv .venv-icon && .venv-icon/bin/pip install -r resources/requirements.txt

# 生成标准图标
.venv-icon/bin/python resources/generate_icon.py

# 生成 dev 模式图标（带 DEV 角标）
.venv-icon/bin/python resources/generate_icon.py --dev
```

打包为 .app（macOS）：

```bash
# 标准版
./scripts/bundle.sh

# dev 版（图标带 DEV 角标）
./scripts/bundle.sh --dev
```

## 项目结构

```
src/
├── lib.rs           # 库入口
├── main.rs          # 应用程序入口（CLI 主循环）
├── app.rs           # 应用逻辑和状态管理
├── config.rs        # 配置管理（读写 ~/.config/pmux/config.json）
├── git_utils.rs     # Git 仓库验证
├── file_selector.rs # 跨平台文件选择器（使用 rfd）
└── ui/
    └── mod.rs       # UI 渲染（CLI 版本使用 ASCII 艺术）
```

## 技术栈

- **Rust** - 主要编程语言
- **serde** - JSON 序列化/反序列化
- **rfd** - 跨平台文件选择器
- **thiserror** - 错误处理
- **tempfile** - 测试用的临时文件（dev dependency）

## 开发方法：TDD

本项目严格遵循测试驱动开发（TDD）原则：

1. **RED**: 先写测试，确保测试失败
2. **GREEN**: 写最小代码使测试通过
3. **REFACTOR**: 重构代码，保持测试通过

### 测试统计

| 模块 | 测试数 | 状态 |
|------|--------|------|
| config | 6 | ✅ 通过 |
| git_utils | 8 | ✅ 通过 |
| file_selector | 1 | ✅ 通过 |
| app | 4 | ✅ 通过 |
| main | 1 | ✅ 通过 |
| **总计** | **20** | **✅ 全部通过** |

## 规格说明

详见 `openspec/changes/spec-1-start-page/` 目录：

- `proposal.md` - 提案文档
- `design.md` - 设计文档
- `specs/workspace-selection/spec.md` - 详细规格
- `tasks.md` - 任务清单（全部完成）

## 下一步

规格 2 将实现：
- 单仓主分支 + Sidebar
- tmux session 集成
- 终端渲染

## 许可证

MIT
