# 当前项目实施记录

## 当前目标

- 目标 ID：20260810-resizable-terminal-dividers
- 目标：让同一 Terminal Tab 内的 pane 始终有清晰分隔线，并可通过分隔线调整宽度和高度。
- 交付物：Rust-owned 有界 split ratio/divider 快照、主/独立窗口共用的可拖拽且可键盘操作的 Slint divider、回归测试、双语文档和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app.rs`、`src/app/{panes,terminal_bridge,view}.rs`、`ui/{app,workspace-shell,theme}.slint`、相关测试、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：pane 持久化、SFTP splitter、关闭 pane、重排 pane、worker/PTY/SSH 生命周期、SSH trust/credential 规则、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TDIV1 | completed | 为 `PaneTree` 增加有界 split ratio、divider 布局快照及调整入口 | pane 单元测试、直接 Rustfmt | 比例只在运行时保存，必须拒绝非法 divider/非有限值并限制最小 pane 比例。 |
| TDIV2 | completed | 增加 Slint 分隔线、拖拽/键盘/双击交互并贯通主/独立窗口 | Slint compile、路由测试、Cargo check | divider 只绘制一次；普通态用 `Theme.divider`，活动态用 `Theme.accent`。 |
| TDIV3 | completed | 同步双语架构/使用说明、项目地图和环境记录 | tracker/Markdown validator | 明确布局不持久化，Tab 切换及 detach/return 保留当前进程内比例。 |
| TDIV4 | completed | 执行完整门禁、审阅并创建独立提交 | fmt/check/clippy/test/diff/staged review | 拖拽、键盘焦点、最小尺寸和真实 PTY resize 由用户在目标平台验收。 |

## 已完成

- 已读取工程规则、AxSSH Rust/Slint skill 及 references、项目地图、双语架构和现有 pane/terminal callback 实现。
- 环境预检确认独立 Rust 2024 项目、MSRV 1.92.0、Slint 1.17.1、Cargo locked/offline 和 141/121 测试基线未漂移；本机仍缺 Cargo fmt/Clippy 子命令。
- 已确认当前视觉缺口：`TerminalPane` 只有焦点 pane 的 accent 外框，`TerminalPaneGroup` 没有内部 split divider；`PaneNode::Split` 也固定按 0.5 布局，无法保存拖拽比例。
- 已确定由 `PaneTree` 保存有界 volatile ratio 并发布内部 split divider；Slint 只显示和发送调整意图，主窗口与 detached 窗口复用同一组件和路由。
- 已为每个 split 增加默认 0.5、限制在 0.1-0.9 的 volatile ratio，并按树的前序生成最多 7 个稳定 divider ID；非法 ID、NaN 和无变化调整不会触发布局刷新。
- 已补充根/嵌套 split 比例、边界、方向和 divider identity 测试，直接 Rustfmt 与 6 项 pane 定向测试通过。
- 已增加只绘制一次的 Terminal divider overlay：8px 稳定命中区、1px 普通线、2px accent 活动态、resize 光标、鼠标拖动、方向键、Home/End、双击/Enter/Space 复位及 slider 无障碍语义。
- 已贯通 `TerminalPaneDividerView`、主/独立 `WorkspaceShell`、`AppWindow` 与 `WindowRouter` callback；`cargo check --locked --offline` 和比例跨 Tab/detach/return 路由回归通过。
- 已同步双语架构/使用说明和项目地图，明确比例的 10%-90% 边界、当前运行期生命周期及既有 terminal resize 复用。
- 已记录本轮没有依赖、锁文件、工具链、MSRV 或 CI 变化；项目地图无需新增文件，仅刷新既有职责和 divider 定位。
- 已完成 locked/offline 全量门禁：库 141、应用 124、Doc tests 0；直接 Rustfmt、Cargo check、tracker、44 个 Markdown 相对链接和 diff check 通过。

## 验证

- 已完成：项目/环境/工具链与 ownership 复核、Rust ratio/divider 布局与 6 项 pane 测试、Slint/窗口路由接线、Cargo check、跨 Tab/detach/return 回归、完整 Cargo test、双语架构/使用说明、项目地图、环境记录、tracker/Markdown/diff 门禁和独立提交审阅。
- 未完成：本机没有 `cargo fmt` 与 `cargo clippy` 子命令；分隔线视觉、鼠标宽高拖动、键盘焦点/操作、无障碍读取和真实 PTY resize 需用户在目标平台验收。

## 风险与阻塞

- 自动化已覆盖稳定 divider identity 和 10%-90% 比例边界；目标平台拖拽过程中 Slint model 刷新后的指针连续性仍需人工确认。
- 布局状态属于当前进程的 `WindowRouter`，不进入配置、worker 或凭据；本轮不改变 SSH 安全边界。

## 下一步

- 在目标平台验收主窗口和 detached Terminal 的分隔线、宽高拖动、键盘/无障碍操作与真实 PTY resize。

## 最后更新时间

- 2026-08-10 21:00 CST
