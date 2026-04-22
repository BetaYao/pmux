# amux

**One native macOS workspace for every coding agent, every git worktree, and every parallel thread of execution.**

**一个真正为 AI 编程时代设计的原生 macOS 工作台，把 coding agent、git worktree 和并行任务收进同一个窗口。**

amux is built for people who no longer work in a single terminal tab.
If your day already looks like multiple repos, multiple branches, multiple agents, and a constant stream of runs, prompts, completions, and failures, amux turns that chaos into one coherent workspace.

如果你的日常已经不是“开一个终端写一天代码”，而是多个仓库、多个分支、多个 agent 同时推进，amux 就是把这套混乱重新组织起来的那个工具。

## Screenshots

### Dashboard

![Dashboard](assets/screenshots/1.png)

See every active worktree, every agent, and every status in one place.

一眼看到所有活跃的 worktree、agent 和状态，不再靠记忆切窗口。

### Focus Panel

![Focus Panel](assets/screenshots/2.png)

Go deep on one worktree, split panes, and run parallel threads without losing context.

在一个 worktree 里深入推进任务，同时保留 split pane 的并行能力。

### Notification History

![Notification History](assets/screenshots/3.png)

Track what just happened and jump back to the exact place that needs attention.

回看刚刚发生了什么，并快速跳回真正需要处理的现场。

### System Notification

![System Notification](assets/screenshots/4.png)

Notifications carry actual context: result, target, and the prompt that triggered it.

通知不再只是“有事发生了”，而是明确告诉你结果、目标和对应 prompt。

## 中文

### 为什么是 amux

AI 编程工具已经改变了开发方式，但大多数人的工作界面还停留在过去：

- 一个个终端窗口来回切
- 一堆 worktree 散落在 Finder 和 shell 里
- agent 在跑、在等、已经挂了，靠肉眼盯着看
- 收到通知时，只知道“有事发生了”，却不知道是哪一个 pane、哪一个分支、哪一次任务

amux 的目标很直接：

**把“和 coding agents 一起工作”这件事，做成一个完整、稳定、原生、可长期使用的 macOS 产品。**

### 它解决什么问题

amux 不是另一个终端皮肤，也不是一个轻量 wrapper。

它解决的是更真实的开发问题：

- 你可以同时管理多个仓库和多个 git worktree
- 你可以在一个 worktree 里拆多个 pane，让多个 agent 并行推进
- 你可以一眼看到谁在运行、谁在等你、谁已经完成、谁真的失败了
- 你可以从通知直接回到对应上下文，而不是重新找窗口、找 tab、找目录

当你的开发方式从“单线程 coding”变成“多 agent orchestration”，amux 才是那个合适的界面。

### 核心亮点

- 原生 macOS 体验，不是 Electron 套壳
- 基于 Ghostty 的终端能力，保留终端工作流的速度和手感
- Dashboard 一屏总览所有 worktree、agent 和状态
- Focus Panel 支持 split pane，在同一 worktree 内并行推进任务
- 通知按 pane 精确归因，不再是模糊的“某个任务完成了”
- Notification History 帮你追踪刚刚发生了什么，并快速跳回现场
- 面向真实工作流设计，适合 Claude Code、Codex 等 agent 并行使用

### 适合谁

- 重度使用 coding agent 的开发者
- 同时维护多个分支、多个 worktree 的个人和团队
- 想把 AI 编程从“试验玩法”变成“日常主工作流”的人

### 为什么不直接用 Terminal + tmux

当然，你可以继续靠终端、tmux、worktree 和脑内上下文管理一切。

但当任务开始并行、agent 开始增多、通知开始变得频繁时，纯命令行方案很快会暴露出几个问题：

- 状态是分散的，不是聚合的
- 通知是碎片化的，不是可导航的
- pane 在跑什么、哪个分支需要你、哪个任务刚结束，需要你自己拼上下文

amux 不是替代终端，而是把这些终端工作流抬到一个更适合 agent 时代的操作界面里。

### 安装

如果你只是想直接试用：

- 打开 GitHub Releases
- 下载对应架构的 `amux-macos-arm64.zip` 或 `amux-macos-x86_64.zip`
- 解压并启动应用

### 开发

