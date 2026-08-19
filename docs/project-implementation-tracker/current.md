# 当前项目实施记录

## 当前目标

- 目标 ID：20260818-terminal-selection-lifecycle
- 目标：在终端视口身份变化时清除 Slint 局部选区，避免输出、resize 或滚动后复制陈旧 cell 坐标对应的文字。
- 交付物：Rust-owned 有界 selection revision、输出/真实 resize/有效滚动/回到底部失效链路、软/硬换行复制回归、双语契约和完整离线门禁。

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
| SL1 | completed | selection revision 贯通 AppState snapshot、Rust view DTO 与 Slint pane | focused output/resize/scroll tests、Cargo check | revision 不携带选区坐标或文字。 |
| SL2 | completed | 软换行/硬换行复制回归与双语契约、项目地图 | focused selection tests、双语结构检查 | 使用模型合法的最小 10 列。 |
| SL3 | completed | 完整离线门禁与最终边界审阅 | fmt/check/Clippy/test/translation/Markdown/tracker/diff | GUI 交互留目标平台验收。 |

## 已完成

- 已确认 selection draft 与坐标只属于 `TerminalPane`，复制文字仍由 `TerminalModel::selection_text()` 按当前 viewport 提取。
- `TerminalTabState` 新增有界 selection revision，经 active/split snapshots 和 `TerminalViewState` 传入 Slint；terminal identity、断开、失焦及 revision 变化均清除局部选区。
- 非空输出、真实的标准化 resize、有效本地 scroll 和输入前回到底部推进 revision；同尺寸 resize、零增量或已夹位 scroll 和空输出不推进。
- 复制回归覆盖 soft wrap 不插入换行，以及 hard break 和空白行保留换行。

## 验证

- 已完成：仓库/技能/架构与尺寸下限审阅；selection、output revision、真实/no-op resize、有效/no-op scroll 和 scroll-to-bottom 聚焦回归；`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 187、应用 176、Doc tests 0）、415 条翻译校验、8 个本轮 Markdown 相对链接、tracker validator 和 `git diff --check`。
- 未完成：目标平台输出/resize/scroll 后选区清除、稳定视口右键 Copy 和真实 GUI 手感验收；tracker validator 仍报告 37 条既有历史时间字段问题，本轮记录未新增错误。

## 风险与阻塞

- 高频输出会主动清除选区，这是避免坐标映射到新内容的保守行为；不会尝试在终端 reflow 后追踪文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- Slint 局部高亮与真实右键 Copy、输出、resize 和 scroll 的交互仍需目标平台确认。

## 下一步

- 由用户在目标平台验收输出、resize、scroll 后选区自动清除，以及稳定视口下右键 Copy 是否仍符合预期。

## 最后更新时间

- 2026-08-18 18:50 +0800
