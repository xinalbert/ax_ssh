# 当前项目实施记录

## 当前目标

- 目标 ID：20260820-renderer-preference
- 目标：让用户在 Settings > Appearance 持久化选择自动、GPU 或软件渲染，并只在重启时于首个窗口创建前应用该偏好。
- 交付物：schema v24 的规范化 renderer 偏好、启动选择、Settings 草稿/保存链路、双语说明和离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/{app,config}.rs`、`ui/{app,settings,workspace-shell,settings/appearance}.slint`、翻译目录、双语开发/架构/使用说明，以及项目地图和 tracker。
- 不在本轮范围内：运行时热切换 renderer、修改终端 parser/snapshot DTO、selection/copy/reporting 链路、终端 model、SSH trust、凭据、worker 生命周期或参考工程代码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| RP1 | completed | 定义 renderer 偏好、默认/覆盖优先级和启动所有权 | config/UI/`src/app.rs` 边界审阅 | `SLINT_BACKEND` 始终最高优先级；偏好不在运行中的 Slint window 上应用。 |
| RP2 | completed | 配置 schema、启动选择和 Settings 草稿/保存链路 | 配置回归与 `cargo check` | Automatic：macOS GPU，Windows/Linux software；GPU/Software 显式选择对应 backend。 |
| RP3 | completed | 双语文档、翻译目录与项目地图 | 翻译/Markdown/tracker 检查 | 已明确重启生效及环境变量覆盖规则。 |
| RP4 | completed | 完整离线门禁与差异审计 | repository verification commands | GUI renderer 的实际效果留给用户平台验收。 |

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
- `AppearanceSettings` 现以 schema v24 保存 `RendererPreference::{Automatic,Gpu,Software}`；缺失或无效值规范化为 Automatic。
- Settings > Appearance 的 Renderer 草稿沿既有预览/保存通路写入偏好，但不重建或切换活动 renderer；界面明确显示重启后生效，并支持中英文搜索与翻译。

## 验证

- 已完成：配置/Settings/启动选择、config 与应用定向回归、翻译/双语文档和项目地图。
- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 196、应用 178、Doc tests 0）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py` 和 `git diff --check`。
- 未完成：目标平台重启后的 GUI/renderer 人工验收；tracker validator 已执行，但被月度历史中既存的多项时间/状态字段格式问题阻断，本轮新增 `current.md` 和变更条目均已通过其结构检查。

## 风险与阻塞

- GPU/Skia 初始化失败时仍依赖 Slint softbuffer 回退；`SLINT_BACKEND=winit-software` 可建立同负载基线。即使 renderer 切换成功，`TerminalModel::snapshot()` 的可见网格扫描和文本布局仍可能成为下一热点。
- 输出可能改变选区坐标下的 cell 内容；当前契约是保留坐标并在 Copy 时读取最新内容，不承诺追踪原始文本 identity。
- revision 只能表达“清除”，不能包含坐标、文字、clipboard 内容或 worker handle。
- 语义搜索可能跨软换行或滚动历史，返回 UI 前必须裁剪到当前可见 viewport；单 cell 语义范围需要独立的有效位，否则现有坐标判定会把它当空选区。
- Slint 双击 callback 在普通 click 后触发；自动 Copy、目标激活和远端 reporting 的覆盖顺序必须保持可观察且不重复发送鼠标事件。
- 旧 Settings link/path 配置值仍存在于 schema，必须继续保持可读取和可保存；它不应重新进入 renderer 或可见颜色输入。
- 配置在窗口创建前才能读取并调用 `BackendSelector`，因此无法热切换；设置页必须明确重启生效，且预览不得调用 renderer 选择。

## 下一步

- 用户在目标平台打开 Settings > Appearance > Renderer，保存并重启后确认 Automatic/GPU/Software；需要诊断时用 `SLINT_BACKEND=winit-software` 覆盖当前进程并与默认路径对照采样。

## 最后更新时间

- 2026-08-20 13:12 +0800
