# 当前项目实施记录

## 当前目标

- 目标 ID：20260810-terminal-pane-groups
- 目标：让一个可见 Terminal Tab 管理其内部的多个独立 terminal pane，分屏时不再向顶部 Tab 管理栏增加新 Tab。
- 交付物：分离可见 Tab 与焦点 pane 身份的窗口路由、每个窗口可保存多组 pane tree 的生命周期、整组关闭/拖动/detach/return 行为、回归测试、双语文档和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app.rs`、`src/app/{panes,state,view,workspace}.rs`、`ui/{app,workspace-shell}.slint`、相关状态/路由测试、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：复用同一个 PTY/SSH channel、SFTP 作为 terminal pane、持久化 pane layout、SSH trust/credential 规则、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PTAB1 | completed | 将窗口路由拆成可见 workspace Tab 与焦点 terminal pane，并为每个可见 Terminal Tab 保存独立 PaneTree | 路由单元测试、focused Cargo test | 子 pane 继续使用独立 UUID、worker、terminal model 和认证 phase。 |
| PTAB2 | completed | 让 Tab 列表、循环、拖动、关闭、detach/return 以 pane group 为单位 | 状态/窗口迁移回归、Cargo check | 关闭顶部 Tab 要关闭整棵 tree；SFTP companion 保持独立可见 Tab。 |
| PTAB3 | completed | 同步 Slint 身份契约、双语文档、项目地图和环境记录 | Slint compile、tracker/Markdown validator | 顶栏分屏按钮必须定向当前焦点 pane，而不是固定根 pane。 |
| PTAB4 | completed | 执行完整门禁、审阅提交范围并创建独立提交 | fmt/check/clippy/test/diff/staged review | GUI 视觉和真实 SSH/Telnet/Serial 生命周期由目标平台用户验收。 |

## 已完成

- 已按项目规范重新读取 AxSSH Rust/Slint skill、Rust/Slint references、架构文档、项目地图、当前环境记录和相关真实源码。
- 环境预检确认 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo locked/offline 和既有测试命令未漂移；本轮不需要新增依赖或联网。
- 已确认当前问题根因：`WindowRoute.active_tab_id` 同时承担顶部 Tab 与焦点 pane 身份，且单个 `pane_tree` 无法保存多个可见 Terminal Tab 的分屏组。
- 已将 route 拆为稳定的可见 `active_tab_id` 和 PaneTree 内部焦点；每个窗口可保存多组 tree，子 pane UUID 从 Tab model 隐藏。
- 已让 Tab 循环、拖动、关闭和 detach/return 按可见 pane group 工作；整组关闭不联动关闭独立 SFTP companion，Return 保留子 pane 焦点。
- 已新增 `active-pane-id` Slint 契约，顶部分屏按钮和 Terminal 输入继续定向当前焦点 pane；focused tests 与 `cargo check --locked --offline` 已通过。
- 已同步双语架构/使用说明、项目地图和环境记忆，并完成完整 locked/offline 测试与 staged commit 范围审阅。

## 验证

- 已完成：项目边界/工具链/所有权复核、单 Tab pane group 路由、Slint 身份拆分、整组生命周期、直接 Rustfmt、`cargo check --locked --offline`、完整 `cargo test --locked --offline`（库 141、应用 121、Doc tests 0）、tracker validator、44 个 Markdown 相对链接、`git diff --check` 和提交范围审阅。
- 未完成：目标平台 GUI、键盘焦点、主/独立窗口布局与真实 SSH/Telnet/Serial 生命周期人工验收；本机缺少 `cargo fmt` 和 `cargo clippy` 子命令，需由 CI 补充。

## 风险与阻塞

- 自动化已覆盖稳定 Tab ID、隐藏 pane、整组关闭、SFTP companion 和 detached Return；剩余风险是目标平台视觉/焦点与真实 transport 生命周期，只能由用户或 CI 验收。
- SSH 子 pane 仍走正常 host-key 与认证流程，不继承一次性密码或 private-key passphrase；本轮未改变安全策略。

## 下一步

- 等待目标平台确认分屏后顶部仍只有一个 Terminal Tab，以及主/独立窗口的焦点、快捷键、Return 和真实连接行为。

## 最后更新时间

- 2026-08-10 19:20 CST
