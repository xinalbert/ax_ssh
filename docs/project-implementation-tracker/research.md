# 项目研究记录

## 2026-08-14 终端纵向扩容与 scrollback 语义

- 检索问题：普通 shell 终端在纵向放大、光标位于原底行且没有可恢复历史时，新增的空行应位于顶部还是底部？
- 检索原因：用户截图显示 AxSSH 将普通输出下推至窗口底部并在顶部留下大面积空白，需要判定这是否符合主流终端行为后再修改缓冲区层。
- 来源列表：Alacritty `Grid::grow_lines` <https://github.com/alacritty/alacritty/blob/master/alacritty_terminal/src/grid/resize.rs>；xterm.js `Buffer.resize` <https://github.com/xtermjs/xterm.js/blob/master/src/common/buffer/Buffer.ts>；WezTerm `Screen::resize` <https://github.com/wezterm/wezterm/blob/main/term/src/screen.rs>；kitty `screen_resize` <https://github.com/kovidgoyal/kitty/blob/master/kitty/screen.c>。
- 关键结论：高度增长可从 scrollback 恢复真实历史行，届时提示符可相对下移；历史不足时，各实现都在底部补空行并保留现有可见输出的顶部位置。Alacritty 的注释明确区分这两种情况。将已有内容下移来制造顶部空行不是普通 shell 的 resize 语义。
- 对实施计划的影响：删除 AxSSH 在 `Term::resize` 后的 `scroll_down` 与强制 cursor-to-bottom 补偿，直接交给锁定的 `alacritty_terminal` 处理主/备用屏与 reflow；回归按“无历史顶部对齐”和“有历史恢复历史”分别断言。
- 未解决问题：目标 macOS 的连续窗口拖动、极小 pane 裁剪和 IME 坐标仍需用户视觉验收；它们属于 UI 几何，不能用缓冲区补偿掩盖。

## 2026-08-04 VS Code Terminal 最小对比度语义

- 时间：2026-08-04 11:52 +0800
- 检索问题：VS Code Terminal 所谓的亮度/对比度调节是否会重写所有 ANSI 颜色，AxSSH 应采用何种兼容语义？
- 检索原因：当前 AxSSH 的 Brightness 以 RGB 混合无条件改变前景和背景，用户观察到颜色整体漂移且效果不佳，需要以官方行为作为替换依据。
- 来源列表：VS Code 官方 Terminal Appearance 文档 <https://code.visualstudio.com/docs/terminal/appearance#_minimum-contrast-ratio>；VS Code `terminalConfiguration.ts` 的 `minimumContrastRatio` 设置定义；xterm.js `Color.ts` 的 `ensureContrastRatio`、WCAG 相对亮度和前景修正实现。
- 关键结论：VS Code 没有同语义的亮度滑块，而是 `terminal.integrated.minimumContrastRatio`，默认 4.5、1 表示不修正、21 表示最高目标。它只在每个单元格的前景低于实际背景对比度时调整前景；dim 单元使用半目标，并保留背景与已经达标的颜色。修正朝黑/白方向搜索，可能轻微降低饱和度但不会把全部 ANSI 色无条件重映射。
- 对实施计划的影响：AxSSH 将 Brightness 替换为 1.0–21.0、0.5 步长的最小对比度设置，持久化使用 tenths 固定精度；渲染实现标准 WCAG 相对亮度、实际 cell background 和仅前景修正，schema v17 放弃旧亮度数值而迁移到 4.5 默认。
- 未解决问题：xterm.js 的 powerline glyph 特殊豁免尚未映射到 AxSSH 的 run DTO；目标平台仍需用户确认字体、dim、反色和彩色背景的实际观感。

## 2026-07-30 主题明暗与控件 palette 统一

