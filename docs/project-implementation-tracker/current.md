# 当前项目实施记录

## 当前目标

- 目标 ID：20260810-cross-platform-terminal-edit-menu
- 目标：为跨平台菜单栏补齐 Terminal Copy、Paste、Select All，并提供不破坏终端控制键语义的常规快捷键。
- 交付物：上下文受限的 Edit 菜单、活动 pane 定向的 Slint 本地编辑命令、跨平台 accelerator、回归测试、双语文档和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/{app,workspace-shell,terminal-pane}.slint`、`src/app/{input,diagnostics,view}.rs`、相关测试、`docs/{architecture,architecture.zh,usage,usage.zh}.md`、`docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：通用 TextInput 焦点编辑桥、Cut/Undo、快捷键配置 schema、detached 客户区菜单栏、终端 buffer/worker/PTY/SSH 生命周期、SSH trust/credential 规则、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| EDIT1 | completed | 审核菜单、终端快捷键、普通/秘密文本输入边界并确定平台默认值 | 静态路由与锁定 Slint API 复核 | macOS 用 Cmd；Windows/Linux 保留 terminal 的 Ctrl+Shift 约定。 |
| EDIT2 | completed | 补齐 Edit 菜单并将 Copy/Paste/Select All 只发送给活动 terminal pane | Cargo check、菜单/快捷键定向测试 | 不让全局菜单抢占 Settings、会话编辑器或秘密输入。 |
| EDIT3 | completed | 同步双语菜单/使用契约、项目地图和环境记录 | tracker/Markdown validator | Select All 使用固定平台快捷键，不扩展持久化 schema。 |
| EDIT4 | completed | 执行完整门禁、审阅并创建独立提交 | fmt/check/clippy/test/diff/staged review | 原生菜单显示、焦点与 detached 快捷键由用户在目标平台验收。 |

## 已完成

- 已读取工程规则、AxSSH Rust/Slint skill/references、项目地图、环境记忆和现有菜单/终端输入实现。
- 环境预检确认 Rust 2024、MSRV 1.92.0、Slint 1.17.1、Cargo locked/offline 和 141/124 测试基线未漂移；本机仍缺 Cargo fmt/Clippy 子命令。
- 已确认缺口：Terminal 右键菜单已有 Copy/Paste/Select All，键盘已有可配置 Copy/Paste，但顶部 Edit 菜单只有无效的 disabled Undo，且 Select All 没有键盘入口。
- 已确认普通文本输入已有自身编辑快捷键/右键菜单，秘密输入故意禁止复制；本轮 Edit 菜单只在活动 Tab 为 Terminal 时启用。
- 已确定菜单命令通过 Slint 局部 command + revision 信号广播，由唯一 focused `TerminalPane` 执行；detached 窗口继续使用 pane 自身键盘处理，不新增客户区菜单。
- 已移除永久 disabled Undo，增加 terminal-only Copy/Paste/Select All 菜单项；Copy/Paste 复用配置快捷键，Select All 固定使用 macOS `Cmd+A`、Windows/Linux `Ctrl+Shift+A`。
- 已把命令/修订值贯通主窗口与 detached 的共用 `TerminalPaneGroup`，只有 focused pane 调用既有局部 copy/paste/select-all 函数；detached 通过 pane 键盘路径支持 Select All。
- 已扩展固定 menu action 白名单和跨平台 `slint::Keys` 映射测试；直接 Rustfmt、Slint/Rust `cargo check --locked --offline` 及 3 项定向测试通过。
- 已同步双语架构/使用说明和项目地图，明确 focused pane、平台默认值、detached 键盘路径以及普通/秘密文本输入边界。
- 环境记录确认本轮没有依赖、锁文件、工具链、CI、配置 schema 或 SSH/worker 生命周期变化；tracker validator、46 个 Markdown 相对链接和 diff check 通过。
- 已完成第二轮 Slint/Rust `cargo check --locked --offline` 和完整 `cargo test --locked --offline`（库 141、应用 125、Doc tests 0），并完成参考耦合、秘密、无界状态、UI 阻塞和 staged commit 范围审阅。

## 验证

- 已完成：项目边界/环境审计、菜单树与输入安全边界复核、Slint 命令路由、跨平台 accelerator、直接 Rustfmt、Cargo check、定向/完整测试、双语/项目记录、文档门禁和独立提交审阅。
- 未完成：目标平台原生菜单显示、focused pane 定向、普通文本字段快捷键和 detached Terminal 快捷键人工验收。

## 风险与阻塞

- 原生菜单 accelerator 必须复用既有平台转换，避免 macOS Command/Control 交换和 Windows/Linux Ctrl+C 中断语义回归。
- terminal 选区只存在于 `TerminalPane` 局部状态；菜单命令必须通过有界 revision 信号只让 focused pane 执行，不能把选区坐标或文本提升到 Rust。
- 本轮不改变 SSH 安全边界、秘密输入、配置 schema 或依赖。
- `cargo fmt` 与 `cargo clippy` 因本机没有对应 Cargo 子命令无法执行；直接 Rustfmt 已通过，Clippy 仍由安装组件的 CI/目标环境补充。

## 下一步

- 在目标平台验收主窗口 Edit 菜单、多 pane focused 定向、普通文本字段和 detached Terminal 快捷键。

## 最后更新时间

- 2026-08-10 22:16 CST
