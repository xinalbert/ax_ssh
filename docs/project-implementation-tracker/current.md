# 当前项目实施记录

## 当前目标

- 目标 ID：20260819-terminal-multi-click-selection
- 目标：基于 `alacritty_terminal::SelectionType::{Semantic,Lines}` 支持终端双击选词和同序列三击选逻辑行，并保持本地选区、远端鼠标 reporting、目标激活和 Copy 链路边界清晰。
- 交付物：语义/逻辑行范围 DTO、主窗口/分屏/Detached callback 转发、单字符有效选区、双击/三击自动 Copy 兼容、模型与 UI 回归、双语契约和离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/terminal.rs`、`src/app/terminal_bridge.rs`、`ui/{app,workspace-shell,terminal-pane,components/terminal-grid}.slint`、双语使用/架构文档和 tracker。
- 不在本轮范围内：把选区坐标或文字提升到 AppState、修改 clipboard/transport 协议、SSH trust、凭据、worker 生命周期、配置 schema、依赖、锁文件或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SD1 | completed | `TerminalModel` 使用 alacritty 语义/逻辑行搜索返回可见、有界选区 DTO | semantic/line selection focused tests | 不写入 Term 持久 selection，不携带文字或 worker。 |
| SD2 | completed | Slint 双击/三击事件与主窗口/分屏/Detached callback 链路 | Slint compile、callback wiring review | 同一 cell 序列、超时和第四次点击行为明确；远端 reporting、Shift、Cmd/Ctrl 目标激活优先。 |
| SD3 | completed | 显式本地选区有效位、自动 Copy 兼容与回归 | focused/full Cargo tests | 单字符、ASCII 标点、CJK、括号、软换行、逻辑行和滚动边界。 |
| SD4 | completed | 双语契约、项目地图、完整离线门禁 | fmt/check/Clippy/test/translation/Markdown/tracker/diff | 真实双击和远端 TUI 仍需用户平台验收。 |

## 已完成

- 已确认 selection draft 与坐标只属于 `TerminalPane`，复制文字仍由 `TerminalModel::selection_text()` 按当前 viewport 提取。
- 输出 snapshot 只刷新网格，不推进 `selection_revision`；局部选区可在持续输出期间保持，Copy 读取最新 cell。
- `TerminalTabState` 的 revision 仍经 active/split snapshots 和 `TerminalViewState` 传入 Slint；terminal identity、断开、失焦、真实 resize 和有效 scroll 继续清除局部选区。
- 复制回归覆盖 soft wrap 不插入换行、hard break/空白行保留换行，以及输出刷新后读取最新 cell。
- 本轮已决定语义选词只由 `alacritty_terminal` 计算；Slint 不按 `render_lines` 字符串扫描。
- `TerminalModel` 已增加临时 `SelectionType::Semantic` 范围计算并裁剪到可见 viewport；Slint 已增加双击 callback 和显式局部选区有效位。
- `TerminalModel` 也通过临时 `SelectionType::Lines` 返回裁剪到可见 viewport 的逻辑行范围；`TerminalGrid` 仅维护同一 cell 的有界点击序列，第三击触发行选区，第四击及以后不重复该动作。

## 验证

- 已完成：上轮输出刷新/选区解耦的完整门禁；本轮仓库/技能/架构审阅、callback 冲突分析、语义/逻辑行范围 focused tests、`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 195、应用 176、Doc tests 0）和 `git diff --check`。
- 未完成：目标平台双击手感验收、真实远端 TUI reporting 验收，以及 tracker validator 的历史时间字段告警复核。

## 风险与阻塞

- 输出可能改变选区坐标下的 cell 内容；当前契约是保留坐标并在 Copy 时读取最新内容，不承诺追踪原始文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- 语义搜索可能跨软换行或滚动历史，返回 UI 前必须裁剪到当前可见 viewport；单 cell 语义范围需要独立的有效位，否则现有坐标判定会把它当空选区。
- Slint 双击 callback 在普通 click 后触发；自动 Copy、目标激活和远端 reporting 的覆盖顺序必须保持可观察且不重复发送鼠标事件。
- Slint 不暴露原生 click count；三击由 `TerminalGrid` 用同一 cell、500ms 有界计数器组合 `clicked`/`double-clicked`，因此目标平台仍需验收不同系统双击速度和连续点击手感。

## 下一步

- 完成 SD1-SD4 后，由用户在目标平台验收双击选词、持续输出期间 Copy、远端 reporting 和 resize/scroll 清除。

## 最后更新时间

- 2026-08-19 16:26 +0800
