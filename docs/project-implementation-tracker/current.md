# 当前项目实施记录

## 当前目标

- 目标 ID：20261001-terminal-restore-reflow
- 目标：修复工作区恢复后首次 pane resize 丢失软换行开头内容、硬换行边界失真的问题，同时保留已完成的输入回底语义。
- 交付物：安全的恢复网格初始化、软/硬换行恢复回归测试、双语架构说明、实施记录，以及 Rust 静态门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/terminal/model.rs`、`src/app/state/tabs.rs`、终端/状态恢复测试、终端视口双语架构说明，以及 `docs/project-implementation-tracker/`。
- 不在本轮范围内：终端 parser、输出刷新策略、renderer cursor 绘制、transport worker、SSH trust/凭据策略、Slint UI contract、配置 schema、vendor 参考项目和 `third_package/axshell`。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| INPUT1 | completed | 核对输入、viewport owner 与标准终端回底语义 | 源码、xterm.js/Alacritty/WezTerm 公开实现 | 输出继续保持 detached；只对有效用户输入回底 |
| INPUT2 | completed | 恢复生产输入/粘贴回底路径并发布视口变化快照 | 聚焦状态测试与输入路径检查 | 不改变 worker、parser 或 cursor renderer |
| RESTORE1 | completed | 定位恢复文本在首次 resize 中丢失前导软换行内容的原因 | 状态级复现、`TerminalModel` 源码 | 默认大网格缩小会把恢复内容送入 scrollback |
| RESTORE2 | completed | 使用最小网格暂存恢复文本并覆盖混合换行边界 | 终端/状态回归测试 | live resize 语义保持不变 |
| RESTORE3 | completed | 完成完整 Rust 门禁和 tracker 收口 | fmt/check/Clippy/test、tracker、diff | 目标平台 GUI 仍需用户验收 |

## 已完成

- 已核对 `TerminalInputContext::dispatch`、`AppState::scroll_terminal_to_bottom` 和 `TerminalModel::scroll_to_bottom` 的 ownership 链路；当前生产 wrapper 被错误地限制在测试 cfg，输入路径没有回底。
- 已检索 xterm.js、Alacritty 和 WezTerm 的公开实现：输出允许 detached scrollback，默认有效键盘输入和粘贴回到底部；不让历史视口显示 live cursor。

## 验证

- 已完成：前置源码核对、标准终端公开实现检索、生产输入路径、聚焦状态回归、fmt、tracker validator 和 `git diff --check`。
- 已完成：输入回底完整门禁、状态级恢复复现与修复、混合软/硬换行聚焦测试、本轮完整 `cargo check`、严格 all-target Clippy、完整 Cargo 测试、tracker validator 和 `git diff --check`。
- 未完成：目标平台实际 GUI 交互验收。

## 风险与阻塞

- 输入回底会清除 detached 视口的本地 selection revision，并要求 UI 立即消费一次完整视口快照；输出到达路径仍不自动回底。
- 当前光标在 detached 视口隐藏是既有标准终端语义，本轮不改 renderer；目标平台仍需用户验收输入、回车、粘贴和持续刷新时的视觉稳定性。

## 下一步

- 用户重新打开含有长软换行和明确硬换行的 workspace，确认首次布局、窗口缩放和重连后内容边界正确；输入回底和持续输出视口语义继续按上一轮验收。

## 最后更新时间

- 2026-10-01 13:42 +0800
