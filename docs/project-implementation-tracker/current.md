# 当前项目实施记录

## 当前目标

- 目标 ID：20260818-xterm-mouse-capabilities-and-backpressure
- 目标：拆分终端按钮与滚轮 reporting 能力，按事件当前状态编码 release/cancel 修饰键，并避免高频 motion 填满通用 worker 输入队列。
- 交付物：button/wheel capability DTO、release/cancel modifier 修正、最新 motion 合并、motion 背压静默丢弃、协议/状态回归、双语契约和完整离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/{terminal}.rs`、`src/app/{state,state/tabs,terminal_bridge,terminal_render,view/{settings,terminal}}.rs`、`ui/{app,terminal-pane,components/terminal-grid}.slint`、双语使用/架构文档、研究记录和 tracker。
- 不在本轮范围内：按 tmux/vim/less/fzf 软件名特判、修改 transport 协议或 SSH trust、凭据、连接生命周期、配置 schema、依赖、锁文件或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| MB1 | completed | button/wheel reporting 能力拆分并贯通 Rust/Slint DTO | TerminalModel 状态回归、Cargo check | 1007 不再改变按钮 owner 或右键菜单。 |
| MB2 | completed | release 使用 pointer-up 修饰键；cancel 使用最后 pointer 修饰键 | Slint 编译、事件链静态回归 | 按钮身份仍由按下时 owner 保存。 |
| MB3 | completed | motion 最新坐标合并与背压静默丢弃 | 定向 bridge/协议回归、完整测试 | press/release 保持有序并保留错误反馈。 |
| MB4 | completed | 双语文档、项目地图和完整离线门禁 | fmt/check/Clippy/test/translation/tracker/diff | 真实 TUI/GUI 高频拖动留目标平台验收。 |

## 已完成

- 已确认当前 `mouse_reporting_active` 同时包含 1000/1002/1003 和备用屏 1007，导致只启用 alternate-scroll 时 UI 仍会取得按钮手势 owner。
- 已确认 xterm 与 xterm.js 均按 release/motion 事件当前 modifier state 编码，按钮 release 身份与 modifier 时态应分开保存。
- 已确认 SSH、Telnet、Serial 和 Local worker 的通用输入入口均为容量 32 的非阻塞队列；当前 bridge 对 motion 与 press/release 使用同一错误提示路径。
- 已拆分 button/wheel capability；普通屏只启用 1007 时两者均关闭，备用屏只启用 1007 时仅 wheel 开启，1000/1002/1003 同时启用 button 与 wheel。
- release 使用 pointer-up 当前修饰键，cancel 使用最近一次 pointer 状态；按钮身份和 gesture owner 仍由 pointer-down 锁定。
- Slint 每 16ms 只发送最新 motion，并在 press/release/cancel 前刷新；worker 对 motion Full 静默丢弃，对 Closed/Disconnected 返回错误，Tokio 队列保留一个可靠事件槽位。

## 验证

- 已完成：源码链路审阅、xterm 官方实现对照、`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 185、应用 173、Doc tests 0）、415 条翻译校验、19 份 Markdown 相对链接、tracker validator 和 `git diff --check`；tracker validator 只报告 37 条既有历史时间字段问题，本轮记录未新增错误。
- 未完成：目标平台 GUI/TUI 高频拖动、窗口失活 cancel、真实 tmux/vim/less/fzf 和本地/远程终端人工验收。

## 风险与阻塞

- motion 合并必须在 release/cancel 前同步刷新最后坐标，不能把 release 排到尚未发送的 motion 前面。
- 仅 motion 的正常队列背压可静默丢弃；worker 关闭、pane 路由失效及 press/release 失败仍必须保持可观察。
- Slint pointer capture 的真实窗口失活/cancel 时态仍需目标平台确认，自动化只能验证 Rust 协议与生成代码契约。

## 下一步

- 由用户在目标平台验收标准 xterm 模式、本地选区优先模式、滚轮、拖动、窗口失活 cancel 与真实 TUI 高频 motion。

## 最后更新时间

- 2026-08-18 17:33 +0800
