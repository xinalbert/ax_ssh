# 当前项目实施记录

## 当前目标

- 目标 ID：20261102-tooltip-size-surface
- 目标：修复用户截图中的 Tab hover 提示外框宽度与内部背景不一致。
- 交付物：共享 tooltip 明确尺寸约束、单一可见外框背景、既有 hover 回归补充及双语说明。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/elided-controls.slint`、`ui/theme.slint` 的说明、`src/app/view/tests/text_overflow.rs`、双语架构与 tracker；保留工作区中前轮终端光标/重绘改动并一并验证。
- 不在本轮范围内：前轮光标呈现、SSH/transport、配置与标准 Slint/vendor 源码。
- 所有权与安全：Slint 只调整 tooltip 布局和视觉表面；保留标准 hover/弹窗生命周期、焦点、点击和不泄露秘密正文的既有契约。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| T1 | completed | 确认外层标准 Tooltip 与自定义内容的布局问题 | 用户截图、锁定 Slint 1.18.1 源码、真实 popup 数值尺寸 | 不截图，不修改 Slint/vendor |
| T2 | completed | 内容尺寸约束和单一可见表面 | 原有 tab hover 回归覆盖长路径、宽度、背景、焦点和点击 | 共享 ElidedTooltipContent，保留 Sidebar 自定义卡片范围 |
| T3 | completed | 本机完整门禁和交付记录 | fmt/check/严格 Clippy/test/build、Markdown/skill/tracker/diff | GUI 最终由用户截图和确认验收 |
| T4 | completed | 修复主题 Tooltip 表面并完成门禁 | focused/full test、fmt/check/严格 Clippy、Markdown/tracker/diff | GUI 最终由用户截图和确认验收 |

## 已完成

- 锁定 Slint 1.18.1 会给自定义 Tooltip 包装 ToolTipImpl 外框；修改前既有 Tab 回归新增断言稳定复现两层不透明背景（预期 1、实际 2）。
- 共享内容统一实际/min/preferred/max 尺寸与内边距，并用主题表面覆盖同尺寸标准 Tooltip 外框。
- 用户复验发现标准 widget 外框使用平台色，与 AxSSH 主题不一致；共享内容现以相同几何覆盖标准外框并使用 `Theme.tooltip-*` 和 `Theme.text`。
- 保存先前工作区 diff 为忽略的 target 基线；光标与原有改动保留。

## 验证

- 已完成：修改前双背景数值失败复现、Slint 重编译、5 项 text_overflow 回归；`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 all-target Clippy、完整 `cargo test --locked --offline`（404 library + 516 binary，共 920 项通过，Doc tests 0）、`cargo build --locked --offline`、Markdown/源入口检查、tracker validator 和 `git diff --check`；长路径/中文/短标题仅一个可见背景，popup 宽度受限，原 focus/click/秘密提示回归通过。
- 未完成：当前修复后的实际 GUI 外观仍待用户在目标 macOS 会话中确认；其它 CI 原生平台仍不在本机验证范围内。项目 skill validator 已在 `python_test` 环境尝试，但共享 skill 仓库的元数据校验失败项与 AxSSH 本项目无关。

## 风险与阻塞

- GUI 视觉验收由用户完成；仅做实际 popup 几何、item 属性及输入交互回归。
- 前轮持续 TUI 光标已完成本机 920 测试门禁，原场景实机效果仍待用户确认；本轮不修改该实现。

## 下一步

- 新构建位于 `target/debug/ax_ssh`，由用户切换/重启到该构建验收。

## 最后更新时间

- 2026-11-02 13:52 +0800
