# 项目研究记录

## 2026-07-29 Rust 与 Slint 工程规范依据

- 时间：2026-07-29 09:34 +0800
- 检索问题：当前 Rust 文件模块布局和 Slint 1.17.1 的文件拆分、生成代码、UI 线程、弱引用、异步调度、Model 与可访问性规范是什么？
- 检索原因：根 `AGENTS.md` 和项目 skill 将长期指导后续实现，必须区分上游当前建议、项目锁定 API 和本仓库架构约束。
- 来源列表：Rust Book 的模块文件章节 <https://doc.rust-lang.org/book/ch07-05-separating-modules-into-different-files.html>；Cargo targets <https://doc.rust-lang.org/cargo/reference/cargo-targets.html>；Slint latest 文件、properties、callbacks、globals、models 与 accessibility 指南；Slint 1.17.1 本地 crate 的 `ComponentHandle`、`invoke_from_event_loop`、`spawn_local` 和 `slint_build::compile` API 文档。
- 关键结论：Rust 当前惯用布局是 `foo.rs` 配合 `foo/bar.rs`，`mod.rs` 是仍支持的旧布局；Slint 组件和 event loop 必须保持 UI 线程亲和，callback 应捕获弱组件引用，后台结果通过 owned `Send + 'static` 数据回到 event loop；Tokio I/O 不应直接交给 Slint local executor；`.slint` 应保持声明式并使用 Model/虚拟化视图承载重复数据。
- 对实施计划的影响：根指令固定不可违反的架构、安全和验证门禁；项目 skill 负责工作流，Rust/Slint 细则拆入按需读取的 references；执行时以 Cargo.lock 中 Slint 1.17.1 为 API 基线，依赖升级时重新核对 latest 指南和迁移说明。
- 未解决问题：Slint `latest` 会随上游更新；未来升级 Slint、renderer/backend 或 Rust MSRV 时必须重新验证本文结论，不能仅沿用当前版本规则。

## 2026-07-29 会话分组与系统凭据方案

- 时间：2026-07-29 10:23 +0800
- 检索问题：参考项目如何组织保存会话并跳过密码提示，当前 Rust 跨平台系统凭据 API 能否在项目 MSRV 下替代明文密码持久化？
- 检索原因：用户要求复现参考项目的分组和免重复输入密码行为，但 AxSSH 安全契约禁止在配置或日志中保存明文凭据。
- 来源列表：本仓库只读参考 `third_package/axshell/src/session.rs`、`third_package/axshell/src/app/actions/saved_sessions.rs`、`third_package/axshell/src/app/views/sidebar.rs` 和 `third_package/axshell/src/app/session_ui.rs`；crates.io 的 `keyring 4.1.5` 元数据；本机下载的 `keyring 4.1.5` `README.md` 与 `src/v1.rs`。
- 关键结论：参考项目以规范化 `group_name` 聚合并折叠会话，但免输密码来自序列化的明文 `password`；`keyring 4.1.5` 默认 API 可用同一 `Entry` 接口访问 macOS Keychain、Windows Credential Manager 和 Unix Secret Service，MSRV 为 `1.88.0`。
- 对实施计划的影响：沿用会话级组名和运行期展开状态，不复制参考源码；新增独立系统凭据模块，以 profile UUID 作为稳定 account，只在 JSON 保存非敏感的凭据启用标记，所有系统凭据调用通过 Tokio blocking 边界执行。
- 未解决问题：当前环境只能真实验证 macOS Keychain；Linux Secret Service 和 Windows Credential Manager 仍需对应平台运行验收，系统服务不可用时必须回退临时密码弹窗。

## 2026-07-29 Slint Apple 修饰键映射

