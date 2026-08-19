# 当前项目实施记录

## 当前目标

- 目标 ID：20260819-terminal-selection-refresh-semantics
- 目标：让终端输出刷新与局部选区解耦，刷新网格时保留选区，并在 Copy 时读取选区坐标对应的最新 cell。
- 交付物：输出不推进 selection revision、最新 cell 复制回归、resize/scroll 失效边界、双语契约和完整离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/terminal.rs`、`src/app/{state,state/tabs,state/terminal,state/tests,terminal_bridge,view/terminal}.rs`、`ui/{app,terminal-pane}.slint`、双语使用/架构文档和 tracker。
- 不在本轮范围内：把选区坐标或文字提升到 AppState、修改 clipboard/transport 协议、SSH trust、凭据、worker 生命周期、配置 schema、依赖、锁文件或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SR1 | completed | 输出刷新不再推进 selection revision，Copy 读取最新 cell | output/selection focused tests、Cargo check | 选区坐标仍只属于 Slint。 |
| SR2 | completed | 双语契约、项目地图和跟踪记录 | translation/Markdown/tracker/diff | resize/scroll/identity/focus 失效边界保持不变。 |
| SR3 | completed | 完整离线门禁与最终边界审阅 | fmt/check/Clippy/test | GUI 交互留目标平台验收。 |

## 已完成

- 已确认 selection draft 与坐标只属于 `TerminalPane`，复制文字仍由 `TerminalModel::selection_text()` 按当前 viewport 提取。
- 输出 snapshot 只刷新网格，不推进 `selection_revision`；局部选区可在持续输出期间保持，Copy 读取最新 cell。
- `TerminalTabState` 的 revision 仍经 active/split snapshots 和 `TerminalViewState` 传入 Slint；terminal identity、断开、失焦、真实 resize 和有效 scroll 继续清除局部选区。
- 复制回归覆盖 soft wrap 不插入换行、hard break/空白行保留换行，以及输出刷新后读取最新 cell。

## 验证

- 已完成：仓库/技能/架构审阅；输出/selection focused tests、真实/no-op resize、有效/no-op scroll 和 scroll-to-bottom 回归；`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 188、应用 176、Doc tests 0）、415 条翻译校验、Markdown 相对链接、tracker validator 和 `git diff --check`。
- 未完成：目标平台持续输出期间拖选、刷新后 Copy、resize/scroll 失效和真实 GUI 手感验收；tracker validator 的 37 条既有历史时间字段问题仍未修复。

## 风险与阻塞

- 输出可能改变选区坐标下的 cell 内容；当前契约是保留坐标并在 Copy 时读取最新内容，不承诺追踪原始文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- Slint 局部高亮与真实右键 Copy、输出、resize 和 scroll 的交互仍需目标平台确认。

## 下一步

- 由用户在目标平台验收持续输出期间拖选、刷新后 Copy，以及 resize/scroll 后的选区清除。

## 最后更新时间

- 2026-08-19 09:01 +0800
