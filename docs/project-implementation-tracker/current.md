# 当前项目实施记录

## 当前目标

- 目标 ID：20260731-module-boundary-refactor
- 目标：按现有 Rust/Slint 所有权边界拆分过大的配置、连接与终端 UI 模块，同时保持行为与安全契约不变。
- 交付物：职责单一的现代 Rust 子模块、聚合后的 Slint 子组件、保持稳定的根入口/DTO/callback 契约、回归测试和更新后的项目地图。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/config.rs` 及其子模块、`src/app/connection.rs` 及其子模块、`ui/terminal-pane.slint` 的内部组件，以及 `docs/{architecture,architecture.zh}.md` 和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：SSH 信任判定、凭据 schema/秘密生命周期、Tokio worker 行为、Slint 根 `AppWindow` 契约、依赖升级、参考子模块耦合和 GUI 视觉验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| R1 | completed | `config` 拆分为主题、设置、会话域、持久化和测试子模块 | config focused tests、Cargo check | 根 `config.rs` 仅保留明确入口/re-export |
| R2 | completed | 拆分 SSH 连接回调、探测与认证流程 | application state tests、Cargo check | 不改变逐 Tab phase 或 host-key 默认拒绝 |
| R3 | completed | 以内部 Slint 组件收敛终端绘制/输入/选择职责 | Slint 联合编译、静态输入契约检查 | `TerminalPane` 对 `WorkspaceShell` 的接口不变 |
| R4 | completed | 文档/地图/完整验证收口 | Rust/Cargo/tracker/Markdown/diff | 不创建 Worker 记录；GUI 视觉由用户验收 |

## 已完成

- 已完成根指令、Rust/Slint 规范、项目地图、架构契约和代码体积盘点；不使用联网或多 agent。
- `src/config.rs` 已收敛为入口、schema 常量与显式 re-export；`persistence`、`session`、`settings`、`theme` 和 config 回归测试各有独立模块，现有 JSON schema 与公开 config 路径保持稳定。
- `src/app/connection.rs` 已收敛为连接组装入口；请求/probe、host-key 确认、认证和 worker 启动分离，同时保留逐 Tab phase 重验、未知或变化主机密钥默认拒绝，以及短期秘密边界。
- `ui/components/terminal-grid.slint` 已接管有界终端快照的绘制、光标/IME preedit 覆盖、选择呈现与指针/菜单意图；`TerminalPane` 保留焦点、IME 代理、选择草稿、尺寸合并和对外 callback，`TerminalViewState` 及上层接口未改变。

## 验证

- 已完成：config focused tests（29 passed）；直接 `rustfmt --edition 2024 --check`；`cargo check --locked --offline`；完整 `cargo test --locked --offline`（库 63 passed、应用 33 passed、Doc tests 通过）；Markdown 相对链接、tracker validator 和 `git diff --check`。
- 未完成：`cargo fmt --all -- --check` 与 `cargo clippy --all-targets --locked --offline -- -D warnings`，本机未安装对应 Cargo 子命令；未进行 GUI 截图或平台交互验收。

## 风险与阻塞

- 该轮为结构重构，不得改变 SSH host-key 默认拒绝、短期秘密清零、worker 队列上限或已持久化 JSON schema。
- Slint 文件拆分会保留既有根 callback/property 名称；视觉与焦点验收仍需用户在目标平台完成。

## 下一步

- 等待用户在目标平台手动确认终端网格的渲染、焦点、拖拽选区、右键菜单、IME preedit 与窗口 resize；后续功能改动继续从各模块入口定位到对应的单一职责子模块。

## 最后更新时间

- 2026-07-31 17:36 +0800
