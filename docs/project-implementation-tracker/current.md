# 当前项目实施记录

## 当前目标

- 目标 ID：20260819-terminal-target-underline-only
- 目标：让 URL/路径只在按住平台主修饰键时显示临时下划线，默认保持终端原有前景色，并清理常驻语义 Link 颜色对渲染和 Settings 的覆盖。
- 交付物：renderer 状态语义色收窄、主修饰键目标提示链路核对、兼容旧配置字段、Settings/双语契约同步、渲染回归和完整离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/terminal_render.rs`、`src/app/view/terminal.rs`、`ui/{settings/terminal,terminal-pane}.slint`、`scripts/build_zh_catalog.py`、双语使用/架构文档、项目地图和 tracker。
- 不在本轮范围内：修改目标 parser、目标 DTO、selection/copy/reporting 链路、配置 schema/迁移、依赖、锁文件、SSH trust、凭据、worker 生命周期或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| TU1 | completed | URL/路径从常驻语义 renderer 和 Link 颜色覆盖中移除，3xx 归入 Information | terminal_render focused tests | 旧 config link/path 字段保留但不再参与渲染。 |
| TU2 | completed | Settings、主修饰键 gate 和双语使用/架构契约同步 | Slint compile、translation/catalog review | Cmd/Ctrl 目标提示仍由既有 TerminalTargetHighlight 下划线链路负责。 |
| TU3 | completed | tracker、环境记录与完整离线门禁 | fmt/check/Clippy/test/translation/tracker/diff | tracker validator 仅报告本轮未触及的历史时间字段；目标平台 Cmd/Ctrl 下划线 hover/click 仍需用户验收。 |

## 已完成

- 已确认 selection draft 与坐标只属于 `TerminalPane`，复制文字仍由 `TerminalModel::selection_text()` 按当前 viewport 提取。
- 输出 snapshot 只刷新网格，不推进 `selection_revision`；局部选区可在持续输出期间保持，Copy 读取最新 cell。
- `TerminalTabState` 的 revision 仍经 active/split snapshots 和 `TerminalViewState` 传入 Slint；terminal identity、断开、失焦、真实 resize 和有效 scroll 继续清除局部选区。
- 复制回归覆盖 soft wrap 不插入换行、hard break/空白行保留换行，以及输出刷新后读取最新 cell。
- 本轮已决定语义选词只由 `alacritty_terminal` 计算；Slint 不按 `render_lines` 字符串扫描。
- `TerminalModel` 已增加临时 `SelectionType::Semantic` 范围计算并裁剪到可见 viewport；Slint 已增加双击 callback 和显式局部选区有效位。
- `TerminalModel` 也通过临时 `SelectionType::Lines` 返回裁剪到可见 viewport 的逻辑行范围；`TerminalGrid` 仅维护同一 cell 的有界点击序列，第三击触发行选区，第四击及以后不重复该动作。
- URL/路径识别不参与常驻语义着色；renderer 只保留 HTTP 状态和成功/信息/警告/错误词的可选颜色，3xx 使用 Information。
- Settings 不再显示 Link and path 颜色输入；旧配置字段仍经过兼容 DTO 传递，但 renderer 忽略该值。

## 验证

- 已完成：Rust/Slint/架构边界审阅、renderer 与 Settings 目标收窄、双语文档和项目地图同步；`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 195、应用 176、Doc tests 0）、中文 catalog 生成/检查、相对 Markdown 链接和 `git diff --check`。
- 未完成：tracker validator 仍报告 40 条本轮未触及的历史时间字段/状态转换格式问题；新建 TU 条目不在错误列表中。

## 风险与阻塞

- 输出可能改变选区坐标下的 cell 内容；当前契约是保留坐标并在 Copy 时读取最新内容，不承诺追踪原始文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- 语义搜索可能跨软换行或滚动历史，返回 UI 前必须裁剪到当前可见 viewport；单 cell 语义范围需要独立的有效位，否则现有坐标判定会把它当空选区。
- Slint 双击 callback 在普通 click 后触发；自动 Copy、目标激活和远端 reporting 的覆盖顺序必须保持可观察且不重复发送鼠标事件。
- 旧 Settings link/path 配置值仍存在于 schema，必须继续保持可读取和可保存；它不应重新进入 renderer 或可见颜色输入。

## 下一步

- 由用户在目标平台验收 Cmd/Ctrl 按下和释放时 URL/路径下划线的出现、消失、pointer cursor 及与远端 reporting/本地选区的优先级。

## 最后更新时间

- 2026-08-19 18:12 +0800
