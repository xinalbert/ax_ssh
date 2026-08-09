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

- 阶段：实施中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PANE1 | completed | 确认既有未提交变更的提交边界，并定义窗口级有界窗格树和方向操作 | staged diff review、窗格树单元测试 | 窗格树只保存 UUID 与布局，不持有 Slint、worker、receiver 或秘密。 |
| PANE2 | in_progress | 为拆分创建独立 terminal session，并把输入、resize、scroll、selection 精确路由到目标 UUID | focused application/state tests、Cargo check | 不复制 live PTY/russh handle；SSH child沿正常认证流程且不继承短期秘密。 |
| PANE3 | pending | 主窗口与 detached 窗口渲染同一窗格布局，提供关闭、聚焦和行/列拆分意图 | Slint compile、Cargo check | SFTP 维持单一 companion surface；返回主窗口时保留该窗口的窗格布局。 |
| PANE4 | pending | 接入 `Alt+H/J/K/L` 焦点与 `Alt+Shift+H/J/K/L` 方向拆分快捷键 | input/bridge tests、目标平台手工验收 | macOS Option/Meta 语义保持可用，未知或被拒绝操作不吞掉普通终端输入。 |
| PANE5 | pending | 补齐双语文档、项目地图和月度记录，执行全量门禁并按逻辑范围提交 | Cargo/test/tracker/diff/staged review | 不引入参考项目源码、依赖或文档链接。 |

## 已完成

- 已完成环境复核：Rust 2024、MSRV 1.92.0、Cargo locked/offline、Slint 1.17.1、Tokio/russh 版本和 CI 契约未漂移；本机 `cargo fmt` 与 `cargo clippy` 子命令仍缺失。
- 已审阅参考产品的窗格产品行为和默认快捷键，但不会引入其代码、Cargo 依赖、生成输入或文档链接。
- 已确认 ownership：`AppState` 继续独占每个 Terminal Tab 及其 worker，`WindowRouter` 拥有每个 window 的 volatile pane tree，Slint 只接收有界 snapshot 并发出 UUID intent。SSH host-key trust、credential 和 transport API 不改变。
- 已完成窗口内 `PaneTree`：支持最多 8 个 terminal UUID 叶节点、左/右/上/下创建、基于标准化矩形的相邻焦点、关闭后折叠单子树和焦点恢复；定向单元测试通过。

## 验证

- 已完成：本轮计划、现有项目地图和环境证据审阅；`cargo test --locked --offline panes -- --nocapture` 通过；没有联网或多 agent。
- 未完成：拆分会话/定向 callback 回归、Slint/Cargo 联合编译、完整 locked/offline 测试、Rustfmt/Clippy（取决于本机子命令）、tracker/Markdown/diff 检查，以及主/独立窗口的目标平台视觉与键盘验收。

## 风险与阻塞

- 拆分只能创建独立会话，不能把一个 live PTY 或 SSH channel 绘制到多个窗格；否则输入、resize 和 worker lifetime 会失去单一所有权。
- 新 SSH child 不得复制未持久化密码或私钥口令，可能要求用户按既有流程认证。
- `Alt` 在 macOS 可作为终端 Meta；只有匹配已支持 pane 指令且操作可受理时才消费该组合。
- 主/独立窗口的真实焦点、resize、Return merge 和 SSH/SFTP worker 连续性须由目标平台用户验收。

## 下一步

- 为窗格拆分创建全新的本地或连接 terminal，并按 pane UUID 路由 terminal 意图。

## 最后更新时间

- 2026-08-09 22:00 CST
