# 当前项目实施记录

## 当前目标

- 目标 ID：20261001-terminal-native-redraw-wakeup
- 目标：终端快照已经应用到 Slint model 后，在 Windows 和 Linux 上无需点击窗口即可及时呈现持续输出。
- 交付物：工作区终端更新后的跨平台 native redraw 唤醒、双语架构说明、环境与实施记录，以及 Rust 静态门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/view/workspace.rs`、终端刷新/renderer 相关双语架构说明、`docs/project-implementation-tracker/` 和 `docs/project-env-audit/`。
- 不在本轮范围内：终端 parser、transport worker、SSH trust/凭据策略、Slint UI contract、配置 schema、vendor 参考项目和 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| REDRAW1 | completed | 核对终端输出、presentation gate、Slint model 应用与 winit backend 的刷新链路 | 源码、项目地图和锁定 vendor backend | 问题发生在显示唤醒边界，不改 parser/transport |
| REDRAW2 | completed | 在终端 pane model 更新后请求 Windows/Linux native redraw | `cargo check`、Clippy、测试 | 复用 `WinitWindowAccessor`，保持 UI 线程调用 |
| REDRAW3 | completed | 同步双语架构、环境与实施记录并完成门禁 | fmt/check/Clippy/test、tracker、diff | Windows/Linux GUI 由原生环境用户验收 |

## 已完成

- 已核对输出从 transport worker、`TerminalPresentation`、`AppState` refresh gate 到 `workspace.rs` pane model 的链路；输出已到达 UI model，缺口在 native frame 唤醒边界。
- 在匹配的终端 pane 增量 model 更新后调用 `WinitWindowAccessor::with_winit_window` 和 `request_redraw()`；Windows/Linux 共用同一 application bridge 路径，其他平台保持一致。
- 修复只影响呈现唤醒，不改变 `TerminalSnapshot`、dirty-row 选择、parser、transport、SSH trust、凭据或有界 refresh gate；未修改 vendor backend。
- 已同步中英文架构说明、环境当前态和本轮实施记录；项目地图已有 `workspace.rs`、terminal presentation 和 winit backend 路由，无需结构刷新。

## 验证

- 已完成：fmt、locked/offline check、严格 all-target Clippy、完整测试（库 297、应用 298、Doc tests 0）、tracker validator 和 `git diff --check`。
- 未完成：本机仅安装 macOS targets，未运行 Windows/Linux 原生编译或 GUI；需对应 CI runner 和用户新构建验证持续输出无需点击即可显示。

## 风险与阻塞

- `request_redraw()` 是 native frame 唤醒请求，最终呈现仍由 Slint/winit backend 和平台 compositor 决定；Windows/Linux GUI 需要原生环境确认。
- 若平台仍不呈现，应继续采集对应 backend 的 redraw/present 事件；本轮不扩大到 parser、transport 或 vendor backend。

## 下一步

- 用户在 Windows 和 Linux 最新构建中保持 SSH/Local/Telnet/Serial 终端持续输出，确认无需点击窗口即可连续显示；同时检查窗口失焦、分屏和 detached window。

## 最后更新时间

- 2026-10-01 12:36 +0800