- 时间：2026-07-29 16:31 +0800
- 检索问题：为什么 macOS 物理 Ctrl 在终端和 tmux 中被应用识别为 Cmd？
- 检索原因：终端编码器已支持 C0 控制字节，但实际 macOS `Ctrl+B/C` 仍表现为 Command 组合，需要确认事件进入应用前的映射。
- 来源列表：Cargo.lock 锁定的 `i-slint-backend-winit 1.17.1` 本机源码 `event_loop.rs` 键盘事件转换；`i-slint-core 1.17.1` 本机源码 `input.rs` 修饰键状态；本仓库 `ui/app.slint`、`ui/terminal-pane.slint` 与 `src/app.rs` 输入路径。
- 关键结论：Slint winit 后端在 Apple 平台为兼容 Qt，明确把物理 Command 映射为 Slint Control、把物理 Control 映射为 Slint Meta；直接使用 `event.modifiers.control/meta` 会反转终端 Ctrl 与 macOS Cmd 快捷键。
- 对实施计划的影响：在 `src/app.rs` 唯一应用边界恢复物理修饰键语义；顶层 Slint shortcut capture 使用 `apple-platform` 属性执行同样的物理 Ctrl 优先判断；终端编码模块保持与 Slint 解耦。
- 未解决问题：该映射属于锁定版本行为；升级 Slint/winit 时必须重新核对，真实物理键和系统 IME 仍需目标平台手工验收。

## 2026-07-29 macOS 窗口拖动区域

- 时间：2026-07-29 16:45 +0800
- 检索问题：macOS 自定义统一标题栏中哪些区域应允许移动窗口，如何避免 Tab、侧栏和终端背景触发窗口拖动？
- 检索原因：当前窗口在任意背景拖动，破坏 Tab、侧栏和终端的常规交互；用户要求对齐常见代码编辑器。
- 来源列表：Apple Developer Documentation 的 `NSWindow.isMovableByWindowBackground`；Cargo.lock 锁定的 `objc2-app-kit 0.3.2` 本机 `NSWindow` API；锁定 Slint 1.17.1 `Flickable`/scroll-event 本机实现；用户提供的两张代码编辑器布局参考图。
- 关键结论：`isMovableByWindowBackground=true` 明确定义为任意窗口背景均可拖动，不适合终端；AppKit `performWindowDragWithEvent` 应在命中的 mouse-down 期间接收原始事件。常规代码编辑器把窗口拖动限制在标题栏未被 Tab/按钮占用的空白，Tab 条溢出通过横向 viewport 滚动处理。
- 对实施计划的影响：关闭全背景拖动，只在 macOS 红绿灯旁和 Tab 后方空白注册 Slint pointer-down callback；Tab、关闭按钮、侧栏和终端不注册该 callback；用现有有界 Tab 模型驱动 Flickable 横向滚动，不新增框架。
- 未解决问题：系统辅助功能权限关闭，无法自动完成真实拖动手势；最终需结合窗口截图和目标平台手工拖动验收。

## 2026-07-30 会话侧栏的 Group 折叠层级

- 时间：2026-07-30 11:27 +0800
- 检索问题：数据密集型会话侧栏应如何保留 Group，同时让服务器条目保持可扫描的单行，并避免 `v / >` 形式的折叠符？
- 检索原因：用户要求恢复 Group 折叠、以组名而非文件夹图标区分分组，并保持服务器单行与隐私遮蔽。
- 来源列表：Apple Human Interface Guidelines 的 Sidebars <https://developer.apple.com/design/human-interface-guidelines/sidebars>；Material Design 的 Lists <https://m3.material.io/components/lists/overview>；Fluent 2 Tree <https://fluent2.microsoft.design/components/web/react/core/tree/usage>。
- 关键结论：Sidebar 适合提供顶层集合导航，垂直列表适合连续可扫描的条目；Tree 的父项承担层级展开/收起，叶子项不承担该动作。因此 Group 是唯一的可折叠 parent，服务器是单行连接 leaf。
- 对实施计划的影响：以组名的前两个字符生成 Group 文字徽标，采用 `⌄ / ⌃` 上下尖角而非 `v / >`，并使 Group 具备可聚焦、Enter/Space 等价于点击的操作；运行期 `BTreeSet` 仅保存已展开的规范化 Group 名称。
- 未解决问题：不同目标平台字体对 `⌄ / ⌃` 的视觉字重、基线和辨识度仍需用户在实际窗口中确认。

