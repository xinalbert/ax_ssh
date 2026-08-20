# 当前项目实施记录

## 当前目标

- 目标 ID：20260820-terminal-renderer-selection
- 目标：根据 macOS `sample` 证据把持续整帧软件栅格化从默认路径移到平台合适的 GPU-backed renderer，同时保留明确的软件回退和既有终端语义。
- 交付物：编译 Slint Skia 与 software renderer；macOS 默认选择 Metal-backed `winit-skia`，其它桌面平台保持 `winit-software`；尊重 `SLINT_BACKEND` 覆盖，更新双语契约并完成离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`src/app.rs`、双语开发/架构说明、项目地图和 tracker。
- 不在本轮范围内：修改终端 parser/snapshot DTO、selection/copy/reporting 链路、终端 model、配置 schema/迁移、SSH trust、凭据、worker 生命周期或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PR1 | completed | 审计新旧 sample 结论与 Slint 1.17.1 renderer/fallback 源码 | sample call graph、依赖源码审计 | 既有 sample 的主线程热点集中在 DisplayLink/software renderer；新路径当前在工作区不可读，不能伪造对其内容的分析。 |
| PR2 | completed | 启用 Skia，按平台在首个 `AppWindow` 前选择 renderer，并保留环境覆盖 | Cargo feature resolution、启动选择代码审计 | macOS 选择 `winit-skia`（Metal + softbuffer fallback），Windows/Linux 选择 `winit-software`。 |
| PR3 | completed | 完成双语文档、Cargo 门禁和差异审计 | repository verification commands | 目标 macOS 负载下仍需重新采样，确认 `SoftwareRenderer::render_buffer_impl` 不再是默认主路径。 |

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
- 启动时先处理 `SLINT_BACKEND`；无显式覆盖时 macOS 选择 Skia/Metal，Windows/Linux 选择 software，避免把诊断回退和平台默认策略混在一起。

## 验证

- 已完成：既有 sample call graph 与 Slint 1.17.1 fallback 源码审计、启动 renderer 选择实现、双语契约更新。
- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 195、应用 177、Doc tests 0）、`python3 scripts/check_translations.py` 和 `git diff --check`；目标 macOS 仍需在相同终端输出负载下重新采样，量化 Skia 对 software renderer 热点的替换收益。

## 风险与阻塞

- 新 renderer 选择不能替代用户平台验收：Metal/Skia 初始化失败时应观察 softbuffer 回退；`SLINT_BACKEND=winit-software` 可用于建立同负载基线。即使 renderer 切换成功，`TerminalModel::snapshot()` 的可见网格扫描和文本布局仍可能成为下一热点。
- 输出可能改变选区坐标下的 cell 内容；当前契约是保留坐标并在 Copy 时读取最新内容，不承诺追踪原始文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- 语义搜索可能跨软换行或滚动历史，返回 UI 前必须裁剪到当前可见 viewport；单 cell 语义范围需要独立的有效位，否则现有坐标判定会把它当空选区。
- Slint 双击 callback 在普通 click 后触发；自动 Copy、目标激活和远端 reporting 的覆盖顺序必须保持可观察且不重复发送鼠标事件。
- 旧 Settings link/path 配置值仍存在于 schema，必须继续保持可读取和可保存；它不应重新进入 renderer 或可见颜色输入。
- 该优化只降低 Slint model churn，不减少 `TerminalModel::snapshot()` 的可见网格扫描，也不改变 software renderer 的选择；若 sample 仍显示持续整帧栅格化，需要下一轮单独评估 GPU backend 或更细粒度 dirty-row 设计。

## 下一步

- 完成离线门禁后，用户在同一输出负载下分别采集默认 macOS 与 `SLINT_BACKEND=winit-software` sample，比较 `render_buffer_impl`、Skia/Metal 栈、`render_component_items` 和主线程占用；随后确认终端刷新、选区和 Copy 未回归。

## 最后更新时间

- 2026-08-20 10:30 +0800
