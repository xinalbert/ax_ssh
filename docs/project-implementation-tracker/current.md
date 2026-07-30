# 当前项目实施记录

## 当前目标

- 目标 ID：20260730-macos-settings-menu-retention
- 目标：修复 macOS 标准应用菜单中的 `Settings...` 消失问题，并把过大的 Rust/Slint 入口按功能拆分。
- 交付物：可持续保留的 macOS `Settings...`/About 原生入口；独立的 Rust 输入映射模块；Slint 标题栏、会话导航和安全弹窗组件；回归验证和实施记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app.rs`、`src/app/`、`ui/app.slint`、`ui/settings.slint`、`ui/settings/`、`ui/components/`、成对架构文档、项目地图和实施/环境记录。
- 不在本轮范围内：Windows/Linux 菜单视觉重构、侧边栏布局、设置 schema、SSH 认证、host-key、凭据、worker 或终端生命周期。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | macOS Settings 消失的根因与边界结论 | 锁定 Slint 1.17.1 winit/Muda 源码、运行日志和现有 AppKit bridge 审查 | Muda 在动态菜单属性变化时重建整棵原生菜单，删除额外插入项 |
| P2 | completed | 不受活动 Tab 状态重建影响的 macOS 应用菜单 | Slint/Cargo 联合编译与 AppKit 主线程/弱引用审查 | macOS 关闭 Tab 项静态，Windows/Linux 保留动态 enabled |
| P3 | completed | application bridge、Settings 分类和主窗口 Slint 功能拆分 | focused tests、Slint/Cargo 联合编译和文件规模复核 | MenuBar 受 Slint 语法约束保留在 Window 入口 |
| P4 | in_progress | 完整回归、运行时菜单检查和配套记录 | locked/offline check/test、格式、validator、差异检查和 macOS 进程检查 | 自动菜单点击仍受系统辅助功能权限影响 |

## 已完成

- 已确认初次 AppKit 菜单安装没有错误日志，问题不是菜单查找或插入失败。
- 已确认锁定的 Slint 1.17.1 Muda adapter 会跟踪 `MenuItem.enabled`；`active-tab-id` 或 `active-tab-kind` 变化会重建原生菜单，重建过程重新创建默认应用菜单，因此删除 AppKit bridge 额外插入的 `Settings...` 并解除 About 绑定。
- 已确认修复只需留在 UI/application bridge，SSH、安全、凭据和持久化边界均不变化。
- 已放弃定时重绑 workaround；macOS `Close Current Tab` 不绑定活动 Tab 动态状态，因此 Muda 不会因 Tab 身份/类型变化重建原生菜单，Settings/About 只由 AppKit bridge 安装一次。
- 已通过 `cargo check --locked --offline`，生成的 Slint callback、AppKit target 生命周期和主线程边界保持不变。
- 已完成结构拆分：`src/app.rs` 从约 2257 行降到 232 行，工作区、连接、worker monitor、终端、设置和 view 映射各自进入私有功能模块；状态转换与测试进入 `src/app/state/`。
- `ui/app.slint` 从约 866 行降到 399 行，标题栏、会话导航和安全弹窗进入组件；`ui/settings.slint` 从约 603 行降到 262 行，六类设置页面进入 `ui/settings/`。

## 验证

- 已完成：根指令、项目 skill/reference、环境记忆、锁定后端根因审查、功能拆分、连续 locked/offline check 和 5 个状态 focused tests。
- 未完成：完整测试、格式/clippy、运行时菜单保留检查、文档 validator 和最终差异审查。

## 风险与阻塞

- 无实现阻塞。macOS AppKit 操作必须继续只在主线程执行并只捕获 `Weak<AppWindow>`；原生菜单自动点击受 Screen Recording/Accessibility 权限限制。

## 下一步

- 运行完整回归、validator 和最新 macOS 进程/菜单检查。

## 最后更新时间

- 2026-07-30 07:49 +0800