## 2026-07-30 vt100 宽字符缩窄越界

- 时间：2026-07-30 12:10 +0800
- 检索问题：`vt100 0.16.2` 在缩窄终端列数、宽字符续位格被截断时是否已有可安全升级的修复？
- 检索原因：本地 Shell 缩放后触发 `Row::clear_wide` 的越界 panic；共享终端模型也服务远程 SSH，不能只绕过本地 PTY。
- 来源列表：锁定的 `vt100 0.16.2` `src/grid.rs`、`src/row.rs` 和 `CHANGELOG.md`；crates.io sparse index 的 `vt100` 发布记录；上游 <https://github.com/doy/vt100-rust> 主分支 `src/row.rs`、发布记录和 commits API。
- 关键结论：`0.16.2` 是当前最新发布版本；其 `Grid::set_size` 直接以 `Row::resize` 截断 cells，可能保留最后一格的宽字符首格。后续 `clear_wide` 访问不存在的下一格而 panic。上游主分支仍包含该实现，故没有可安全升级版本。`Row::truncate` 已包含预期修复：缩窄后清除新的行尾宽字符首格。
- 对实施计划的影响：保持 `vt100 0.16.2` API 与依赖版本，使用保留 MIT 许可的 `vendor/vt100` 受控 patch，只在列数缩窄时改为 `Row::truncate`；新增普通与备用屏幕回归，不以重建 parser、丢弃 scrollback 或 `catch_unwind` 规避问题。
- 未解决问题：需要在 macOS 实机对本地 Shell 以及真实 SSH 会话反复缩放验收；上游发布修复后应删除本地 patch。

## 2026-07-30 终端缩放的渲染状态顺序

- 时间：2026-07-30 12:48 +0800
- 检索问题：主流终端在窗口缩放和异步 PTY 输出交错时，如何确保终端不会在变宽后重新显示旧列数的渲染结果？
- 检索原因：用户提供的截图显示窗口内容区已变宽，但 `ls -l` 仍按约 40 列排版，说明 UI event loop 中较晚执行的旧输出 snapshot 覆盖了已经 resize 的模型。
- 来源列表：xterm.js `BufferService` <https://github.com/xtermjs/xterm.js/blob/master/src/common/services/BufferService.ts>；xterm.js `RenderService` <https://github.com/xtermjs/xterm.js/blob/master/src/browser/services/RenderService.ts>；WezTerm `termwindow/resize.rs` <https://github.com/wez/wezterm/blob/main/wezterm-gui/src/termwindow/resize.rs>。
- 关键结论：xterm.js 在 `BufferService.resize` 内先同步更新 buffer 的 `cols`/`rows`，随后才发送 resize 通知；其 renderer 在排队渲染前后均核对当前 viewport 尺寸。WezTerm 在同一窗口 resize 路径中计算 canonical `TerminalSize`、写入 window state、对所有 tab 执行 resize，随后使相关渲染失效。两者都不把过期的已序列化渲染快照作为之后事件的权威状态。
- 对实施计划的影响：AxSSH 的 Local PTY 与 SSH worker 事件只请求活动终端 UI 刷新；`slint::invoke_from_event_loop` 实际执行时才从 `AppState` 复制当前活动 snapshot。这样先排队的旧 Output 无法在 resize 后覆盖新网格，且保持 worker 不直接接触 Slint。
- 未解决问题：仍需在目标 macOS 上对高频 PTY 输出、连续窗口拖动和真实 SSH 输出组合进行人工验收；若快照复制成为性能热点，再按测量结果增加有界刷新合并，而不改变状态所有权。
