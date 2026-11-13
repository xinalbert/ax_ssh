# 当前项目实施记录

## 当前目标

- 目标 ID：20261113-macos-titlebar-startup
- 目标：按用户追加要求移除 macOS 整条原生标题栏，统一应用顶栏并修复原生初始化时序；整理本地提交。
- 交付物：无边框主/独立窗口、应用窗口/分屏控件、原生就绪初始化、回归、双语说明及本地提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：应用 window/native input bridge、macOS 原生标题栏移除、Slint 共用顶栏/布局、回归、架构/tracker 文档及本地提交。
- 不在本轮范围内：SSH/凭据/配置 schema、依赖升级、推送/tag/发布、用户真实会话或日志读取、GUI 启动/截图。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| M1 | completed | 截图和原生创建/定时器链路审计 | 锁定 Slint/winit 源码与调用顺序 | new_events 先处理 timer，resumed 才创建 native window |
| M2 | completed | 原方案：创建时标题栏属性与原生就绪初始化 | 作用域/错误清理回归、编译 | 创建属性与原生标题栏方案由用户后续选择的 M4 替代 |
| M3 | completed | 原方案：完整验证和双语说明 | fmt/check/Clippy/test、精确 macOS CI 命令、本机构建、文档/tracker/diff | 证据保存在月度记录，不代替新范围验证 |
| M4 | completed | 按用户选择移除整条原生标题栏 | 共用 Slint 控件、Mac/非 Mac 手势和窗口动作回归 | 5 项 window_chrome 回归通过，保留 macOS 菜单和 Command 语义 |
| M5 | completed | 新范围完整验证与双语说明 | Slint 重编译、完整 Rust 门禁、原生 CI 命令、本机构建、文档/tracker/diff | 最终实现全部本机门禁通过，视觉验收仍待用户 |
| M6 | completed | 整理本地提交 | 逐项暂存/审查、提交后工作区状态 | 修复提交 e999b7a；地图/验证记录另行归档 |

## 已完成

- 查看用户提供的截图，主窗口保留原生 AxSSH 标题文字且 Tab 行显示不完整；当前工作区起始干净。
- 发现主窗口和 detached 标题栏依赖 show 后 100ms 单次 timer。主窗口 configure 失败只记日志，且配置之后才恢复几何；backend 的 new_events 在 resumed 创建 native window 之前处理 timer。
- 原方案曾使用创建属性 hook，并完成本机门禁；用户随后明确选择移除整条原生标题栏，原方案及其专用标记测试已清理，历史证据保留在月度记录。
- 主/独立窗口由 Slint 从创建时声明为无边框，统一应用顶栏、窗口动作、系统身份图标和独立 Terminal 分屏控件；移除原生标题栏 subclass、NSButton、NSImageView 与 raster/tint 路径。
- 保留 macOS 系统菜单和 Command 语义；原生图标/菜单初始化与 placement 恢复在首个原生事件触发，移除两个 100ms 初始化 timer。
- 双语架构与地图已同步；新增 `window_chrome` 定向测试验证 Mac/非 Mac 的实际控件行为，未扩展 SSH/凭据、vendor 或持久化 schema 边界。
- 按授权保存 `e999b7a`，包含最终无边框修复、回归与双语架构；提交后只剩预期三份 tracker 记录，单独归档。

## 验证

- 已完成：源码/锁定版本/平台 cfg 审计；本机 rustc 1.99.0、原生 aarch64-apple-darwin，Windows SDK 限制沿用已验证事实。
- 已通过（最终实现）：5 项 window_chrome 定向回归、fmt、Slint 重编译和 locked/offline check、严格 all-target Clippy、1005 项 Rust 测试（461 library + 544 binary）、`cargo build --locked --offline`；精确 CI 命令 `cargo check --locked --target aarch64-apple-darwin` 和 `cargo clippy --all-targets --locked --target aarch64-apple-darwin -- -D warnings`；11 个翻译目录各 694 条文案和生成一致性、56 份 Markdown/源入口、tracker/diff 检查。原方案验证见月度历史。
- 定向测试按源语言和真实 item 几何定位，只验证数值和事件，不捕获或检查图像。暂存的 15 份实现/回归/双语说明已复核，余下只有三份预期 tracker 记录。
- 构建产物：`target/debug/ax_ssh`，未启动 GUI。
- 未完成：用户冷启动、窗口恢复及拖动/缩放/键盘/分离窗口的实际行为确认，其它平台原生 CI 未在本机运行。

## 风险与阻塞

- 无边框属性由 Slint 从创建时声明，原生句柄相关操作等待真实事件；保持单一 winit 事件过滤器与 weak UI 引用。移除依赖标准红绿灯/NSWindow 标题栏的按钮、图标和 subclass。
- 不启动应用或自制截图作视觉证据；其它平台原生 CI 与用户冷启动/窗口恢复验收仍需相应环境。
- AX Agent Context 当前没有提供可调用的 memory tools，继续使用仓库记录，不创建或绑定新记忆空间。

## 下一步

- 用户使用新构建确认冷启动、恢复工作区与窗口控件/缩放/键盘行为。当前本轮只保存本地提交，未推送或创建 release tag。

## 最后更新时间

- 2026-11-13 09:46 +0800
