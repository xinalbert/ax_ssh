# 当前项目实施记录

## 当前目标

- 目标 ID：20260820-terminal-render-model-reuse
- 目标：根据 macOS `sample` 证据降低终端输出期间的主线程软件渲染压力，避免每次 snapshot 替换整棵 `render_lines` 动态模型树。
- 交付物：主终端 render-line/run model 原地复用、未变化行不发通知、focused model 回归和完整离线门禁；不改变终端内容、选区、worker 或 transport 语义。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/view.rs`、`src/app/view/settings.rs`、`src/app/view/terminal.rs`、`src/app/view/tests.rs`、双语架构说明、项目地图和 tracker。
- 不在本轮范围内：替换 Slint renderer backend、修改终端 parser/snapshot DTO、selection/copy/reporting 链路、配置 schema/迁移、依赖、锁文件、SSH trust、凭据、worker 生命周期或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PR1 | completed | 读取 sample 证据并定位主线程 software renderer 与整模型替换热点 | sample call graph、源码路径审计 | 141 个 1ms 样本中 116 个位于 DisplayLink，69 个进入 `SoftwareRenderer::render_buffer_impl`。 |
| PR2 | completed | 主终端 render-lines model 原地复用，行/run 内容未变化时跳过通知 | focused view tests、Slint compile | 外层 `VecModel` identity 保持；行数变化只 reset 同一 model。 |
| PR3 | completed | 完成格式、Clippy、全量测试、diff/tracker 门禁并记录残余风险 | repository verification commands | 未替换 software renderer backend；目标平台仍需在同一输出负载下重新采样确认收益。 |

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
- macOS sample 显示 CPU 主要集中于主线程 DisplayLink 的 Slint software renderer，而非 Tokio/SSH worker；主终端路径原先每个 snapshot 都替换整个 `render_lines` model。
- `apply_rendered_terminal` 现在复用已有 `VecModel<TerminalRenderLine>`，按行比较嵌套 runs，只对实际变化的行发通知；通用 nested model 更新也跳过相等 cursor/run。

## 验证

- 已完成：sample call graph 与源码热点审计、`cargo fmt --all -- --check`、直接 `rustfmt --edition 2024 --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 195、应用 177、Doc tests 0）、view focused tests、`git diff --check`。tracker validator 可运行但仍报告既有历史条目的时间/状态格式问题；本轮新增条目未触发错误。
- 未完成：在相同终端输出负载下重新生成目标平台 macOS `sample`，因此尚未量化 model 复用对 software renderer 的收益；GUI 选区、刷新和 Copy 仍需用户在目标平台验收。

## 风险与阻塞

- 输出可能改变选区坐标下的 cell 内容；当前契约是保留坐标并在 Copy 时读取最新内容，不承诺追踪原始文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- 语义搜索可能跨软换行或滚动历史，返回 UI 前必须裁剪到当前可见 viewport；单 cell 语义范围需要独立的有效位，否则现有坐标判定会把它当空选区。
- Slint 双击 callback 在普通 click 后触发；自动 Copy、目标激活和远端 reporting 的覆盖顺序必须保持可观察且不重复发送鼠标事件。
- 旧 Settings link/path 配置值仍存在于 schema，必须继续保持可读取和可保存；它不应重新进入 renderer 或可见颜色输入。
- 该优化只降低 Slint model churn，不减少 `TerminalModel::snapshot()` 的可见网格扫描，也不改变 software renderer 的选择；若 sample 仍显示持续整帧栅格化，需要下一轮单独评估 GPU backend 或更细粒度 dirty-row 设计。

## 下一步

- 用户在同一输出负载下重新采样，比较 `render_buffer_impl`、`render_component_items` 和主线程占用；随后确认终端刷新、选区和 Copy 未回归。

## 最后更新时间

- 2026-08-20 10:05 +0800