- 时间：2026-07-30 21:12 +0800
- 检索问题：应用明暗模式、主题配色和标准控件 palette 应如何分层，并怎样避免自定义主题导致控件不可读或下拉颜色错配？
- 检索原因：用户要求参考 AxShell 和网络规范检查主题结构，并继续实施 AxSSH、Slint 标准控件和终端的统一解析。
- 来源列表：AxShell 固定提交的 `src/app/theme.rs` 与 `docs/features/appearance-settings.zh.md`；W3C WCAG 2.2 Contrast Minimum、Non-text Contrast 和 Use of Color；Microsoft Fluent 2 Color；Cargo.lock 锁定的 Slint 1.17.1 `widgets/{cupertino,fluent,material}/style-base.slint` 与 `styling.slint`。
- 关键结论：显示策略、Light/Dark 模式和 palette 是独立维度；标准控件必须与应用最终明暗状态一致。Slint `Palette.color-scheme` 可写，`ColorScheme.unknown` 会让内置 palette 使用后端系统状态；因此 System 模式应保留 `unknown`，手动模式再显式写 Light/Dark。正文和关键控件状态仍需分别满足 WCAG 4.5:1 与 3:1。
- 对实施计划的影响：本轮先建立单一 `resolved-dark` 并同步标准控件与终端；后续 schema 重构再把模式和 palette 拆开，并为自定义颜色增加成对 token 与对比度校验。
- 未解决问题：原生 `ContextMenuArea` 的具体色值仍由目标平台决定；自定义 Light/Dark 双 palette 和完整对比度门禁需要后续独立迁移。

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

## 2026-07-30 Slint 扁平右键与下拉动作菜单

- 时间：2026-07-30 20:22 +0800
- 检索问题：Slint 1.17.1 能否用一个扁平动作组件同时支持右键菜单和按钮触发的下拉菜单，并由 model 动态生成菜单项？
- 检索原因：用户要求把当前重复的 Group/服务器右键菜单抽成扁平组件，并同时覆盖下拉菜单触发方式。
- 来源列表：Slint 1.17.1 `ContextMenuArea` 官方文档 <https://docs.slint.dev/1.17.1/docs/slint/reference/window/contextmenuarea/>；Slint latest 组件组合文档 <https://docs.slint.dev/latest/docs/slint/guide/language/coding/file/>；本机锁定 `i-slint-compiler 1.17.1` 的 `builtins.slint`、`passes/lower_menus.rs` 和 `tests/syntax/elements/menu-shortcuts.slint`。
- 关键结论：`ContextMenuArea` 自动处理右键/Menu 键/长按，并允许通过 `show(Point)` 主动显示同一原生菜单；每个 area 必须有且仅有一个非条件/非重复的根 `Menu`，但根菜单内部支持 `if` 和 `for` 动态生成 `MenuItem`。Slint 组件适合通过组合复用；当前项目实测继承 `ContextMenuArea` 会触发 1.17.1 inlining panic，因此必须组合而非继承。
- 对实施计划的影响：新增普通 Rectangle 组件持有 `[ActionMenuItem]` 和内部 `ContextMenuArea`，暴露 action ID callback 与 `show-at(Point)`；会话导航只负责把 Group/服务器/空白区域语义映射为扁平 action 列表和既有 callback。
- 未解决问题：原生菜单的屏幕边缘定位和 macOS/Windows/Linux 平台外观需目标平台验收；可搜索或富内容下拉不在本轮范围内。

## 2026-07-31 Telnet 与 Serial transport 依赖

- 时间：2026-07-31 21:12 +0800
- 检索问题：哪些 Rust crate 能在当前 Tokio/三平台边界内提供异步串口和非裸 TCP 的 Telnet 协议解析？
- 检索原因：新增依赖会改变 Cargo.lock；仓库要求在修改 Tokio 或 transport 依赖前核对上游元数据、MSRV 和协议能力。
- 来源列表：crates.io `tokio-serial 5.5.0` 元数据与下载源码 <https://crates.io/crates/tokio-serial/5.5.0>；crates.io `nectar 0.4.0` 元数据、README 与下载源码 <https://crates.io/crates/nectar/0.4.0>；两者的 Cargo manifest 和公开 API。
- 关键结论：`tokio-serial 5.5.0` 默认 feature 不启用 Linux `libudev`，公开 `available_ports`、USB 元数据、异步 `SerialStream` 与标准串口参数，package metadata 记录 MSRV 1.71。`nectar 0.4.0` 是 Rust 2021 的 Tokio 0.7 codec，解析 DO/DONT/WILL/WONT、NAWS 和未知 subnegotiation，并对出站 IAC 转义；它只声明 partial RFC 854 且未声明 MSRV，因此应用只启用最小协商并以项目 MSRV/loopback tests 约束行为。
- 对实施计划的影响：锁定 `tokio-serial 5.5.0` 和 `nectar 0.4.0`，直接声明 `tokio-util`/`futures-util` codec 边界；Serial 枚举不启用 `libudev`，Telnet worker 使用 character mode 保留入站字节并只接受应用生成的有效 UTF-8 输入。
- 未解决问题：`nectar` 没有稳定 `rust-version` 声明；真实 legacy Telnet server 的非标准协商和各平台串口权限仍需目标环境验证。