本地构建：

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build
```

运行 UI 测试：

```bash
./run_ui_tests.sh
```

打当前机器架构的 release 包：

```bash
./scripts/package_release.sh
```

产物会输出到 `dist/`。

### GitHub Release

仓库已经包含 [`.github/workflows/release.yml`](.github/workflows/release.yml)。

- 推送 `v2.0.0` 这类 tag 会触发 release workflow
- workflow 会分别构建 `arm64` 和 `x86_64` 的 macOS 包
- 最终上传 `amux-macos-arm64.zip` 和 `amux-macos-x86_64.zip`

如果配置了下面这些 secrets，workflow 还会自动签名、notarize、staple：

- `APPLE_CERTIFICATE_P12`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_DEVELOPER_IDENTITY`
- `APPLE_ID`
- `APPLE_APP_SPECIFIC_PASSWORD`
- `APPLE_TEAM_ID`

### 发版流程

1. 更新 `project.yml` 里的版本号。
2. 提交并推送到默认分支。
3. 创建并推送 tag，比如 `git tag v2.0.0 && git push origin v2.0.0`。
4. 等待 `Release` workflow 完成。
5. 检查 GitHub Release 里的产物和说明。

## English

### Why amux

AI coding tools changed how we build software, but most developer interfaces still assume a much older workflow:

- too many terminal windows
- too many loose worktrees
- too many parallel agent runs with no clear status model
- too many notifications that tell you something happened, but not where or why

amux is built to fix that.

**It is a native macOS workspace for serious agent-driven development.**

Not a toy wrapper. Not another terminal theme. Not a dashboard that stops where real work begins.

amux gives you a single place to run, observe, and navigate multiple coding agents across multiple git worktrees, with pane-level precision and a UI designed for parallel execution.

### What makes it compelling

- Native macOS app, built for speed and clarity
- Ghostty-backed terminal surfaces
- A dashboard that shows the real state of your active work
- Split panes inside a worktree so multiple agents can move in parallel
- Status aggregation that answers the important question immediately: what needs my attention right now?
- Notification history that lets you jump back into context instead of hunting for the right window
- System notifications that are tied to the actual target and prompt, not vague generic messages

### Who it is for

- Developers already working with Claude Code, Codex, or similar coding agents
- People managing multiple branches and worktrees every day
- Teams turning AI-assisted coding from an experiment into a production workflow

### Why Not Just Use Terminal + tmux

You can keep juggling terminals, tmux sessions, worktrees, and mental bookkeeping.

That works for a while.

But once agent runs become parallel and notifications become constant, the cracks show:

- status is scattered instead of aggregated
- notifications are noisy instead of navigable
- context lives in your head instead of the interface

amux does not replace the terminal. It gives terminal-heavy, agent-heavy workflows a better operating surface.

### The Pitch

If your coding setup is becoming a swarm of terminals, branches, prompts, and half-finished agent runs, amux is the layer that makes it usable again.

It helps you move from:

- scattered terminals to one workspace
- implicit status to visible status
- noisy notifications to actionable notifications
- single-threaded development to parallel execution

### Install

If you just want to try it:

- Open GitHub Releases
- Download `amux-macos-arm64.zip` or `amux-macos-x86_64.zip`
- Unzip it and launch the app

### Development

Build locally:

```bash
xcodebuild -project amux.xcodeproj -scheme amux -configuration Debug build
```

Run UI tests:

```bash
./run_ui_tests.sh
```

Build a release zip for the current machine architecture:

```bash
./scripts/package_release.sh
```

Artifacts are written to `dist/`.

### GitHub Releases

This repository includes [`.github/workflows/release.yml`](.github/workflows/release.yml).

- Pushing a tag like `v2.0.0` triggers the release workflow
- The workflow builds both `arm64` and `x86_64` macOS artifacts
- It publishes `amux-macos-arm64.zip` and `amux-macos-x86_64.zip` to the GitHub Release

If the following repository secrets are configured, the workflow will also sign, notarize, and staple the app:

- `APPLE_CERTIFICATE_P12`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_DEVELOPER_IDENTITY`
- `APPLE_ID`
- `APPLE_APP_SPECIFIC_PASSWORD`
- `APPLE_TEAM_ID`

### Release Process

1. Update the version in `project.yml`.
2. Commit and push to the default branch.
3. Create and push a tag, for example `git tag v2.0.0 && git push origin v2.0.0`.
4. Wait for the `Release` workflow to finish.
5. Verify the GitHub Release assets and notes.
