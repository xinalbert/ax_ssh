# 当前项目实施记录

## 当前目标

- 目标 ID：20260809-terminal-pane-splitting
- 目标：让主窗口和 detached Terminal 窗口都能将终端按行或列拆分，并提供与参考产品一致的方向焦点和拆分快捷键。
- 交付物：有界且可测试的窗口内窗格树、每窗格独立 Terminal Tab/worker、按 Tab UUID 定向的终端 callback、主窗口与 detached 窗口共享的窗格 UI、快捷键、双语说明、回归测试和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app.rs`、新增 `src/app/panes.rs`、`src/app/{state,terminal_bridge,view,workspace}.rs`、`ui/{app,workspace-shell,terminal-pane}.slint`、相关单元测试、双语架构/使用文档和项目实施记录。
- 不在本轮范围内：SFTP 分栏、同一 PTY/SSH channel 的双重渲染、worker/russh handle 迁移、配置持久化恢复、SSH trust/credential 契约、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PANE1 | completed | 确认既有未提交变更的提交边界，并定义窗口级有界窗格树和方向操作 | staged diff review、窗格树单元测试 | 窗格树只保存 UUID 与布局，不持有 Slint、worker、receiver 或秘密。 |
| PANE2 | completed | 为拆分创建独立 terminal session，并把输入、resize、scroll、selection 精确路由到目标 UUID | focused application/state tests、Cargo check | 不复制 live PTY/russh handle；SSH child 沿正常认证流程且不继承短期秘密。 |
| PANE3 | completed | 主窗口与 detached 窗口渲染同一窗格布局，提供关闭、聚焦和行/列拆分意图 | Slint compile、Cargo check | SFTP 保持独立 surface；返回主窗口时保留该窗口的 pane layout。 |
| PANE4 | completed | 接入 `Alt+H/J/K/L` 焦点与 `Alt+Shift+H/J/K/L` 方向拆分快捷键 | pane/state tests、目标平台手工验收 | macOS Option/Meta 语义保持可用，未知或被拒绝操作不吞掉普通终端输入。 |
| PANE5 | completed | 补齐双语文档、项目地图和月度记录，执行全量门禁并按逻辑范围提交 | Rustfmt/Cargo/test/tracker/Markdown/diff/staged review | 不引入参考项目源码、依赖或文档链接。 |

## 已完成

- 已完成环境复核：Rust 2024、MSRV 1.92.0、Cargo locked/offline、Slint 1.17.1、Tokio/russh 版本和 CI 契约未漂移；本机 `cargo fmt` 与 `cargo clippy` 子命令仍缺失。
- 已审阅参考产品的窗格产品行为和默认快捷键，但不会引入其代码、Cargo 依赖、生成输入或文档链接。
- 已确认 ownership：`AppState` 继续独占每个 Terminal Tab 及其 worker，`WindowRouter` 拥有每个 window 的 volatile pane tree，Slint 只接收有界 snapshot 并发出 UUID intent。SSH host-key trust、credential 和 transport API 不改变。
- 已完成窗口内 `PaneTree`：支持最多 8 个 terminal UUID 叶节点、左/右/上/下创建、基于标准化矩形的相邻焦点、关闭后折叠单子树和焦点恢复；定向单元测试通过。
- 拆分 Local Shell 会新建 PTY；SSH、Telnet 与 Serial 会新建 profile connection。SSH child 不继承 one-time password 或 private-key passphrase，仍按正常 host-key/认证流程运行。
- 主窗口和 detached Terminal 共同使用 `TerminalPaneGroup`。每个可见 pane 的输入、resize、scroll 和 selection callback 都带 Tab UUID，并在 `WindowRouter` 中校验所有权；SFTP 不可分屏，但可以保持单独的 detached view。
- detached Return/关闭会恢复同一 pane tree，不会停止 worker 或重连。关闭 terminal Tab 会移除该 leaf 并折叠单子树分支。
- 已更新双语 architecture/usage、项目地图、环境审计和月度记录；没有联网或使用多 agent。
- 已完成直接 Rustfmt、完整 locked/offline Cargo check/test、tracker validator、46 个仓库 Markdown 相对链接和 `git diff --check`；库测试 141 项、应用测试 116 项、Doc tests 0 项均通过。

## 验证

- 已完成：本轮计划、现有项目地图和环境证据审阅；pane、会话隔离、UUID resize、terminal/SFTP companion transfer 与 standalone SFTP detach 回归；直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 116、Doc tests 0）、tracker validator、46 个 Markdown 相对链接和 `git diff --check`。`cargo fmt` 与 `cargo clippy` 已实际尝试，但本机缺少对应 Cargo 子命令。
- 未完成：主/独立窗口的真实焦点、Alt 快捷键、pane resize、原生 Return，以及实际 SSH/Telnet/Serial 生命周期仍需目标平台用户验收；按项目约束未自行截图替代该验收。

## 风险与阻塞

- 拆分只能创建独立会话，不能把一个 live PTY 或 SSH channel 绘制到多个窗格；否则输入、resize 和 worker lifetime 会失去单一所有权。
- 新 SSH child 不得复制未持久化密码或私钥口令，可能要求用户按既有流程认证。
- `Alt` 在 macOS 可作为终端 Meta；只有匹配已支持 pane 指令且操作可受理时才消费该组合。
- 主/独立窗口的真实焦点、resize、Return merge 和 SSH/SFTP worker 连续性须由目标平台用户验收。

## 下一步

- 本轮实现与自动化门禁已完成；等待目标平台 GUI 和实际连接生命周期反馈。

## 最后更新时间

- 2026-08-09 23:55 CST