## 2026-07-31 Telnet 流解析器复审

- 时间：2026-07-31 22:16 +0800
- 检索问题：`nectar 0.4.0` 暴露真实流边界后，现有 Rust Telnet parser 中哪一个能满足 TCP 分片、转义 IAC、选项协商、NAWS、MSRV、许可证和有界内存要求？
- 检索原因：`nectar` 字符模式除 LF 不消费外，还会在转义 IAC、两字节命令和不完整 subnegotiation 上丢字节、停滞或越界；继续用窄补丁不能形成可靠的 RFC 854 transport。
- 来源列表：crates.io 与下载源码：`libmudtelnet-rs 2.0.10` <https://crates.io/crates/libmudtelnet-rs/2.0.10>、`libmudtelnet 2.0.2`、`libtelnet-rs 2.0.0`、`telnet-codec 0.1.0`、`telnet 0.2.5`、`mini-telnet 0.1.8`、`safe-telnet-parser 0.1.0`、`rfc2217-rs 0.1.0`；对应 manifest、parser、事件、协商表与测试源码。
- 关键结论：`libmudtelnet-rs 2.0.10` 声明 Rust 1.66、MIT，继承 Blightmud/libtelnet 系列生产历史，提供 typed events、选项状态、自动接受/拒绝、IAC escaping 与 subnegotiation 编码；但当前 `receive` 不能直接接收跨调用的不完整 IAC/协商帧，且 `IAC IAC` 后跟数据时与文档不一致。其它候选分别存在不完整帧、无界缓冲、subnegotiation panic/编码错误、同步 I/O 或许可证问题，不能直接替代。
- 对实施计划的影响：用 `libmudtelnet-rs` 负责完整帧的 RFC 854 事件、协商状态和编码；AxSSH 在 socket 与 parser 之间维护最大 64 KiB 的有界分帧适配，只把完整 IAC 命令/协商/subnegotiation 交给 parser，并将转义 `IAC IAC` 明确还原为终端数据。删除旧 codec 专用直接依赖，新增逐字节分片、CRLF、IAC、NOP、未知选项和 NAWS 转义回归。
- 未解决问题：真实 legacy Telnet server 的非标准命令仍需目标网络验收；若上游修复跨调用分片与 `IAC IAC`，可删除本地分帧适配并保留同一回归集。

## 2026-08-01 SFTP 参考行为、协议库与共享 SSH 生命周期

