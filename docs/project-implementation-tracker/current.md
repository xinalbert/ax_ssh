# 当前项目实施记录

## 当前目标

- 目标 ID：20260810-terminal-divider-drag-focus-fix
- 目标：修复 Terminal 分屏 divider 无法持续拖动，以及操作 divider 后 terminal 输入焦点未恢复的问题。
- 交付物：保持 Slint repeater 实例的 pane/divider 几何更新、拖动结束的 focused pane 输入恢复、定向测试、双语文档和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/{workspace-shell,terminal-pane}.slint`、`src/app.rs`、`src/app/{terminal_bridge,view}.rs`、相关测试、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：PaneTree 持久化、分屏方向/数量规则、SFTP splitter、通用 TextInput 焦点桥、终端 buffer/worker/PTY/SSH 生命周期、SSH trust/credential 规则、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TDRAG1 | completed | 对齐 Terminal、SFTP splitter 与 Slint repeater 更新生命周期 | 静态路由与锁定 Slint 1.17.1 model API 复核 | 已确认全量替换 divider model 会让 pressed 组件失去稳定实例。 |
| TDRAG2 | completed | 用原 model 行更新发布 PaneTree 几何，并在 pointer release/cancel 后恢复 focused terminal 输入 | Cargo check、model 定向测试 | 键盘/无障碍 divider 焦点继续保留；只恢复鼠标拖动后的 terminal 输入。 |
| TDRAG3 | completed | 同步双语行为契约、项目地图和环境记录 | tracker/Markdown validator | 不改变持久化 schema、依赖或 SSH 安全边界。 |
| TDRAG4 | completed | 执行完整门禁、审阅并创建独立提交 | fmt/check/clippy/test/diff/staged review | 真实鼠标拖动与输入由用户在目标平台验收。 |

## 已完成

- 已读取工程规则、AxSSH Rust/Slint skill/references、项目地图、环境记忆和现有 Terminal/SFTP splitter、pane focus 与 model 发布实现。
- 环境预检确认 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo locked/offline 和 141/125 测试基线未漂移；本机仍缺 Cargo fmt/Clippy 子命令。
- 已修复拖动根因：`WindowRouter::resize_terminal_divider` 返回同一 `PaneTree` 的 `PaneLayout`；bridge 只在 pane UUID、divider ID/方向和 model 行数均一致时调用 `set_row_data` 原地发布几何，保留 pressed divider 的 repeater 实例，否则回退 `refresh_workspace`。
- 已修复输入根因：divider 在 pointer release/cancel 后仅递增 Slint-local 有界 revision；当前 focused、connected `TerminalPane` 因而重新聚焦透明 IME `TextInput`。键盘和无障碍 divider 操作不会触发此恢复。
- 已核对 Slint 1.17.1 `ModelRc::set_row_data` 与 repeater `row_changed`：更新现有行会调用组件 `update` 而不替换实例，适合在同一 PaneTree shape 内发布拖动几何；`cargo check --locked --offline` 已重新编译整个 Slint 图，新增 2 项 model identity 定向测试和完整 141/127 Cargo 测试通过。
- 本轮保持 `PaneTree` 为比例和几何唯一 owner；UI 只增加一次有界 focus request revision，不保存比例副本，不跨模块传递 worker、buffer、锁或秘密。

## 验证

- 已完成：项目边界/环境审计、drag/focus 生命周期和 Slint model API 复核、实现、`cargo check --locked --offline`、2 项 model 定向测试、完整 `cargo test --locked --offline`（库 141、应用 127、Doc tests 0）、直接 Rustfmt、tracker validator、44 个 Markdown 相对链接目标和 `git diff --check`；提交前 staged 审阅完成。
- 未完成：目标平台主窗口/独立 Terminal 的连续拖动、宽高调整、拖动后直接键入、键盘/无障碍 divider 和真实 PTY resize 人工验收；`cargo fmt`、`cargo clippy` 需由安装组件的 CI/目标环境执行。

## 风险与阻塞

- 已保持：拖动更新先验证 pane UUID、divider ID/方向和 model 行数全部匹配才原地写行，避免 stale callback 局部更新错误 Tab。
- 已保持：鼠标 release/cancel 后只请求当前 `focused`、connected pane 的 IME 输入焦点；Tab 键、方向键和无障碍 action 操作 divider 时不能强制跳回 terminal。
- 本轮不改变 SSH 安全边界、秘密输入、配置 schema、worker 生命周期或依赖。
- 本机没有 `cargo fmt` 与 `cargo clippy` 子命令；直接 Rustfmt 已通过，完整格式和 lint 仍由安装组件的 CI/目标环境补充。

## 下一步

- 请用户在主窗口和 detached Terminal 验收连续拖动、宽高调整及拖动后直接键入；CI/目标环境补跑 Cargo fmt 与 Clippy。

## 最后更新时间

- 2026-08-11 CST
