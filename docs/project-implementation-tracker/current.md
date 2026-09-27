# 当前项目实施记录

## 当前目标

- 目标：修复 Windows、Linux、Wayland 等其他平台上系统文件管理器文件/文件夹拖入 SFTP Remote files 的命中和上传路由，并核对 SFTP Tab 内部拖动。
- 交付物：跨平台原生光标坐标桥接、无坐标安全拒绝、双语行为契约、项目地图与验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/native_file_drop.rs`、`src/app/terminal_bridge.rs`、`src/app.rs`、`Cargo.toml`、`Cargo.lock`、SFTP 拖放相关 UI/bridge 文档与跟踪文件。
- 不在本轮范围内：`third_package/axshell`、`AXtoolkit/`、SSH host-key/认证、SFTP worker 协议、用户 GUI 截图验收和非原生平台运行时验证。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| DROP1 | completed | 审查内部 SFTP 拖动、原生 DroppedFile 路由和上传目标重验 | 代码路径、测试与安全边界审查 | 无活动目录 fallback |
| DROP2 | completed | Windows/ Linux 原生光标坐标查询及 Wayland/Winit 回退 | 平台 cfg、坐标转换测试、Cargo 锁定依赖 | Windows/Linux 原生 GUI 交 CI |
| DROP3 | completed | 双语文档、地图、tracker 与完整离线 Rust 门禁 | fmt/check/Clippy/test/diff/tracker/Markdown | Windows/Linux 原生 GUI 交 CI/用户验收 |

## 已完成

- 已确认内部 Local files -> Remote files、Remote files -> Local files、系统文件管理器拖入都复用有界 SFTP transfer queue；目录递归、过滤、冲突批次和当前远端目录重验保持不变。
- macOS 继续在每个 `DroppedFile` 到达时读取 AppKit 光标；Windows 使用 Win32 `GetCursorPos`/`ScreenToClient`；Linux X11 使用 `XQueryPointer`；Wayland/其他平台使用当前外部 hover 的 Winit 坐标。
- 无坐标、落在本地 pane/其他区域、SFTP 状态未就绪时拒绝，不按活动目录猜测；新增模块不持有文件系统、worker、凭据或 transport。

## 验证

- 已完成：坐标转换与现有 SFTP/native drop 回归审查；`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 `cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（288 项应用测试，库测试合计 285，Doc tests 0）、`git diff --check`、Markdown 相对链接和 tracker validator 均通过；本机 Intel macOS target 的 locked/offline check 与严格 all-target Clippy 也通过。
- 未完成：Windows/Linux/Wayland 原生 GUI 行为未在本机执行；Linux target 因本机未安装 `x86_64-unknown-linux-gnu` 无法进行 target check。

## 风险与阻塞

- 本机仅有 macOS targets，无法替代 Windows/Linux 原生窗口拖动验收；CI 需覆盖 Windows、Linux X11/Wayland 构建和实际事件路径。
- Wayland 不提供统一的同步窗口指针查询，依赖外部拖动期间可用的 Winit `CursorMoved`；若没有坐标会安全拒绝上传。
- GUI 视觉和真实系统文件夹拖动仍需用户在对应平台验收；代理不采集自身应用截图作为证据。

## 下一步

- 在原生 Windows、Linux X11/Wayland 构建上拖入文件和文件夹，确认只在 Remote files 上传，并确认拖到 Local files、Terminal、空白区域均不上传。

## 最后更新时间

- 2026-09-26 Asia/Shanghai
