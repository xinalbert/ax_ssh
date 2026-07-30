# 当前项目实施记录

## 当前目标

- 目标 ID：20260730-tab-drag-visual-feedback
- 目标：让拖动中的工作区 Tab 跟随鼠标，并持续显示源槽与目标槽。
- 交付物：Slint 拖拽视觉状态、双语架构说明和回归验证。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/workspace-titlebar.slint`、`ui/theme.slint`、`docs/architecture*.md`、`docs/project-implementation-tracker/`。
- 不在本轮范围内：Tab 排序域状态、配置 schema、终端模型、PTY/SSH worker、macOS 原生窗口设置、SSH 信任/认证和凭据。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 跟随鼠标的 Tab 副本、源槽与目标槽 | Slint/Cargo 联合编译 | 视觉状态只在 WorkspaceTitlebar 内存中存在 |
| P2 | completed | 双语文档、tracker 和完整回归 | locked/offline Cargo、tracker、Markdown 链接与差异检查 | 不改变 Tab 排序或运行时 Tab 生命周期 |

## 已完成

- 拖动中的 Tab 显示为跟随鼠标的不可交互副本，源位置保持半透明占位，目标位置保留强调边框。
- 工作区 Tab 的内存排序、UUID、稳定实例编号和 worker 生命周期均未调整。
- 标准原生标题栏、Tab 手势隔离和最右侧已保存 SSH 连接选择器保持原有行为。

## 验证

- 已完成：根指令、AxSSH Rust/Slint skill、项目地图、双语架构与 Slint 手势路径审查；`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 47 passed、1 ignored；应用 21 passed）、tracker validator、Markdown 相对链接与 `git diff --check`。
- 未完成：`cargo fmt --all -- --check` 与 `cargo clippy --all-targets --locked --offline -- -D warnings` 因本机未安装对应 Cargo 组件无法执行；实际 macOS 拖拽视觉与手势仍需用户验收。

## 风险与阻塞

- 浮动副本限制在可滚动 Tab 视口内，避免覆盖最右侧的保存 SSH 连接 `+`；实际 macOS 指针跟随与窄窗口下的截断位置仍需用户验收。

## 下一步

- 用户在 macOS 上确认 Tab 副本随指针移动、目标槽正确提示，且拖拽不移动窗口。

## 最后更新时间

- 2026-07-30 16:11 +0800
