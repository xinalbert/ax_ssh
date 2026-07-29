# 当前项目实施记录

## 当前目标

- 目标 ID：20260729-platform-menubar
- 目标：复用 macOS 已有的系统 `ax_ssh` 应用菜单承载 Settings/About，在 Windows/Linux 提供窗口顶部同构菜单，并把左侧会话导航重做为互斥的展开卡片列表与收起图标栏。
- 交付物：macOS 应用菜单内可用的 About 与 `Settings...`（`Cmd+,`）；六个跨平台业务菜单；无 Settings/About Activity Bar 入口；展开态 Local Shell、分组与会话卡片；收起态终端/文件夹/会话紧凑图标；集中式 Theme token、成对说明和完整回归记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`Cargo.toml`、`Cargo.lock`、`src/app.rs`、`src/app/macos_window.rs`、`src/app/session_groups.rs`、`ui/app.slint`、`ui/theme.slint`、`ui/components/sidebar-controls.slint`、根 README、成对架构文档和实施/环境记录。
- 不在本轮范围内：分组重命名、完整实现 Edit/Pane/Window 命令组、修改设置 schema、SSH 认证、host-key、worker 或终端生命周期。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| P1 | completed | 锁定 Slint 1.17.1 MenuBar 平台行为和所有权边界 | 本地 locked crate source 与现有 callback 审查 | macOS/Windows 使用 native muda，Linux 由 Slint 渲染 |
| P2 | completed | 六个顶级菜单骨架和已有命令入口 | Slint/Cargo 联合编译与菜单 callback 审查 | 不手写 AppKit target/action，不新增依赖 |
| P3 | completed | macOS 系统菜单实际检查和重复入口结论 | 用户截图与实际菜单项审查 | 已确认应复用现有应用菜单，不在 File/Help 重复 Settings/About |
| P4 | completed | macOS 应用菜单桥接及六个可见业务菜单 | Slint/Cargo 联合编译与 AppKit 主线程/生命周期审查 | macOS 条件隐藏重复 Settings/About；Edit 保留标准禁用 Undo 槽位 |
| P5 | completed | 最新 macOS 顶部菜单运行时检查 | 实际启动、进程日志和系统截图 | 已确认空 Edit 会被省略并完成可见性修正；菜单点击受辅助功能权限限制 |
| P6 | completed | 无 Settings/About 入口的展开/收起会话导航组件 | Slint/Cargo 联合编译、模型映射测试和桌面截图 | 展开态与收起态互斥；不新增持久化字段 |
| P7 | completed | 双语说明、完整回归和最终 GUI 确认 | full test、文档/tracker validator、运行时截图 | Windows/Linux 外观保留对应平台验收 |

## 已完成

- 已确认用户所指菜单栏是操作系统/窗口顶部菜单，而不是左侧 Activity Bar。
- 已确认锁定的 Slint 1.17.1 `MenuBar` 会在 macOS 屏幕顶部显示原生菜单，在 Windows 使用原生窗口菜单，在不支持 native muda 的 Linux 后端由 Slint 在窗口顶部渲染。
- 已确认菜单项可直接复用 `new-session`、`open-local-shell`、`open-settings` callback，并由 Slint 本地状态切换 General/About 和侧栏可见性，不需要新增 Rust、AppKit 或持久化契约。
- 已确认顶级骨架需要保留 `File`、`Edit`、`View`、`Pane`、`Window`、`Help`，后续命令在对应菜单内增量填充。
- 已实现单一 MenuBar；File 接入 New Session、New Local Shell 和 Settings，View 接入 Toggle Session Sidebar，Help 接入 About AxSSH，Edit/Pane/Window 保留空菜单骨架。
- 已通过 `cargo check --locked --offline` 验证 Slint 菜单生成契约和现有 Rust callback 无变化，并同步双语产品、架构与项目地图说明。
- 已通过实际 macOS 菜单确认：Slint/winit 已生成标准 `ax_ssh` 应用菜单，继续在 File/Help 增加 Settings/About 会形成重复入口；空 Edit/Pane/Window 也不会显示。
- 已确定复用现有应用菜单：重新绑定标准 About，紧随其后插入 `Settings...`；六个业务菜单各使用现有可执行命令，避免空骨架被系统省略。
- 已完成 cfg-scoped AppKit action target：标准 About 与新插入的 Settings 只捕获 `Weak<AppWindow>`，由菜单项 represented object 保持 target 生命周期；`cargo check --offline` 通过。
- 已启动最新二进制并取得系统截图，确认 macOS 顶部显示 `ax_ssh / File / View / Pane / Window / Help`；由此确认空 Edit 会被系统省略，并加入标准禁用 Undo 槽位维持六分类可见。
- 用户进一步要求移除左侧 Settings/About，并以参考图的卡片式展开会话列表和图标式收起栏替代现有 Activity Bar；Local Shell 的文本符号将替换为自绘终端图标。
- 已删除左侧 Settings/About；展开态只显示 Local Shell 与分组/会话卡片，收起态只显示终端、文件夹/两字分组、已展开子会话和新建会话。
- 已新增 `ui/components/sidebar-controls.slint`，自绘稳定的终端/文件夹图标与窄栏卡片；对应颜色和全部几何尺寸进入 `ui/theme.slint`。
- 已让 Ungrouped 生成正式分组行，并以 Unicode 前两个字符生成分组/会话紧凑标签；删除独立 group-icons 模型，展开/收起复用同一个 `SessionRow` 模型。
- 已通过 `cargo check --locked --offline`、紧凑标签测试和会话分组/展开映射测试；设置 schema、SSH、凭据和 worker 均未变化。
- 已把密码弹窗、会话编辑器条件行和终端下划线偏移的遗留静态尺寸收回 `ui/theme.slint`，页面不再新增或保留这些配置字面量。
- 已完成最终 locked/offline 回归：库 44 passed、1 ignored，应用 16 passed；直接 rustfmt、tracker、Markdown 相对链接、Cargo metadata、主题/耦合/无界 channel 扫描和 `git diff --check` 通过。
- 已启动最终二进制，CoreGraphics 确认屏幕内 1180x740 AxSSH 窗口；系统截图确认 `ax_ssh / File / Edit / View / Pane / Window / Help` 六分类全部可见，随后停止测试进程。

## 验证

- 已完成：项目 skill/reference、环境记忆、项目地图、macOS 标准应用菜单桥接、会话导航组件/模型、双语说明、locked/offline check/test、直接 rustfmt、tracker/Markdown validator、边界扫描、最终二进制窗口和系统顶部菜单截图。
- 未完成：本机未安装 Cargo `fmt`/`clippy` 子命令；Screen Recording/Accessibility 权限阻止 Slint 客户区截图和自动菜单点击；Windows/Linux 平台外观未在本机验收。

## 风险与阻塞

- 无实现阻塞。剩余风险仅是未自动化的 Settings/About 原生菜单点击、展开/收起像素细节和 Windows/Linux 顶部菜单外观；SSH、安全、凭据和配置 schema 未变化。

## 下一步

- 用户在目标平台确认 macOS 应用菜单的 Settings/About 点击，以及展开/收起侧栏的卡片密度、文本截断和图标观感。

## 最后更新时间

- 2026-07-30 00:24 +0800
