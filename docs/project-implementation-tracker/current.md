# 当前项目实施记录

## 当前目标

- 目标 ID：20260929-desktop-menu-titlebar
- 目标：在 Windows/Linux 以应用菜单顶栏替代可见原生标题栏，保留快捷键、窗口动作和工作区几何。
- 交付物：无边框窗口顶栏、主/独立窗口控制、菜单动作接线、双语说明及验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/app.slint`、`ui/components/`、`src/app/window_bridge.rs`、窗口坐标接线、双语架构/使用说明与跟踪文件。
- 不在本轮范围内：`third_package/axshell`、macOS 原生标题栏行为、SSH host-key/认证、transport、持久化 schema 与用户 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| CHROME1 | completed | Windows/Linux 无边框顶栏、菜单与窗口控制 | Slint 编译、菜单/窗口静态核查 | macOS 仍使用原生标题栏 |
| CHROME2 | completed | 主/独立窗口几何、拖放和关闭生命周期接线 | Slint 编译及窗口代码核查 | 不改变 SSH/worker 边界 |
| CHROME3 | completed | 双语文档、完整门禁与目标平台验收项 | fmt/check/Clippy/test/diff/tracker | GUI 由用户验收 |

## 已完成

- 已核对当前 Slint/winit 后端、原生/客户区菜单差异，以及现有主窗口与独立窗口布局和关闭路由。
- 已确认 `no-frame`、`resize-border-width`、`WindowMoveArea`、`ContextMenuArea.show()` 与隐藏 `MenuBar` 保留快捷键均存在于锁定的 Slint 1.18.1。
- Windows/Linux 主窗口现由应用菜单和窗口控件共用顶栏；独立窗口提供 Return 与窗口控件。两个可见/隐藏菜单入口共用根组件动作函数，终端呈现及 SFTP 原生拖放坐标补偿 32px 顶栏。
- 中文译文目录与构建映射同步，双语架构、用法和项目地图已更新。

## 验证

- 已完成：Slint 重编译、fmt、locked/offline check、严格 Clippy、翻译目录更新后的完整测试（库 288、应用 290）、中文目录生成/校验、`msgfmt --check`、相对 Markdown 链接及 `git diff --check`。tracker validator 已执行，本轮新增记录无报错。
- 未完成：Windows/Linux 原生 GUI 验收。Windows target check 因本机缺 Windows SDK 头文件，在 `aws-lc-sys` 停止；Linux target 未安装。tracker validator 仍由既有 8/9 月历史与 research 字段错误阻断。

## 风险与阻塞

- 自绘标题栏须处理主/独立窗口的关闭、最大化、拖动、缩放，以及顶栏引入后的终端和 SFTP 坐标偏移。
- Windows 11 最大化按钮悬停的 Snap Layout、Linux X11/Wayland 的窗口管理行为须在原生平台验收。
- GUI 视觉由用户验收；代理不采集自身应用截图作为证据。

## 下一步

- 请用户在 Windows/Linux 确认菜单、拖动、缩放、最大化、关闭及独立窗口返回的视觉与行为。

## 最后更新时间

- 2026-09-29 18:01 +0800