- 时间：2026-08-01 00:25 +0800
- 检索问题：AxSSH 第一阶段 SFTP 应采用什么用户范围和 worker ownership；`russh 0.62.2` 与当前 `russh-sftp` 能否在同一已认证 transport 上安全建立远端文件浏览？
- 检索原因：用户明确要求参考 AxShell 并联网检索；依赖选择、SSH handle 所有权、目录内存上限和关闭语义会直接改变跨模块实现。
- 来源列表：AxShell 公开 SFTP 文档与源码树 <https://github.com/xinalbert/axshell/blob/main/docs/features/sftp.zh.md>、<https://github.com/xinalbert/axshell/tree/main/src/sftp>；`russh-sftp` README、manifest 和 client API <https://github.com/AspectUnk/russh-sftp>、<https://github.com/AspectUnk/russh-sftp/blob/master/src/client/session.rs>；SFTP v3 draft <https://datatracker.ietf.org/doc/html/draft-ietf-secsh-filexfer-02>；本机锁定 `russh 0.62.2` channel/client 源码和缓存的 `russh-sftp 2.3.0` 源码。
- 关键结论：AxShell 将远端文件浏览、传输和受管编辑拆成独立状态，并对目录采用 250 条/page、2,000 条或 2 MiB 名称/路径预算；其传输完整范围明显大于 AxSSH 的第一阶段。`russh` 认证 handle 可在同一 transport 上继续 `channel_open_session` 并请求 `sftp` subsystem。`russh-sftp 2.3.0` 实现常见 SFTP v3 和高层文件 API，但未声明 MSRV，client packet writer 使用 unbounded channel，高层 `read_dir` 会读完整目录后才返回。
- 对实施计划的影响：第一阶段只交付当前 SSH Tab 的远端浏览，并由现有 SSH worker 统一拥有 shell 与 SFTP channel；不建立第二套认证，不把 handle 送进 `AppState`。AxSSH 自设 bounded command/event channel、请求超时、250 条/page、2,000 条与 2 MiB 总预算，使用 raw directory cursor 分页而不调用会一次读完的高层 `read_dir`。上传/下载/删除/编辑留到有独立确认、进度与取消模型的后续阶段。
- 未解决问题：`russh-sftp` 未声明 MSRV，需用项目 MSRV/CI 约束；其内部 unbounded packet sender 无法由外层替换，本轮通过单 session、串行请求和外层有界入口限制风险。真实 OpenSSH/非 OpenSSH SFTP 服务兼容性与 GUI 交互仍需目标环境验收。
## 2026-08-01 SSH 交互输入延迟与预测回显

- 时间：2026-08-01 12:10 +0800
- 检索问题：russh/Tokio 交互 SSH 是否默认启用 `TCP_NODELAY`，远端 PTY 回显的 RTT 下限是什么，高 RTT 下有哪些经过验证的改善方案？
- 检索原因：用户要求检索并评估 SSH 远程输入延迟，随后授权按建议在 AxSSH 中实施低风险方案。
- 来源列表：russh 0.62.2 client config/source <https://docs.rs/russh/0.62.2/src/russh/client/mod.rs.html#981-988> 与 <https://docs.rs/russh/0.62.2/src/russh/client/mod.rs.html#2093-2116>；Tokio `TcpStream::set_nodelay` <https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html#method.set_nodelay>；PuTTY interactive Nagle 设置 <https://the.earth.li/~sgtatham/putty/latest/htmldoc/Chapter4.html#config-nodelay>；RFC 4254 Sections 5.2/6.2/8 <https://www.rfc-editor.org/rfc/rfc4254.html>；POSIX terminal ECHO <https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap11.html#tag_11_02_05>；Mosh Technical Info <https://mosh.org/#techinfo>。
- 关键结论：russh 0.62.2 默认 `nodelay: false`，其 `connect()` 只在显式启用时调用 `set_nodelay(true)`；Tokio 说明该选项使小量数据尽快发送，PuTTY 也默认为交互连接禁用 Nagle。普通 SSH 的输入需到达远端 PTY 后由 ECHO 返回，网络 RTT 无法由客户端 socket 选项消除。Mosh 的状态同步与预测回显能改善高 RTT 体感，但需要预测确认、状态校正和不同 transport/server 架构。
- 对实施计划的影响：本轮明确设置 `Config.nodelay = true`，保留输入立即发送，加入脱敏的 UI/queue/russh/output-dispatch 阶段耗时；不实施朴素本地回显、Mosh、TCP_QUICKACK、压缩或 keepalive 伪优化。
- 未解决问题：真实网络收益取决于 RTT、服务端和中间网络；russh 调用完成与远端回显不能一一关联，需在同主机上与系统 `ssh` 做 P50/P95 手工对比。

## 2026-08-01 终端横向 resize 重排语义

