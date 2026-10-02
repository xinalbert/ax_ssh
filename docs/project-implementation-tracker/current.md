# 当前项目实施记录

## 当前目标

- 目标 ID：20261002-tmux-wheel-cursor
- 目标：核实 AxSSH 在 tmux `mouse off` 的 copy mode 中的滚轮行为，补齐回归并复查先前的光标跳动边界。
- 交付物：可重复的终端输入链路回归、双语行为说明和验证记录；有证据表明行为失效时再修复。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`ui/components/terminal-grid.slint`、`ui/terminal-pane.slint`、`src/terminal/`、`src/app/{terminal_bridge,view/terminal}.rs` 及相关文档和测试。
- 不在本轮范围内：Cargo 依赖版本、SSH host-key trust、凭据、持久化 schema、worker 队列容量及参考项目源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TMW1 | completed | 对照 AxShell 与本机 tmux 复现 `mouse off` 的键盘、滚轮和备用屏幕事件 | tmux PTY 捕获、现有模型测试、Slint 无窗口事件测试 | 已确认事件没有在现有代码中丢失 |
| TMW2 | completed | 用真实 tmux 模式序列补输入和 UI 回归；核对光标跳动旧边界 | 模型、无窗口 Slint 测试 | 无已证实代码失效，不按 tmux 名称特判 |
| TMW3 | completed | 更新双语行为说明、环境与跟踪记录，完成仓库门禁 | fmt/check/Clippy/test、文档与 tracker 校验、diff | GUI 视觉交给用户验收 |

## 已完成

- 先前 CI 兼容目标已收口；本轮项目地图已覆盖终端 UI、应用 bridge 和模型入口，无需重建索引。
- 本机 tmux 3.6a 在 `mouse off` 时仍向外层终端发送备用屏幕和 application-cursor 模式；AxShell 在无 mouse tracking 时把备用屏幕滚轮转成方向键。
- AxSSH 模型已有备用屏幕滚轮转方向键逻辑；真实 tmux PTY 测试确认 `mouse off` 的 copy mode 接收 `ESC OA` 后，光标由第 22 行移至第 21 行。
- 新增模型回归覆盖 `Ctrl+B`、`Page Up`、tmux 实际备用屏幕/application-cursor 模式和滚轮字节；无窗口 Slint 回归确认滚轮到达远端输入 callback，且不进入本地 scrollback callback。应用 bridge 现有路径按 pane UUID 验证后把编码字节送入有界 worker 命令队列。
- 先前历史 scrollback 后输入回底的光标观感问题已有实现与状态回归；未使用同步输出协议的持续刷新 TUI 仍可能显示其自身分批写出的中间光标位置，当前没有特定程序或现场复现可证明已完全消除。

## 验证

- 已完成：Rust/Cargo 1.97.1、Rust 2024/MSRV 1.92.0、Slint 1.18.1、`Cargo.lock` 和 CI target 矩阵核对；AxShell 参考只读；tmux PTY 模式及 copy cursor 行变化捕获；聚焦回归、fmt、locked/offline check、严格 all-target Clippy、完整测试（库 300、应用 300、Doc tests 0）、Markdown 相对链接、tracker validator 和 `git diff --check` 通过。
- 未完成：真实 GUI 手工验收；Windows/Linux 原生目标由 CI 验证。

## 风险与阻塞

- 当前模型逻辑与 AxShell 同样支持备用屏幕滚轮；未找到失效层，不修改 SSH transport 或添加 tmux 特判。
- 真实鼠标、焦点和远端 tmux 的视觉行为需用户在目标平台确认；自动测试不代替截图验收。

## 下一步

- 真实 GUI 的 tmux copy mode 和持续刷新光标位置由用户在目标机器上验收；若仍有光标跳动，记录具体程序、操作和终端输出节奏后定位。

## 最后更新时间

- 2026-10-02 10:08 +0800
