# 当前项目实施记录

## 当前目标

- 目标 ID：20260729-window-density-drag-regions
- 目标：按常见代码编辑器布局压缩 Activity Bar、侧栏和 macOS Tab 标题栏，并把窗口拖动限制到标题栏空白区域。
- 交付物：无冗余标题/未分组行的会话侧栏、版本化紧凑宽度默认值、34px macOS Tab 标题栏、Tab 横向滚动和 AppKit 精确拖动区域。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`src/config.rs`、`src/app.rs`、`src/app/macos_window.rs`、`ui/app.slint`、`ui/settings.slint`、双语架构/开发文档和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：Tab 重排、侧栏手动 resize、终端鼠标上报、SSH 认证/host-key、PTY 生命周期或持久化终端内容。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 参考图、现有 Slint/AppKit 行为和官方拖动语义核对 | 本机源码与 Apple 文档证据 | 不引入第二套窗口框架 |
| P2 | completed | 扁平未分组会话、紧凑 Activity Bar/侧栏和版本化宽度迁移 | config/app focused tests + Slint 编译 | 自定义宽度保持不变 |
| P3 | completed | macOS 全背景拖动关闭、仅标题栏空白区原生拖动 | Cargo 联合编译 + macOS GUI 走查 | Tab、侧栏和终端不注册拖动 callback |
| P4 | completed | 多 Tab 的滚动与独立标题栏留白命中 | Slint 编译 + viewport/drag 边界审查 | Tab 禁止鼠标拖拽，零 Tab 空白可拖窗口 |
| P5 | completed | 双语说明、完整门禁和差异审查 | Cargo/test/tracker/Markdown/diff | 记录自动化受限的手工边界 |

## 已完成

- 已确认参考布局的 Activity Bar 约 52px、macOS Tab 标题栏约 34px，当前 62px/38px/260px 默认值偏松。
- 已确认会话侧栏的 `Sessions` 标题和 `Ungrouped` 行是冗余层级；未分组 profile 可直接列出，命名分组仍保留折叠行。
- 已确认现有 Tab 使用 `Flickable`，但缺少把垂直滚轮转换为横向 viewport 移动的明确处理。
- 已确认 `setMovableByWindowBackground(true)` 使任意背景可拖动；AppKit 提供 `performWindowDragWithEvent` 支持从命中的标题栏区域交回系统窗口拖动。
- 已移除会话侧栏标题和未分组伪分组行；Activity Bar、macOS 顶栏、侧栏默认值分别收紧为 52px、34px、220px，侧栏下限为 180px。
- schema 7 仅迁移旧默认 260px，保留自定义宽度；Tab 滚轮/触控板增量统一钳制到横向 viewport 边界。
- macOS 已关闭整窗背景拖动；零 Tab 时中间空白可拖，有 Tab 时只保留最右侧 72px 专用拖动留白；Tab 的鼠标拖拽滚动已关闭，Activity Bar、侧栏和终端也没有窗口拖动调用路径。
- SSH host-key、凭据、worker、PTY 与终端输入安全边界未变化，未引入参考项目依赖或源码。

## 验证

- 已完成：项目 skill/references、tracker contract、参考截图和 AppKit/Slint API 核对；focused config/app tests；直接 `rustfmt --check`；`cargo check --locked --offline`；完整测试（库 44 passed、1 ignored；应用 15 passed）；Cargo metadata；零 Tab 和多 Local Shell 实际 macOS 窗口运行；参考耦合和 `git diff --check`。
- 未完成：本机未安装 `cargo-fmt` 与 `cargo-clippy` 子命令；系统辅助功能权限关闭，无法自动执行真实窗口拖动、Tab 溢出滚轮手势和逐区域负向命中测试，需目标平台手工验收。

## 风险与阻塞

- 无实现阻塞。原生拖动和触控板手势的端到端行为仍属于目标平台手工验收边界。
- 后续布局调整不得重新启用 `setMovableByWindowBackground(true)`，也不得把拖动 callback 放到 Tab、Activity Bar、侧栏或终端。

## 下一步

- 用最新二进制手工确认零 Tab 空白和右侧专用留白可拖、有 Tab 时 Tab 条不可拖，以及溢出 Tab 的触控板/滚轮横向滚动。

## 最后更新时间

- 2026-07-29 17:06 +0800