- 时间：2026-08-01 19:05 +0800
- 检索问题：为什么终端窗口放宽后右侧旧内容不能恢复，AxShell 的终端模型采用何种 resize 语义？
- 检索原因：用户在反复横向 resize 后仍观察到文字被裁切，并要求参考 AxShell；终端模拟器选择会直接决定内容保留与软换行语义。
- 来源列表：AxShell 锁定提交的终端依赖和 `TerminalTab::resize` 参考实现 <https://github.com/xinalbert/axshell/tree/57246689>；Alacritty Terminal 0.26.0 `Term::resize` 与 grid resize 源码 <https://github.com/alacritty/alacritty/tree/v0.16.0/alacritty_terminal/src>；本机锁定 `vt100 0.16.2` 的 `src/grid.rs`。
- 关键结论：`vt100::Grid::set_size` 列缩窄时会截断每一物理行，右侧 cell 已被删除，之后扩宽无法恢复。AxShell 使用的 `alacritty_terminal` 在普通主屏调用 `Grid::resize(true, ...)`，将软换行的连续逻辑行重新分列；备用屏调用 `Grid::resize(false, ...)`，保持全屏程序期望的非重排语义。其高度 shrink 同时把顶部内容送入有界历史区，保持底部交互内容可见���
- 对实施计划的影响：以 `alacritty_terminal 0.26.0` 替换 `vt100` 作为 `TerminalModel` 私有实现，保留既有 DTO、UI、worker 和输入编码边界；新增主屏软/硬换行、宽字符、宽窄往返、备用屏与纵向往返回归。
- 未解决问题：目标平台需用户在真实本地 shell/SSH 全屏程序中确认布局与焦点；本轮不复制 AxShell 或 Alacritty 源码，也不把参考项目加入 Cargo 图。

## 2026-08-06 SFTP 图标与双击打开实现路线

- 时间：2026-08-06 10:33 +0800
- 检索问题：WinSCP、Cyberduck、VS Code、Qt 及三平台系统 API 如何实现远端文件图标、临时副本、默认应用打开和编辑回传；AxSSH 的 SFTP 双击应先交付哪一层能力？
- 检索原因：用户要求参考 AxShell，并检索其他软件的实现方式后制定 AxSSH 实施计划。当前 AxSSH 只有目录浏览，不能把远端路径直接交给本机程序，Transfers 也尚未有真实传输契约。
- 来源列表：详见 `docs/benchmark-grounded-method-research/sftp-icons-local-open/source-tracking.md`；核心来源包括 WinSCP `task_edit`/`temp_folders`、Cyberduck `edit`、Apple `NSWorkspace`/`UTType`、Microsoft `SHGetFileInfoW`、freedesktop MIME/Icon Theme、Qt `QFileIconProvider`、VS Code File Icon Theme/Remote Extensions 和 Rust `open::that_detached`。
- 关键结论：未检索到直接对应的统一公开 benchmark，所有产品和平台文档均为 proxy evidence。WinSCP/Cyberduck 都先下载临时副本再交给外部应用，编辑回传还需要 watch、进程复用处理、后台队列、冲突/版本和清理；VS Code 采用远端文件系统代理，不适合 AxSSH 当前的系统默认应用目标。系统图标应由平台 provider 统一转成缓存位图：macOS 优先核对现代 `iconForContentType`/UTType，Windows 可用 `SHGFI_USEFILEATTRIBUTES` 查询不存在的合成扩展名路径，Linux 采用 MIME + icon theme + hicolor 回退。
- 对实施计划的影响：建立目标 `20260806-sftp-icons-local-open`，分 P1-P9 执行。首版只做 regular file 的本地重验证后 detached open，以及远端 regular file 的有界 chunked download -> 私有 ProjectDirs cache -> fsync/rename -> detached open；目录继续导航，symlink 首版拒绝，失败/取消不打开半文件。远端下载在同一已认证 SSH worker 中开独立 SFTP subsystem task，由 worker 统一拥有取消、并发、进度和 Tab 关闭回收；图标 provider/cache 与 Slint `SftpEntryRow` DTO 分离，列表渲染不调用平台 API。
- 未解决问题：Apple 新 API feature/最低系统版本、Windows Shell/GDI 句柄到 PNG 的 DPI 与释放、Linux 主题依赖的 MSRV/系统环境、远端大文件上限、symlink 策略、缓存占用清理和真实三平台默认应用行为需要 P1/P5-P9 验证；受管编辑/上传/冲突另建目标，不在本轮。
