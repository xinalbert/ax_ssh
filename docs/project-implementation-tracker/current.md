# 当前项目实施记录

## 当前目标

- 目标 ID：20260729-navigation-settings-polish
- 目标：没有已保存服务器时自动隐藏空会话侧栏，并把 Settings 建立在统一、可复用、完全由主题 token 配置的 Slint 基础控件上；根据实际截图移除 Settings 与工作区重复的 Tab/底栏 chrome，并修复 Activity Bar 设置图标缺失。
- 交付物：派生侧栏可见状态、无重复工作区 Tab 的 Settings Activity Bar 入口、可直达 About 的 Activity Bar 入口、稳定设置图标、标题栏内保存/关闭操作、独立 Settings 基础组件模块、集中式语义颜色/字号/间距/尺寸 token、六分类页面、编译期版本展示、成对用户说明和完整回归记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app.rs`、`ui/`、`README.md`、`README.zh.md`、`docs/project-env-audit/` 和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：新增设置字段、会话持久化 schema、SSH 认证、host-key、worker 或终端生命周期。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 环境预检、空侧栏根因和 Slint 所有权确认 | Cargo metadata 与 `ui/app.slint` 绑定审查 | 项目地图覆盖本轮，无需刷新 |
| P2 | completed | 空会话派生可见性、Settings Activity Bar 入口与分类式设置页面 | Slint/Cargo 联合编译和差异审查 | 保留现有保存 callback 和 draft 状态 |
| P3 | completed | 编译期版本映射、成对用户说明和初次联合编译 | Cargo check、格式和差异审查 | 不修改持久化或 SSH 契约 |
| P4 | completed | Settings 基础组件模块、集中式 Theme token 和基于该模块的页面重构 | Slint import graph 联合编译、硬编码样式扫描与重复布局审查 | 单文件组件集，主题配置留在 `ui/theme.slint` |
| P5 | completed | 完整仓库门禁和 GUI 走查 | test、文档/tracker validator、用户截图 | 自动截图受 macOS Screen Recording 权限限制 |
| P6 | completed | 左侧标题/说明与右侧紧凑控件组成的统一设置行 | Slint/Cargo 联合编译、控件尺寸与重复字段审查 | 保留简单分类导航，不引入 VS Code 大型设置树 |
| P7 | completed | 紧凑设置页最终回归和记录收口 | full test、文档/tracker validator、边界和差异检查 | 最新二进制窗口已启动，视觉细节保留手工确认 |
| P8 | completed | 移除 Settings 重复 Tab/底栏，修复设置图标并加入 About 直达入口 | Slint/Cargo 联合编译、截图标注区域与两个 Activity Bar 入口逐项审查 | Settings 状态保留为单例，UI Tab 模型不再展示它 |
| P9 | completed | 最新布局完整回归和记录收口 | full test、文档/tracker validator、窗口启动和差异检查 | 页面像素仍需目标平台手工确认 |

## 已完成

- 已确认 `sidebar-collapsed` 默认是 `false`，会话面板没有检查 `sessions.length`，所以空模型仍占用配置的 220px 宽度。
- 已确认可见性属于 `ui/app.slint` 的派生展示状态，不需要修改 `src/app.rs`、持久化配置或 SSH 边界。
- 已完成施工前环境复核；环境事实无变化，并把旧环境摘要对齐到当前记录契约。
- 已确认现有 Settings 只有 Terminal、Workspace 和 Keybindings 三个粗粒度页面，Activity Bar 没有可点击 Settings 入口，且没有 About 页面。
- 已确认现有设置字段可在 Slint 内重新分组；Rust 侧只需注入只读编译期版本，不改变保存 callback 参数或配置 schema。
- 已实现空会话侧栏派生可见性；Activity Bar 的 Local Shell、Settings 和新增会话入口始终保留。
- 已把 Settings 重组为六个直接可达页面和固定底部操作栏；分类切换保留 draft，About 显示编译期版本。
- 已刷新项目地图和中英文 README/架构说明，记录新导航、空状态和只读版本映射。
- 已完成初次编译期版本映射、成对说明和 Slint/Rust 联合编译；用户走查后确认仍需统一基础控件以消除页面粗糙感。
- 已创建 Settings 基础组件模块并完成第一次引用编译；用户进一步要求全部界面样式配置集中到配置文件，以便后续配置主题。
- 已把语义颜色、字号、通用间距/圆角和标准工作区、Settings、编辑器、弹窗尺寸集中到 `ui/theme.slint`；页面只保留运行时尺寸、用户设置和必要零值。
- 已让 `ui/settings.slint` 通过 `SettingsPage`、`SettingsRow`、`SettingsToggleRow`、`ShortcutRecorder`、`SettingsFooter` 等基础组件组合六个页面，并加入导航键盘焦点态。
- 已刷新双语架构和项目地图，明确 Theme visual config 与持久化 `AppSettings` 的边界。
- 已完成上一轮完整回归：`cargo check --locked --offline` 和 `cargo test --locked --offline` 通过，库测试 44 passed、1 ignored，应用测试 15 passed；tracker、Markdown 相对链接、Theme 字面量和差异检查通过。
- 已根据用户提供的 Settings 实际截图确认：52px 设置行会拉伸标准控件，Appearance 字体输入重复，右侧字段缺少统一的紧凑尺寸和左侧说明层级。
- 已新增 `SettingsField`，所有右侧控件统一使用 280px 字段槽和 32px 高度；设置行改为标题加简短元数据，Appearance 字体只保留可编辑输入框，并移除无消费者的 Rust 字体选项模型。
- 已按 Slint Fluent SpinBox 的 128px 最小宽度校正双数字字段，并为输入、下拉、数字和滑块补齐可访问标签；联合编译通过。
- 已完成最终回归和最新二进制启动检查；AxSSH 正常创建 1180x740 窗口并初始化，测试、文档/tracker、主题字面量、相对链接、Cargo workspace 和差异检查通过。
- 用户最新截图确认三个 chrome 问题：Settings 仍显示为顶部工作区 Tab 且保留新建按钮，Activity Bar 设置字符缺少字形，页面底部又重复显示状态/Close/Save 操作栏。
- 用户进一步要求 Settings 从左侧 Activity Bar 进入，并在同一栏增加 About 直达入口，同时补充使用说明。
- 已从可见工作区 Tab model 过滤 Settings，并在 Settings 激活时用纯拖动标题区替代 Tab/+；对应应用边界回归测试通过。
- 已用可复用 `SettingsGlyph` 替代缺字齿轮，Activity Bar 的 Settings/About 分别直达 General/About，且只高亮当前入口。
- 已删除 `SettingsFooter`，把非 Ready 状态、Close 和 Save 收入 `SettingsHeader`；中英文 README、架构和项目地图已同步。
- 已为 About 页面补充 AxSSH 的用途说明；对应高度和说明文本尺寸继续由 `ui/theme.slint` 统一配置。
- 已完成最新布局回归；CoreGraphics 确认最新二进制创建 1180x740 的屏幕内主窗口，随后已停止测试进程。

## 验证

- 已完成：项目 skill/reference、环境记忆、Cargo metadata、基础组件/Theme Slint-Rust 联合编译、完整 Cargo test（库 44 passed、1 ignored；应用 16 passed）、静态样式字面量扫描、直接 Rust 格式检查、项目地图、Markdown/tracker validator、相对链接、边界和差异检查，以及最新二进制 1180x740 窗口启动。
- 未完成：macOS Screen Recording/Accessibility 权限阻止自动点击和截图；Settings/About 入口、说明换行与三个原红框区域保留为目标平台手工视觉确认。`cargo-fmt` 和 `cargo-clippy` 组件未安装。

## 风险与阻塞

- 无实现阻塞。Settings 必须继续显式保存并可关闭返回，但不再进入可见工作区 Tab 列表；主题 token 仍只拥有视觉语义，用户设置继续由 `AppSettings` 持久化。剩余风险仅为未自动化的目标平台像素和点击走查。

## 下一步

- 用户在目标平台确认左侧 Settings/About 入口、About 说明换行，以及顶部 Tab、设置图标和底部操作三个原标注区域。

## 最后更新时间

- 2026-07-29 23:04 +0800
