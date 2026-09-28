# 当前项目实施记录

## 当前目标

- 目标 ID：20260928-window-focus-redraw
- 目标：修复 Windows 窗口切回前台后的灰色残留，并理顺快捷键切换 Tab 后的终端键盘焦点。
- 交付物：活动 Tab 标题栏不再延迟夺取焦点；Windows 重获焦点时 software surface 完整重绘；双语架构、项目地图和验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/workspace-titlebar.slint`、`vendor/i-slint-backend-winit/winitwindowadapter.rs`、`vendor/softbuffer/src/backends/win32.rs`、双语架构与跟踪文件。
- 不在本轮范围内：`third_package/axshell`、SSH host-key/认证、终端 transport、用户 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| FOCUS1 | completed | 移除活动 Tab 的延迟自动聚焦 | Slint 重编译、完整离线 Rust 门禁 | 标题栏仍可键盘导航和鼠标聚焦 |
| REDRAW1 | completed | Windows 聚焦时 renderer 失效并重绘；Win32 buffer age 重置 | 代码路径与 cfg 审查 | 只在 Windows focus-in 执行 |
| REDRAW2 | completed | 双语架构、tracker 与最终门禁 | fmt/check/Clippy/test/diff/tracker/Markdown | Windows GUI 由用户验收 |

## 已完成

- 快捷键与鼠标 Tab 激活均进入 `WindowRouter::activate_tab`；此前标题栏活动 Tab 的 16 ms timer 会在终端聚焦后夺取键盘焦点，现仅保留自动滚动显示。
- Windows 默认 software backend 的 Win32 surface 使用局部 `BitBlt`；窗口被遮挡后可能没有 occlusion 事件。重获焦点时失效 renderer、重置 buffer age 并请求整窗重绘。

## 验证

- 已完成：最终 `cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、`cargo test --locked --offline`（库 285、应用 288、Doc tests 0）与 `git diff --check` 通过；tracker validator 本轮条目无报错。
- 未完成：Windows 原生 GUI 验收；tracker validator 仍被既有 8/9 月历史与 research 时间字段错误阻断。

## 风险与阻塞

- 本机没有 Windows SDK，不能替代 Windows runner 编译与原生窗口恢复验收。
- GUI 视觉由用户验收；代理不采集自身应用截图作为证据。

## 下一步

- 请用户在 Windows 上切换至其他窗口再切回，确认所有区域一次性恢复，并试验快捷键切回终端后直接输入。

## 最后更新时间

- 2026-09-28 10:48 +0800
