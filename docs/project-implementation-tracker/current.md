# 当前项目实施记录

## 当前目标

- 目标 ID：20260810-terminal-pane-controls
- 目标：为主窗口 Terminal Tab 管理栏添加可发现的横向、纵向分屏图标按钮，同时保留主窗口和 detached Terminal 的现有方向快捷键。
- 交付物：顶部一组有无障碍名称与悬停说明的紧凑分屏按钮、复用既有活动 pane UUID pane-command callback 的实现、双语使用/架构说明、跟踪记录和独立提交。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/{components/workspace-titlebar,workspace-shell,theme}.slint`、`docs/{usage,usage.zh,architecture,architecture.zh}.md`、`docs/project-{implementation-tracker,env-audit}/` 与当前提交。
- 不在本轮范围内：PaneTree 行为、Rust callback/worker/SSH transport、SFTP pane、同一 PTY 或 SSH channel 的多重渲染、持久化、依赖或工具链升级，以及 `third_package/axshell`。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PCTRL1 | completed | 在主窗口 Tab 管理栏增加一组纵向、横向分屏图标，并将 intent 定向到活动 pane UUID | `cargo check --locked --offline` | 控件只调用既有 `pane-command`，不会持有 worker、连接或秘密。 |
| PCTRL2 | completed | 同步双语使用/架构说明、环境审计和实施跟踪 | tracker validator、Markdown 相对链接检查 | 说明现有 Alt 快捷键和点击后的独立会话语义。 |
| PCTRL3 | completed | 运行 Rust/Slint 自动化门禁，审阅提交范围并创建逻辑独立提交 | check/test/tracker/Markdown/diff/staged review | GUI 视觉、焦点和真实连接生命周期仍由目标平台用户验收。 |

## 已完成

- 已重新审阅根工程规则、AxSSH Rust/Slint UI 规范、项目地图、环境记录、现有 `TerminalPane` 输入处理和 `terminal_bridge` 的 pane-command 路径。
- 已确认现有分屏快捷键为 `Alt+Shift+H/J/K/L`，对应左/下/上/右；按钮可直接复用同一 callback，避免增加 Rust 或 SSH 侧状态。
- 已确认本机 Rust 1.96.1、Cargo 1.96.1、锁定 Slint 1.17.1 和离线 Cargo 合同未漂移；无需联网或多 agent。
- 已完成主窗口 Tab 管理栏、保存连接按钮左侧的固定尺寸纵向/横向分屏图标，均提供 Tooltip、accessible label 和 Enter/Space 激活；它们只作用于当前活动 pane。完整 Slint 图已由 `cargo check --locked --offline` 编译。
- 已更新双语 usage/architecture、项目地图和环境记忆；无新增文件、依赖、工具链、Rust callback 或 SSH 安全契约。

## 验证

- 已完成：范围/所有权/安全复核、Cargo 与工具链预检，重新编译完整 Slint 图的 `cargo check --locked --offline`，以及完整 `cargo test --locked --offline`（库 141、应用 116、Doc tests 0）、tracker validator、Markdown 相对链接检查、静态残留扫描与 `git diff --check`。
- 未完成：主/独立窗口的按钮可见性、hover、点击、焦点、快捷键和真实连接生命周期人工验收。

## 风险与阻塞

- 按钮只能创建独立会话，不能将同一个 live PTY 或 SSH channel 渲染到多个 pane；SSH 新 pane 仍可能需要正常的 host-key/认证流程。
- macOS 的 Option 修饰键仍可能受本机输入法布局影响；主窗口顶部按钮提供与键盘无关的入口，独立 Terminal 窗口仍使用快捷键。

## 下一步

- 本轮实现、自动化验证和独立提交已完成；等待用户在目标平台确认主窗口按钮位置和实际连接生命周期。

## 最后更新时间

- 2026-08-10 08:02 CST
