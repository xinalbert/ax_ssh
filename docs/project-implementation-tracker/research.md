# 项目研究记录

## 2026-10-25 E7 音频实施前标准核对与 G1 约束

- 时间：2026-10-25 11:35 +0800
- 检索问题：锁定 CPAL/PulseAudio 下，远端音频设置、实际录音需求、权限与 stream 清理应如何接线？如何将“先检索标准实现再施工”作为项目常规门禁？
- 检索原因：用户要求继续音频/媒体迁移，并明确要求将施工前检索标准实现落入 AGENTS.md；本轮实现代码尚未修改。
- 来源列表：[CPAL 0.17.3 官方 API/示例](https://docs.rs/cpal/0.17.3/cpal/)、[PulseAudio Audio Streams](https://freedesktop.org/software/pulseaudio/doxygen/streams.html)、[PulseAudio def.h/source state](https://distributions.freedesktop.org/software/pulseaudio/doxygen/def_8h.html)、[Apple requestAccess API 入口](https://developer.apple.com/documentation/avfoundation/avcapturedevice/requestaccess(for:completionhandler:))；本机锁定 CPAL、已有 audio.rs/automatic.rs 和 AudioSettings 作为项目事实。
- 关键结论：CPAL 的 Host/Device/Stream 分层要求按设备支持配置创建 stream，回调属于音频线程，示例说明跨平台启动需显式 play；PulseAudio 区分 record stream、source 和 source-output，source 空闲仍可能采集，不能用“设备存在/运行”替代所属非 corked 录音流需求，stream 使用后需断开。以上为官方 API/行为依据，不把示例称作统一强制标准。Apple 入口正文依赖 JS，Markdown 链接抓取失败，未从入口推断未读到的权限细节；本轮沿用项目已核验的 AVFoundation 适配，不重写原生权限代码。
- 对实施计划的影响：G1 在根 AGENTS.md 要求每个实施切片前外部检索并阅读官方/版本化依据，记录来源、适用版本、决策/偏离/缺口后再写实现，项目 skill 指向该门禁。音频切片优先复用现有 CPAL/rubato/标准 Pulse worker，仅增加保存格式到新 SSH worker 的应用链和明确的连接级转发选择；实际需求/权限/设备/有界 PCM 与取消仍归原 worker。连接级默认关闭、不持久化授权是本项目迁移决策，不声称由上述文档强制要求。
- 未解决问题：真实 macOS TCC/Windows 麦克风权限与 Linux Pulse/pipewire-pulse 服务器、原生 GUI/设备仍需人工或目标平台验证；Apple 页面正文未抓取，不升级 CPAL 或扩展权限实现，附件入口留独立切片。

## 2026-10-23 E6 无 Slint 的 SVG 文件图标解码

- 时间：2026-10-23 15:30 +0800
- 检索问题：如何复用锁定 resvg 的矢量解码而不引入 Slint、任意本机文件读取或错误 alpha？
- 检索原因：Linux 图标解析在 `slint::Image` 下，阻挡 egui-only 共用现有 provider；直接使用 usvg 默认资源 resolver 会读取本机路径。
- 来源列表：[resvg v0.48.1 渲染源码](https://github.com/linebender/resvg/blob/v0.48.1/crates/resvg/src/lib.rs)、[usvg v0.48.1 image resolver 源码](https://github.com/linebender/resvg/blob/v0.48.1/crates/usvg/src/parser/image.rs)；锁定 Cargo registry 的 manifest/options/pixmap 源码。web 工具不可用后，经只读 HTTPS 获取官方固定版本原文。
- 关键结论：resvg 返回 premultiplied sRGB 像素，UI RGBA 入口需要 demultiply；usvg 默认 string resolver 读取文件路径，可用自定义 resolver 拒绝。0.48.1 edition 2024、声明 Rust 1.85，项目仍为 Rust 1.95。禁用默认 features 和 image resolver 可避免图标触发字体发现、外部图片或嵌套 SVG 解析。
- 对实施计划的影响：两前端共用 `file_icons`，Linux runtime 与各平台 test 使用固定 0.48.1/default-features=false；解码 UTF-8 SVG、2 MiB 输入、512px 源边、24px 输出，禁用外部及内嵌 images。macOS/Windows resolver 仅迁移路径，不改变平台逻辑。
- 未解决问题：Linux 实际主题/Windows Shell 原生显示、其它 SDK 链接与视觉仍交原生 CI/用户，不以 macOS SVG 单测代替。

## 2026-10-22 egui 终端 IME 标准输入链路

- 时间：2026-10-22 14:14 +0800
- 检索问题：用户截图中中文提交重复时，egui/winit 和标准终端应如何区分候选态、提交文本与普通键盘文本？
- 检索原因：实验 egui 本地终端显示 `深度发掘深度发掘`；需要用标准输入链路定位，不能把用户的连续提示符截图直接归因为 PTY 或 shell 重复执行。
- 来源列表：锁定本机 `egui-winit 0.37.0` 的 `State::on_ime`、键盘文字事件和 `handle_platform_output`；锁定本机 `winit 0.30.13` 的 `Ime`/`Window::set_ime_allowed` 文档；[Alacritty 窗口事件处理](https://github.com/alacritty/alacritty/blob/master/alacritty/src/event.rs) 和 [IME 窗口设置](https://github.com/alacritty/alacritty/blob/master/alacritty/src/display/window.rs)。
- 关键结论：winit 要求编辑器在期待文本时启用 IME，`Preedit` 表示候选内容，`Commit` 表示应插入编辑器的文本；egui-winit 会把 commit 映射为 `Event::Ime(Commit)`，同时可为可打印键产生 `Event::Text`。其源码还记录 macOS 中文双拼选择候选会直接产生 `Commit`。Alacritty 同样仅显示 preedit，而把 commit 交给 terminal paste；它将 window IME purpose 标注为 Terminal，并以光标位置更新候选窗。eframe 会处理 AxSSH 写出的 `PlatformOutput.ime`，据此启用 IME、设置 Terminal purpose 和候选窗位置。
- 对实施计划的影响：终端控件保留 preedit 于 UI 层且不发 PTY；commit 立即写入一次，吞掉同帧或下一帧的相同 `Event::Text`，不影响后续独立文本。新增无窗口事件回归和真实 PTY 回归，后者验证 `Text("深度发掘") + Commit("深度发掘") + Enter` 只使 shell 收到一行中文。未把连续提示符定性为程序缺陷，仍需原生输入体验确认。
- 未解决问题：不同 macOS 输入法、AltGr/keypad、Windows/Linux 原生 IME 事件顺序及候选窗视觉位置需各平台验收；不以无窗口测试或一次截图代表这些行为已验收。

## 2026-10-22 egui 迁移版本、线程与参考实现

- 时间：2026-10-22 11:39 +0800
- 检索问题：egui/eframe 的最低 Rust、绘制/唤醒、字体回退和多窗口能否承接 AxSSH；用户指定 FigCraft 哪些实现可参考？
- 检索原因：用户要求独立分支迁移计划，并补充本地 FigCraft 为实现参考；版本、软件渲染和工具链会直接改变实施顺序。
- 来源列表：[egui 0.37 manifest](https://github.com/emilk/egui/blob/0.37.0/Cargo.toml)；[0.35 manifest](https://github.com/emilk/egui/blob/0.35.0/Cargo.toml)；[eframe features](https://github.com/emilk/egui/blob/0.37.0/crates/eframe/Cargo.toml)；[0.37 发布说明](https://github.com/emilk/egui/releases/tag/0.37.0)；[Context 唤醒/viewport 源码](https://github.com/emilk/egui/blob/0.37.0/crates/egui/src/context.rs)；用户指定 FigCraft checkout（只读，版本 `a86897dc919b8ae7fb0142fe7e0034c10715f7d9`）的 `apps/figcraft/src/{main,graphics}.rs`、`crates/ui-egui/src/{lib,theme,widgets,windows,render_worker,canvas,i18n}.rs`，对应[固定源码](https://github.com/storytold/figcraft/tree/a86897dc919b8ae7fb0142fe7e0034c10715f7d9)。联网搜索工具不可用，改用只读 HTTPS 获取官方固定版本原文。
- 关键结论：0.37 要求 Rust 1.95，0.35 声明 1.92；0.37 改善系统字体与彩色 Emoji，但不等于终端网格正确性已验证。eframe 标准 WGPU/OpenGL 不继承现有 Slint CPU/damage 策略。FigCraft 实际使用 0.36/Rust 1.95，薄 UI、theme token、定向重绘和 deferred viewport 可借鉴；其 tiny-skia 画布纹理与无界 render channel 不直接适用于终端字节流，其 macOS process exit 不能代替 AxSSH cleanup。版本/API 仍以最终锁定依赖为准。
- 对实施计划的影响：E1 先作版本/MSRV、renderer 支持策略和隔离构建决策；E2 以 Painter 终端原型验证字体/IME/性能，再提取控制层；仅合并已解析快照并正确处理跨快照变化行。E9 前迁移 Slint 翻译提取/入口校验。参考实现定位写入双语 `docs/egui-migration.md` 和 `docs/egui-migration.zh.md`，不引入参考项目依赖或资产。
- 未解决问题：候选版本实际最低工具链依赖闭包、原生 IME/字体/多窗口、软件渲染替代与性能；后续阶段实测，不在规划轮宣称通过。

历史时间字段说明：2026-10-20 整理时，缺失的精确时刻优先依据原始标题补齐；否则使用可追溯的 Git 入库时间并逐条标明依据。原始事件日期/时区保留，入库时间不等同事件发生时间，实施内容与结论未改写。允许缺少时间字段的旧月份条目保持原样。

## 2026-10-13 标准光标显隐与有界重绘

- 时间：2026-10-13 17:55 +0800
- 检索问题：如何遵循标准显隐和同步输出语义，同时减少 LLM CLI 分包重绘时的临时光标位置？
- 检索原因：用户确认无 tmux 也会偶发闪跳，并要求改成标准终端实现。
- 来源列表：[xterm DECTCEM](https://www.invisible-island.net/xterm/ctlseqs/ctlseqs.html)；[同步输出规范](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md)；[Alacritty event loop](https://github.com/alacritty/alacritty/blob/master/alacritty_terminal/src/event_loop.rs)；锁定 `alacritty_terminal 0.26.0`/`vte 0.15.0` 的本机源码。
- 关键结论：DECTCEM 只指定显隐，不定义一帧的边界；2026 才保留上一帧直到结束/超时。锁定解析器沿用 150ms 超时；Alacritty 先有界解析可用 PTY 数据再发 redraw wakeup。AxSSH 的 24ms 隐藏等待是额外启发式，无法判断复位指令何时到达。隔离探针验证正常隐藏/复位/显示和未超时同步帧不会暴露临时位置，而未同步可见移动、提前显示或同步超时均可能暴露。
- 对实施计划的影响：删除 24ms 状态；沿用标准同步帧和上游 timeout。在现有 worker channel 内按序处理最多 64 个已到达事件，再让出 Tokio 并优先到期呈现；保留 renderer/FPS/Slint 和协议模型所有权，不扩展公共 API。
- 未解决问题：未获得实际闪跳时的原始控制序列，不能断言原截图命中某个探针条件；实际 LLM CLI 的视觉验收仍需用户确认，标准实现不保证无同步保护的任意应用都无闪跳。

## 2026-10-01 Rust 1.99 原子 API 弃用与 MSRV

- 时间：2026-10-01 23:56 +0800
- 检索问题：Rust 1.99 对 `AtomicUsize::fetch_update` 的弃用和 `try_update` 稳定版本是否允许项目直接替换？
- 检索原因：macOS Intel CI 的 `-D warnings` 报弃用，项目仍声明 Rust 1.92.0 MSRV。
- 来源列表：[Rust 官方 Atomic 文档](https://doc.rust-lang.org/std/sync/atomic/struct.Atomic.html)；[Rust 官方 AtomicUsize 文档](https://doc.rust-lang.org/std/sync/atomic/type.AtomicUsize.html)。
- 关键结论：`fetch_update` 自 Rust 1.99 弃用，`try_update` 自 Rust 1.95 稳定；直接替换会让 MSRV 1.92 编译失败。`compare_exchange_weak` 可保留原有成功/失败内存序和零值行为。
- 对实施计划的影响：使用 compare-exchange 循环，不使用 `allow(deprecated)`、不提高 MSRV；另外移除 Clippy 指出的闭包多余借用。
- 未解决问题：Windows/Linux 原生 CI 的最终结果；本机已安装并验证 Rust 1.99 和 1.92。

## 2026-10-01 标准终端的输入回底与刷新视口链路

- 时间：2026-10-01 20:33 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `779726a691d6`；仅表示记录入库时间，原事件具体时分未记录。

- 原始时间：2026-10-01
- 检索问题：标准 terminal 如何在用户查看 scrollback 时处理持续输出、键盘输入和粘贴，避免视口跳动？
- 检索原因：用户要求按标准 terminal 行为修复历史视口中的输入、回车和粘贴交互，并区分输出刷新与用户输入的滚动责任。
- 来源列表：[xterm.js CoreService](https://github.com/xtermjs/xterm.js/blob/master/src/common/services/CoreService.ts)；[xterm.js Viewport](https://github.com/xtermjs/xterm.js/blob/master/src/browser/Viewport.ts)；[Alacritty event.rs](https://github.com/alacritty/alacritty/blob/master/alacritty/src/event.rs)；[WezTerm scroll_to_bottom_on_input](https://github.com/wezterm/wezterm/blob/main/docs/config/lua/config/scroll_to_bottom_on_input.md)。
- 关键结论：xterm.js 默认 `scrollOnUserInput`，仅在 buffer `ydisp` 不在底部且发生用户输入时回底；Alacritty 在输入开始和 paste 路径调用 `on_terminal_input_start`，检测 `display_offset != 0` 后滚到底部；WezTerm 默认 `scroll_to_bottom_on_input = true`，键盘和 paste 共用该检查。输出到达不强制回底；xterm.js 的同步输出还会延后 viewport sync，降低刷新期间闪动。
- 对实施计划的影响：AxSSH 保留输出到达时的 Detached 位置；在有效键或 paste 已成功编码后调用生产 `scroll_terminal_to_bottom`，视口确实改变时立即发布一次 terminal snapshot。未知键、未连接 tab 和编码失败不触发回底；不改 cursor renderer，因为 detached 位置隐藏 live cursor 是标准语义。
- 未解决问题：Slint/winit 目标平台的真实光标闪烁和连续输出视觉仍需用户验收；本轮不引入 renderer 级同步输出改造。

## 2026-10-01 SFTP 本地目录事件监听

- 时间：2026-10-01 11:34 +0800
- 检索问题：如何在 Rust 2024、MSRV 1.92.0 的跨平台桌面应用中监听当前本地目录变化？
- 检索原因：本地栏需要在 Finder、资源管理器或其他本机程序改动目录后自动更新，并避免 UI 线程轮询和高频重复扫描。
- 来源列表：[notify 8.2.0 官方 API](https://docs.rs/notify/8.2.0/notify/)；[Watcher::watch 契约](https://docs.rs/notify/8.2.0/notify/trait.Watcher.html)；`cargo info notify@8.2.0` 的 rust-version 1.77 声明。
- 关键结论：`recommended_watcher` 提供平台推荐后端，`NonRecursive` 监听目录和直接子项；默认 macOS FSEvents。事件可能合并或漏报，网络文件系统可能不发事件，不能把监听当作唯一正确性来源。
- 对实施计划的影响：锁定 notify 8.2.0；每个已加载 SFTP Tab 只监听当前本地目录，通过容量为 1 的通道合并事件，再复用现有后台目录读取，Tab 关闭时释放 watcher。保留手动 Refresh。
- 未解决问题：Windows/Linux 原生 GUI 与网络挂载目录需目标平台验收；网络目录事件缺失时仍须手动刷新。

## 2026-09-30 交互终端高延迟输入标准

- 时间：2026-10-01 12:47 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `cc31a5355f43`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：标准 SSH/TCP 和主流终端如何处理高 RTT 下的按键延迟，AxSSH 应采用哪些低风险调整？
- 检索原因：用户要求按相关建议调整项目的远端输入体验；需要区分协议标准、TCP 小包优化和预测回显架构。
- 来源列表：[RFC 4254 SSH Connection Protocol](https://www.rfc-editor.org/rfc/rfc4254)；[RFC 896 Congestion Control in IP/TCP Internetworks](https://www.rfc-editor.org/rfc/rfc896)；[RFC 9293 TCP](https://www.rfc-editor.org/rfc/rfc9293)；[OpenSSH ssh_config](https://man.openbsd.org/ssh_config)；[Mosh 官方说明](https://mosh.org/)；[Mosh 论文](https://mosh.org/mosh-paper.pdf)。
- 关键结论：RFC 4254 的交互终端仍是远端 PTY 的 SSH channel byte stream，通道窗口和最大包大小负责流控；RFC 9293 要求 TCP 提供关闭 Nagle 的方式，RFC 896 解释单字符输入的小包问题。OpenSSH 提供交互 QoS 与按键时序扰动选项，但普通 SSH 不提供本地预测回显。Mosh 通过屏幕状态同步和预测模型改善高延迟输入，属于独立传输/协议架构。
- 对实施计划的影响：AxSSH SSH 已有 `TCP_NODELAY`、无输入 batching timer 和首个远端输出立即刷新，保留现状；Telnet 使用同样的交互 TCP 小包模型，因此在连接后补 `TcpStream::set_nodelay(true)`。不在本轮引入本地预测回显或 Mosh 后端，避免破坏密码、readline、vim 和 ANSI 控制序列语义。
- 未解决问题：真实高 RTT 环境的 P50/P95 仍需同一主机上与系统 `ssh`/Telnet 客户端对比；`TCP_NODELAY` 的收益取决于链路和远端实现，不能消除远端回显 RTT。

## 2026-09-29 Windows/Linux 无边框菜单顶栏

- 时间：2026-09-29 17:42 +0800
- 检索问题：锁定的 Slint 1.18.1 能否让 Windows/Linux 以应用菜单替代可见原生标题栏，同时保留快捷键、拖动和缩放？
- 检索原因：用户要求菜单直接占据窗口顶部，去掉单独的标题行，并保持现有多窗口交互。
- 来源列表：[Slint Window 属性与隐藏 MenuBar](https://docs.slint.dev/latest/docs/slint/reference/window/window/)；[Slint WindowMoveArea](https://docs.slint.dev/latest/docs/slint/reference/window/windowmovearea/)；[Slint ContextMenuArea](https://docs.slint.dev/latest/docs/slint/reference/window/contextmenuarea/)；[winit 0.30 Window 接口](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html)；[Windows 自定义标题栏交互](https://learn.microsoft.com/en-us/windows/apps/design/basics/titlebar-design)；本仓库锁定的 `i-slint-compiler 1.18.1` 和 `vendor/i-slint-backend-winit` 源码。
- 关键结论：`no-frame` 与 winit 专用的 `resize-border-width` 可去掉原生框线并保留边缘缩放；`WindowMoveArea` 可让应用绘制的空白区域触发系统拖动；`ContextMenuArea.show()` 可由顶栏按钮主动展开菜单。不可见的 `MenuBar` 不占布局空间，仍保留快捷键；ContextMenuArea 菜单项本身不能声明 `shortcut`。
- 对实施计划的影响：macOS 继续保留屏幕菜单和 AppKit 标题栏。Windows/Linux 使用应用内菜单顶栏，现有 MenuBar 只保留快捷键，两个菜单表面共享根组件命令分发。新顶栏高度必须在终端几何和 SFTP 原生拖放坐标边界补偿。
- 未解决问题：Windows 11 最大化按钮悬停的 Snap Layout、Linux X11/Wayland 窗口管理器行为和真实菜单视觉需在原生平台由用户验收。

## 2026-09-19 终端显示、滚动与查询应答标准化

- 时间：2026-09-19 14:47 +0800
- 检索问题：终端是否应按光标可见性强制帧对齐，鼠标/尺寸/颜色查询应由 UI 猜测还是依协议请求，以及当前实现还缺少哪些影响互操作的显示和回报行为？
- 检索原因：用户要求逐项对照标准终端的显示、滚动、查询上报和对齐语义，判断剩余未实施部分并完成收口。
- 来源列表：xterm 控制序列参考的 [DECSET、窗口操作、OSC 颜色和 DECSCUSR](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)；Contour 的 [synchronized output 扩展说明](https://contour-terminal.org/vt-extensions/synchronized-output/)；锁定的 `alacritty_terminal 0.26.0` 本机 `Processor`、`Event`、`Colors` 和 `WindowSize` 实现。
- 关键结论：`CSI ?25l/h` 只描述光标可见性；呈现事务只能由 `CSI ?2026h/l` 和其超时兜底控制。终端只在应用发出颜色或窗口查询时回答，不会主动请求 UI 对齐。`CSI 14 t` 是文本区像素，`CSI 16 t` 是字符单元像素，二者必须使用最新的实测 cell 尺寸。动态 OSC 调色板既影响查询回复也影响已有和后续 cell 的可见颜色。
- AxSSH 落地：终端模型在 snapshot 边界解析动态颜色，保留 block/空心 block/underline/beam 光标和 SGR 下划线变体/指定颜色；模型为核心未暴露的 `CSI 16 t` 保留分片安全的窄解析器。协议响应、延后布局查询和 renderer 都保持有界，且 UI 不接触 transport 或终端颜色表。标准 xterm 鼠标路由为默认策略，本地选区优先为显式可选模式。
- 安全与未支持项：`OSC 8` 超链接已实现为有界、仅显式 HTTP(S) 打开；`OSC 52` 默认关闭，显式开启后允许有界的远端写入本机默认剪贴板，远端读取必须逐次经过用户确认且只读取默认 clipboard，selection clipboard 仍关闭，不能因远端输出自动取得系统剪贴板读取能力。图形协议和额外设备属性属于后续扩展，不伪造响应。
- 对实施计划的影响：TERMSTD1--3 覆盖协议、渲染和文档边界，完成后维持现有 SSH 信任、凭据和有界 worker 设计，不为未支持扩展增加猜测性响应。
- 未解决问题：真实 TUI、平台字体、光标/下划线视觉和 PTY 像素行为仍需在目标平台手动验收。
- 验证边界：离线单元/状态/Slint 编译验证协议数据流与有界性；真实 TUI、光标与下划线视觉、平台 PTY 像素行为仍由目标平台用户验收。

## 2026-09-15 全应用键盘事件与终端输入协议

- 时间：2026-09-16 08:57 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `7ee3722159e9`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：终端输入、快捷键录制、普通文本/IME、物理数字小键盘和日志是否应共享同一个应用级键盘事件；AxSSH 应如何参考 AxShell 以及其它终端程序的输入分层？
- 检索原因：原实现的 Slint callback 仍拆成多个 `bool` 与原始文本，`log_keyboard_event` 会重新构造终端键，`NormalizedKeyboardInput.key` 也直接使用终端专用 `TerminalKey`，导致物理键身份、逻辑键语义和终端编码边界混在一起。
- 来源列表：W3C UI Events 的 `KeyboardEvent` 模型（`key`、`code`、`location`、`repeat`、`isComposing`）；锁定 `winit 0.30.13` 的 `KeyEvent`（`logical_key`、`physical_key`、`text`、`location`、`repeat`）和 `WindowEvent::KeyboardInput.is_synthetic`；本仓库锁定的 Slint 1.17.1 struct type mapping；本地 `third_package/axshell` 的 `KeyDownEvent.keystroke`、terminal action、key encoding 和 backend input 分层；kitty keyboard protocol 的 progressive CSI-u 事件；xterm.js 的 `onKey` 事件与 `onData` 终端字节边界；Alacritty/WezTerm 的按键绑定到终端输入编码路径。
- 关键结论：应用层应保留通用逻辑键/文本、物理 `code`/`location`、修饰键快照、composing/repeat/synthetic 元数据；快捷键、日志、终端路由和录制器都消费同一个事件对象。终端 emulator 的最后一步才把通用键映射成控制字节、CSI/SS3、application-cursor 或 application-keypad 序列；application-keypad 不能反过来污染应用层键值。
- AxShell 对照：AxShell 的 `KeyDownEvent.keystroke` 是应用内统一入口，workspace keybinding、密码提示、字符直发和 `encode_key` 都消费同一个事件；AxSSH 采用同样的边界思想，但额外保留 Winit physical code/location 以支持 NumLock 与 DEC application-keypad，不引入参考工程代码或依赖。
- 其它终端对照：成熟终端通常把“平台键事件”与“送往 PTY 的字节流”分层；配置快捷键在前，未被消费的键进入终端编码器。普通模式依赖文本/逻辑键，特殊键依赖规范化名称，application modes 决定导航和小键盘的最终序列；Kitty 的扩展键盘协议属于终端协议输出能力，不应成为 UI callback 的参数形状。
- AxSSH 落地：新增 `ui/components/keyboard-input.slint` 的 `KeyboardEvent`、终端上下文 DTO 和日志 DTO；`src/app.rs` 只做一次 UI DTO -> `NormalizedKeyboardInput` 转换；`NormalizedKeyboardInput.key` 改为 `ApplicationKeyboardKey`；只有 `src/app/terminal_bridge.rs` 在调用 `src/terminal/input.rs` 前转换为 `TerminalKey`。物理小键盘通过独立的 `physical_keycode`/`location` 保留，并在所有桌面平台由活动终端的 application-keypad 模式决定是否发送 SS3。
- 对实施计划的影响：将输入协议标准化拆分为应用层规范化事件和终端层字节编码两步，Kitty CSI-u 继续保持默认关闭，并把目标平台 physical key/location 实机验证列为后续验收。
- 未解决问题：Winit/Slint 在各目标平台实际报告的 `physical_keycode`、`location`、IME dead-key 和合成事件细节仍需 Windows/macOS/Linux 实机验证；Kitty CSI-u 等扩展协议暂不默认启用，避免改变现有远端 TUI 兼容性。

## 2026-09-14 macOS SFTP 原生 file-promise 拖放

- 时间：2026-09-14 14:10 +0800
- 检索问题：如何让尚未落地到本机的远端 SFTP 普通文件作为 macOS 原生拖放目标交给 Finder，同时把同一次拖动返回 AxSSH 时安全下载到开始时的 Local files 目录？
- 检索原因：Slint 的进程内 `DataTransfer` 文本 payload 不能成为 Finder 可接收的文件；将内容提前写到公开临时目录会扩大泄露与清理边界。
- 来源列表：Apple `NSFilePromiseProviderDelegate` 的 [`filePromiseProvider:writePromiseToURL:completionHandler:`](https://developer.apple.com/documentation/appkit/nsfilepromiseproviderdelegate/filepromiseprovider%28_%3Awritepromiseto%3Acompletionhandler%3A%29) 文档；锁定的 `objc2-app-kit 0.3.2` `NSFilePromiseProvider`、`NSDraggingSource`、`NSDraggingDestination` 绑定；本仓库 `src/sftp/transfer/{local,}.rs` 的 `LocalDownloadTarget` 和下载 writer。
- 关键结论：AppKit 在目标接受拖放后才由 delegate 获得承诺文件的最终 URL，delegate 应异步完成写入并调用 completion；可指定 delegate operation queue。实现将该队列限定为 AppKit 主队列，使原生对象和 registry 不跨线程；它只快速排入 owned SFTP 请求，网络和文件写入仍由 worker 完成。原生 source 为 copy-only，发起窗口的 destination 只接受同一 source，并使用开始时捕获的 local target。
- 安全与所有权：不产生公开预下载副本；最终路径仍进入既有 `SftpDownloadRequest`/`LocalDownloadTarget`，继续执行目录重验、拒绝符号链接和既有目标、`.part` fsync 与原子无覆盖发布。Slint 只发出路径 intent；AppKit bridge 不接触 russh handle，SFTP worker 不接触 AppKit/Slint 对象。目录、符号链接和被过滤条目不启用 file promise。
- 对实施计划的影响：SFTPDND1 采用单文件 `NSFilePromiseProvider` 与 copy-only source；SFTPDND2 在 worker 排队前将传输 ID 映射到 native drag ID，以回送异步 completion，且将 NSURL 转为绝对 final path 后复用现有安全 writer。
- 未解决问题：Finder 目标、跨应用取消、返回源窗口的命中区域和原生错误显示只能在目标 macOS GUI 中验收；本地测试不代替这些行为。
- 验证边界：离线编译、Clippy 和 Rust 回归可验证类型、队列和本地目标构造；Finder 目标、跨应用取消和将同一拖动返回当前 AxSSH 窗口需要在目标 macOS 上由用户手动验收。

## 2026-08-26 russh 客户端稳定性与主机密钥回调升级

- 时间：2026-08-26 19:00 +0800
- 检索问题：当前锁定的 `russh 0.62.2` 是否仍包含客户端 Curve25519 崩溃/拒绝服务风险；升级到哪个修复版本会改变 AxSSH 的 host-key trust API 或安全边界？
- 检索原因：安全评估发现 `russh` 0.62.2 低于两个 Curve25519 修复版本；随后 0.63.1 又修复客户端 channel-ID 校验和非默认 MAC-none/block-cipher panic，需要在不放宽信任的前提下落地。
- 来源列表：RustSec advisories <https://github.com/advisories/GHSA-g9hv-x236-4qp3>、<https://github.com/Eugeny/russh/security/advisories/GHSA-5xvq-cp9x-6p6r>、<https://github.com/Eugeny/russh/security/advisories/GHSA-47hw-gvq5-r2gm>、<https://github.com/Eugeny/russh/security/advisories/GHSA-p8qx-h547-fjw9>；russh release list <https://github.com/Eugeny/russh/releases>；本机 `russh 0.63.1` `client::Handler`、`PublicKeyOrCertificate` 和 AxSSH `src/ssh.rs`。
- 关键结论：0.62.2 受影响于错误长度/全零 Curve25519 参数导致的客户端 pre-auth DoS/panic，0.62.4 起修复；0.63.1 还修复了客户端 Handler channel-ID 校验问题和非默认 MAC-none/block cipher panic。0.63.1 的 `check_server_key` 参数变为 `PublicKeyOrCertificate`；AxSSH 只匹配 `PublicKey { key, .. }`，证书直接拒绝，避免调用 `public_key()` 将证书隐式降级为普通公钥。
- 对实施计划的影响：`Cargo.toml`/`Cargo.lock` 锁定 0.63.1；profile fingerprint、OpenSSH known_hosts、revoked/changed 决策和认证顺序保持不变。CI 增加严格 Clippy、MSRV 1.92 locked check 与 RustSec audit；Release 的跨平台 build 依赖 Linux 预检和 audit。
- 未解决问题：AxSSH 尚未实现证书颁发机构信任，因此证书服务端仍不可用；`rustsec/audit-check` 需在 GitHub-hosted runner 访问 advisory 数据库，目标平台真实 SSH/agent/SFTP 互操作仍需手工验收。

## 2026-08-25 macOS Software damage-aware tile present

- 时间：2026-08-25 21:35 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `001e9c1e3a5e`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：如何在 macOS Software renderer 中缩小 damage 提交范围，同时保持 CPU framebuffer 复用和 Core Animation 异步读取安全？
- 检索原因：用户 sample 显示完整 `CGImage` clone、CoreGraphics image-data lock 和 vImage color conversion 是 Software 路径主要热点，需要评估 tile 化提交是否能降低成本。
- 来源列表：用户提供的 macOS sample、锁定的 `softbuffer`/winit renderer 实现和本地实验结果。
- 关键结论：持久 CPU framebuffer 配合 256x128 child-layer tiles 可只复制受 damage 影响的区域，旧 tile image 继续由对应 layer 持有，避免把下一帧写入仍被 Core Animation 读取的图像。
- 对实施计划的影响：保留 application 层 `TerminalRenderLine`/`TermDamage` 边界，tile model 只存在 renderer 适配层，并将首帧、resize、scale、invalidate 和恢复都视为完整提交。
- 未解决问题：目标 macOS 仍需同一软件负载复测 CPU/footprint，并人工确认首帧、resize、Retina、隐藏/恢复、滚动、光标和选区。

- 实施原因：用户 sample 显示单 layer 完整 `CGImage` clone、CoreGraphics image-data lock 和 vImage color conversion 是 Software 路径主要热点；需要在保持 CPU framebuffer 可复用和 Core Animation 异步读取安全的前提下缩小提交范围。
- 实施结论：macOS `softbuffer` 仍持有一个持久 CPU framebuffer，但 presentation surface 固定划分为 256×128 物理像素 child layers。winit 转发的每个物理 damage rectangle 映射到相交 tile；每个更新 tile 从 framebuffer 的对应行复制到独立拥有的 `CGImage`，未变化 tile 保留旧图像。
- 安全边界：不把下一帧写入同一 `CGImage` provider；旧 tile image 继续由对应 layer 持有，直到 Core Animation 替换/回收。首帧、resize、scale 变化、surface invalidate 和恢复都使 age 归零并完整提交所有 tile。应用层仍只有 `TerminalRenderLine`/`TermDamage`，没有 tile model 或配置。
- 验证与遗留风险：根 workspace 的 `cargo fmt --all -- --check`、locked/offline check、严格 Clippy、完整测试（209 库、200 应用）和翻译检查通过；vendor 独立 manifest 不能运行 dev-dependency 测试。目标 macOS 仍需用同一软件负载复测 CPU/footprint，并人工确认首帧、resize、Retina、隐藏/恢复、滚动、光标和选区。

## 2026-08-24 macOS Software 持久 framebuffer 与单 layer 提交

- 时间：2026-08-24 14:30 +0800
- 检索问题：撤销 CoreAnimation child-layer slicing 后，macOS Software backend 应保留哪些 buffer 生命周期和 damage 接口行为？
- 检索原因：用户反馈切片路径造成终端、Tab、Settings 坐标错位和空白区域，需要把最终 surface 提交恢复到单 layer，同时保留持久 framebuffer 的正确性前提。
- 来源列表：本地 `vendor/softbuffer/src/backends/cg.rs`、`vendor/i-slint-backend-winit/renderer/sw.rs`、锁定 `softbuffer 0.4.8` 与 Slint 1.17.1 源码，以及用户目标 macOS 验收反馈。
- 实施结论：为修复 Software renderer 的 buffer 生命周期，同时避免坐标错位，macOS `softbuffer` backend 持有持久 CPU framebuffer；有效帧报告 `age() == 1`，首帧、resize、Retina scale 变化、surface invalidate 或窗口恢复后重新建立有效帧状态。不创建 CoreAnimation child-layer tile 网格。
- 提交方式：CoreAnimation backend 每次从持久 framebuffer 创建独立拥有的完整 `CGImage`，设置到唯一的完整 `CALayer.contents`；`present_with_damage` 忽略 damage 参数并走同一完整提交路径。这样不会把 dirty rectangle 误解释成 layer slicing，也保留了已验证的上游坐标和 Retina 几何语义。
- 失效链路：softbuffer 增加跨平台 no-op `Surface::invalidate()`；winit `occluded` 同步使 surface age 失效。若 age 为 0 且本帧 region 为空，winit 仍调用完整 present，保证首帧和窗口恢复后的空白区域不会漏提交。
- 测试：保留 buffer age、resize、失效传播和完整 present 的 focused 覆盖；不再维护 tile 去重、tile 图像或切片坐标测试。vendor 单独 manifest 的测试若缺少离线 dev-dependency 仍不能启动时，以根 workspace 的 locked/offline 生产检查为准并记录原因。
- 兼容性边界：macOS 默认仍优先 Skia/Metal；本 backend 只服务明确选择 Software 的路径。应用层继续使用单层 `TerminalRenderLine` model、`TermDamage` 和 Slint 内部 dirty region，不重新引入应用 tile/partition；winit 的多矩形 forwarding 只保留为跨平台提交接口能力。
- 关键结论：macOS 不再创建 child layer，也不按 dirty rectangle 切片；`age() == 1` 只建立在持久 framebuffer 已由 Slint 绘制完成的前提上，`present_with_damage` 在该 backend 仍是完整 present。
- 对实施计划的影响：BACKEND3 改为持久 framebuffer + 单 CoreAnimation layer 完整提交；应用层保持单层 `TerminalRenderLine`，不恢复 tile/partition；winit 多矩形 forwarding 作为独立跨平台接口保留，但不宣称 macOS 局部提交。
- 未解决问题：Software 仍可能承担完整 framebuffer 的 CPU 拷贝和 `CGImage` 创建成本；需要目标 macOS 在持续输出、resize、Retina、隐藏/恢复、滚动和光标动画下继续做视觉与 CPU A/B 验收。

## 2026-08-23 绕过 Slint 1.17.1 dirty-present 限制

- 时间：2026-08-23 19:20 +0800
- 检索问题：终端已有 dirty row/tile model 更新后，如何绕过 Slint/winit/softbuffer 的限制，把脏区真正提交到窗口 framebuffer。
- 来源列表：本机锁定的 Slint 1.17.1 `i-slint-renderer-software` partial renderer、`i-slint-backend-winit` software renderer、`i-slint-renderer-skia` software surface、`softbuffer` 0.4.8 CoreGraphics backend；上游 PR [#12758](https://github.com/slint-ui/slint/pull/12758) 及其修复 commit `33033ceb103483d4a030a97417a1d371db5809b8`；Softbuffer API 文档和本仓库 macOS sample。
- 关键结论：Slint partial renderer 已经计算多个 dirty rectangles，但锁定的 winit software 代码只提交 `bounding_box_*`；#12758 已在 2026-08-03 合并却晚于 v1.17.1，因此本地 patch 将 `PhysicalRegion::iter()` 逐项转换为 `softbuffer::Rect`。macOS softbuffer 0.4.8 的 `age()` 固定为 0，`present_with_damage` 等价于 `present()`，每帧把完整 `CGImage` 设置到一个 CALayer；仅修 winit 不能改变该行为。
- 复核结论：上游 `softbuffer 0.4.8` 在 macOS 上每帧分配新 buffer、返回 `age() == 0`，并把完整 buffer 作为一张 `CGImage` 设置到单个 `CALayer`；`present_with_damage` 明确等价于 `present()`。Slint 官方 software backend 也把多块 dirty region 合并为 bounding box 后提交。此前新增的 512x64 child layer 网格不是上游契约，可能把旧像素、坐标翻转和 Retina layer 几何问题引入显示链，因此已撤回；当前 macOS patch 仅保留持久 framebuffer、`age() == 1`、失效传播和单 layer 完整 present。保留 winit 的 `PhysicalRegion::iter()` forwarding，因为这是 Slint 后续独立修复，且只影响真正支持 damage 的平台。GPU/Metal 路径仍由 Skia partial renderer 裁剪绘制，但 CAMetalLayer drawable 仍正常 present，不假称为 partial present。
- 风险与边界：本地 patch 只触及 winit 的 damage forwarding；macOS surface 保持锁定上游实现，不触及 AxSSH 的 Slint UI、终端 parser、worker、transport、SSH trust 或凭据。升级 Slint/softbuffer 时必须重新对照上游实现和锁文件。
- 检索原因：该 backend 方案需要确认锁定版本的 dirty-region、buffer age 和 CoreAnimation layer 行为，避免把应用层 tile 更新误认为最终 surface 局部提交。
- 对实施计划的影响：保留本地 winit/softbuffer patch，并将目标平台 Software renderer 的 DPI、resize、闪烁、残影和方向复验列为未完成验收项。
- 未解决问题：目标 macOS 上仍需用户重启当前候选，确认上游兼容的 Software renderer 完整 present 已消除 Settings、Tab、空白区域和终端旧帧错位；GPU/Skia 与 Software 的同负载 A/B 仍待确认。

## 2026-08-23 Fontique 路径字体源与内存占用

- 时间：2026-08-23 11:54 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `7383ae6d226d`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：Maple/Iosevka/Monaspace 是否需要把完整 TTF 读入 `Vec<u8>`，以及改用路径源后能否降低应用侧常驻内存。
- 检索原因：用户提供的 SoftwareRenderer sample 中，Maple Regular/Bold 两个 anonymous malloc 合计约 39.3 MiB，与资源文件大小相符；当前 `src/app/font_bridge.rs` 先 `fs::read` 再通过 Fontique `Blob` 注册。
- 来源列表：锁定 Fontique 0.10.0 的 `Collection::load_fonts_from_paths`、`SourceKind::Path`、`SourceCache`、`font::FontInfo` 实现；Slint 1.17.1 `fontique_010::shared_collection` 文档；用户提供的 2026-08-23 10:04 sample 及 `assets/fonts/` 文件大小。
- 关键结论：Fontique 路径扫描阶段使用 mmap 读取字体元数据，注册结果只保留路径源；字体实际数据在查询时由 `SourceCache` 按路径加载并以共享 `Blob` 复用。把外部 TTF 改为路径列表可以移除 `FontResources` worker 返回的完整 `Vec<u8>` 和其 `Arc<Blob>` 长期持有。JetBrains Mono 仍是 Rust `include_bytes!` 的嵌入式 UI 字体，继续使用 memory source，避免改变启动可靠性。
- 实施边界：只校验路径存在、常规文件和 0 < size <= 24 MiB；只传入 manifest 中声明的字体文件，不扫描整个目录。Maple 注册后通过 `family_id` 设置唯一 Hani fallback；不添加系统 CJK fallback、不做动态注销、不调整字体字重或 renderer。
- 预期收益与风险：Maple 当前约 39.3 MiB 的完整文件副本不再由应用加载任务持有，渲染期间仍可能出现 mmap/字体解析/glyph cache，且 Fontique/Slint shared collection 没有本轮可靠的运行时 family 注销契约；需同条件 macOS sample 验证 `MALLOC_LARGE`、anonymous malloc、footprint 和 `vmmap` 变化，不能仅凭单次 RSS 下结论。
- 对实施计划的影响：MEM6 采用路径 source 作为外部字体的唯一注册方式，保留 JetBrains 嵌入 source；不扩大到系统 CJK fallback、动态字体注销或 renderer 分支。
- 未解决问题：真实 macOS 运行中 Fontique source cache、glyph cache、mmap 和 allocator 的峰值/驻留比例仍需同 renderer、窗口和字体负载复测；软件 renderer 的 framebuffer 仍是独立内存热点。
- 外部链接：<https://docs.rs/fontique/0.10.0/fontique/struct.Collection.html>、<https://docs.rs/fontique/0.10.0/fontique/enum.SourceKind.html>、<https://docs.rs/slint/1.17.1/slint/fontique_010/fn.shared_collection.html>。

## 2026-08-22 macOS Software renderer 对照复核

- 时间：2026-08-22 22:38 +0800
- 检索问题：新的 macOS sample 是否确实使用 SoftwareRenderer，以及它与上一份 GPU/Skia/Metal sample 对内存、线程和刷新成本有什么可比结论。
- 检索原因：用户明确指出新 sample 是 soft renderer，要求继续核对；需要把 renderer 路径差异与应用自有生命周期机制分开。
- 来源列表：用户提供的 2026-08-22 22:38 macOS `/usr/bin/sample` 输出；同日 22:24 GPU sample；两份 sample 的 Mach-O UUID、线程栈和 Slint 1.17.1 backend 符号。
- 关键结论：PID 90375 的 UUID `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC` 与 GPU sample 相同。22:38 sample 的主线程 2,033 个样本中约 1,320 个 DisplayLink、1,185 个 `WinitSoftwareRenderer`、1,172 个 `draw_contents`、966 个 software `render_buffer_impl` buffer 遍历，确认 SoftwareRenderer 仍走 CPU/softbuffer 整帧热点；约 687 个样本在 CoreFoundation wait。4 个 `axssh-tokio` 线程仍在，且 sample 中同时出现 SSH session/monitor 和 UI refresh 调度。footprint/peak 为 120.4/175.1 MiB，低于 22:24 GPU sample 的 229.4/267.2 MiB，但两次启动时长、窗口/pane、输出量和运行期缓存没有完全固定，不能把差值当作严格 renderer 内存基准或泄漏证据。
- 对实施计划的影响：确认 Software/GPU 的 renderer 区分只影响呈现资源和 CPU/GPU surface 成本，不改变 MEM1-MEM3 的线程、任务、缓存和 drop 生命周期机制。PERF14 仍需同一 release、同一窗口/pane/负载和相同 renderer 的重复采样；Software 侧应单独记录 DisplayLink、`render_buffer_impl`、`vmmap -summary` 与 footprint。
- 未解决问题：当前两份 sample 不是严格同条件 A/B，无法量化 GPU surface 的长期缓存成本、软件 framebuffer 复用和应用缓存释放；后续仅在用户主动要求时复查。

## 2026-08-22 macOS GPU renderer 下的内存与线程生命周期复核

- 时间：2026-08-22 22:24 +0800
- 检索问题：用户提供的 macOS sample 是否证明内存/线程生命周期机制没有生效；这些机制是否应按 Software 或 GPU renderer 分别实现。
- 检索原因：用户要求只在主动提出时复查记录；本次 sample 的 footprint 高于之前 Software sample，需要防止把 renderer 的平台资源误判为应用生命周期失效或泄漏。
- 来源列表：用户提供的 2026-08-22 22:24 macOS `/usr/bin/sample` 输出；当前 `target/debug/ax_ssh` 的 Mach-O UUID；`src/app.rs` 的有界 Tokio runtime 配置；Slint 1.17.1 锁定 GPU/Skia backend 栈。
- 关键结论：PID 89392 的 UUID 为 `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC`，与当前 debug 二进制一致。physical footprint/peak 为 229.4/267.2 MiB；主线程 2,254 个样本中约 1,952 个在 CoreFoundation 等待、272 个进入 DisplayLink、271 个进入 Skia `MetalSurface::render`、223 个进入 dirty-region `draw_contents`，确认当前为 GPU/Skia/Metal renderer。4 个 `axssh-tokio` 线程仍受 runtime 上限控制且主要等待。Tokio async/blocking 上限、blocking 空闲回收、SFTP 图标缓存清理、`Weak<AppState>`、session/PTY/SSH/SFTP worker shutdown 与 Rust/Slint 对象 drop 都跨 renderer 适用；按 renderer 区分只用于解释 Software framebuffer/softbuffer 与 GPU Skia surface、CAMetalLayer drawable、Metal command buffer、Fontique、CoreAnimation、allocator 等不同的缓存和呈现资源。应用 drop 不承诺这些平台缓存立即降低 RSS。
- 对实施计划的影响：MEM1-MEM3 维持完成，不为不同 renderer 重复实现生命周期逻辑。PERF14 的内存验证必须固定同一 release、renderer、窗口尺寸、pane 数和负载，至少三轮记录 footprint、`vmmap -summary`、线程数以及打开/关闭 Settings、Terminal、SFTP 前后差异；不以单次 footprint/peak 或跨 renderer 对照判断泄漏。
- 未解决问题：该 sample 是 debug 构建且没有同负载 release A/B，尚不能量化应用缓存、Skia/Metal 资源和 macOS allocator 各自的 footprint；后续仅在用户主动要求时按上述条件复查。

## 2026-08-22 Slint 脏区域/脏行刷新上游状态

- 时间：2026-08-22 17:57 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `c5eed6f7c12b`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：Slint 对单行 model 更新、partial rendering 和多矩形 dirty region 的处理是否仍有已知问题，相关修复是否已合并，以及当前锁定版本是否包含修复。
- 检索原因：AxSSH 依赖 Slint 1.17.1，终端输出已在应用层按脏 terminal 和稳定行 model 做增量更新；需要确认上游 renderer 是否仍会扩大脏区域，避免把应用层优化误判为 Slint 已支持按行提交。
- 来源列表：Issue/PR [#2143](https://github.com/slint-ui/slint/issues/2143)、PR [#4268](https://github.com/slint-ui/slint/pull/4268)、PR [#9467](https://github.com/slint-ui/slint/pull/9467)、PR [#9743](https://github.com/slint-ui/slint/pull/9743)、PR [#10725](https://github.com/slint-ui/slint/pull/10725)、PR [#11441](https://github.com/slint-ui/slint/pull/11441)、Issue [#12752](https://github.com/slint-ui/slint/issues/12752)、已合并 PR [#12758](https://github.com/slint-ui/slint/pull/12758)、仍开放 PR [#12656](https://github.com/slint-ui/slint/pull/12656)、仍开放 PR [#12806](https://github.com/slint-ui/slint/pull/12806)、Slint [v1.17.1](https://github.com/slint-ui/slint/releases/tag/v1.17.1)。
- 关键结论：#2143（`set_row_data` 单行更新导致整个布局变脏）已于 2024-02-06 关闭，维护者表示现状已不再复现并指向 partial-renderer 回归测试，但没有一个可单独追踪的修复 PR；#4268、#9467、#9743、#10725、#11441 分别改善 model 依赖追踪、几何/渲染 dirty 分离、可见性、不可见项几何和变换裁剪。更直接的近期 damage bug 是 #12752：多块脏矩形被错误地以 bounding box 提交；其修复 PR #12758 已于 2026-08-03 合并，merge commit 为 `9ea33fdd4c091da7bc670f725f8f5fbaa95c858f`，实际修复 commit 为 `33033ceb103483d4a030a97417a1d371db5809b8`，改为提交 `PhysicalRegion::iter()` 的实际矩形。该修复解决 stale gap，不等于 macOS software renderer 获得按行/按矩形的高效呈现。#12656（对齐 dirty region）和 #12806（model predicate 依赖 O(1)）截至检索时仍未合并。
- 对实施计划的影响：继续保留 AxSSH 自有的 dirty terminal UUID、稳定 nested `VecModel` identity、行 revision 和呈现节流；不能把 Slint #12758 当作 macOS software surface 的整帧成本修复。升级 Slint 前必须先确认 release/changelog 是否包含 #12758，再在目标 renderer 上复测实际 damage 行为。
- 未解决问题：#12758 合并时点晚于 v1.17.1（2026-07-07），当前锁定 v1.17.1 的 `internal/backends/winit/renderer/sw.rs` 仍使用 `bounding_box_size()`；需要在后续 Slint patch/nightly 发布后重新检查源码或锁定 commit，并在 macOS GPU/software 与 Linux Wayland 分别验证。下次复查可用 `curl` 查询 PR `merged_at`、release tag 和 `raw.githubusercontent.com/slint-ui/slint/<ref>/internal/backends/winit/renderer/sw.rs` 是否已改为 `region.iter()`。

## 2026-08-14 终端纵向扩容与 scrollback 语义

- 时间：2026-08-14 08:07 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `7461716660cc`；仅表示记录入库时间，原事件具体时分未记录。

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

## 2026-08-14 GitHub Actions Retry And Same-Day Release Tags

- 时间：2026-08-14 10:29 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `1666d166cc50`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：同一日期已有 release tag 时，CI 或平台打包失败需要安全地重试；若需要第二个同日发行，应如何命名并保持包版本可区分？
- 检索原因：已有 `2026-08-14` 的 CI 和 Release 失败后不能安全地覆盖 tag；需要确定能重试已有 tag、又能创建当天第二个独立发行的 GitHub Actions 约定。
- 来源列表：GitHub Docs, [Triggering a workflow](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow), [Reusing workflows](https://docs.github.com/en/actions/how-tos/sharing-automations/reusing-workflows), [Using concurrency](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)；用户指定的 AxShell 参考仓库近期 tag 使用 `v2026.7.16-1`。
- 关键结论：`workflow_dispatch` 应接收显式 tag input，`workflow_call` 可复用同一验证/CI 等待链路，concurrency group 应按 tag 串行化。创建新 tag 与重试已有 tag 必须分离；重试路径不应拥有 tag 写权限。第二次同日发行采用 `YYYY-MM-DD-1`，而不是覆盖首个 tag。
- 对实施计划的影响：Create workflow 保留新 tag 创建职责并调用可复用 retry flow；Retry Existing Release 只验证/dispatch 既有 tag。版本工具接受可选正整数 revision，将第二个 tag 映射为区分的 Cargo、Debian 与 macOS build 版本；Highlights 同时识别修订日期 tag。
- 未解决问题：GitHub-hosted tag CI、平台构建和 Release 附件仍需远端执行；已发布的 tag 不得移动，后续修复应创建下一个修订 tag。

## 2026-08-18 xterm 鼠标能力与事件修饰键时态

- 时间：2026-08-18 16:05 +0800
- 检索问题：xterm 的 1007 alternate-scroll 是否应激活按钮 reporting；SGR/传统 release 和 motion 的 modifier bits 应取按下时还是各事件发生时的状态；主流终端如何区分所需 mouse event 类型。
- 检索原因：当前 AxSSH 将备用屏 1007 合并进单一 `mouse_reporting_active`，并在 release/cancel 复用按下时修饰键，可能造成按钮 owner 误判和事件语义偏差。
- 来源列表：xterm control sequences <https://invisible-island.net/xterm/ctlseqs/ctlseqs.txt>；xterm `button.c` <https://github.com/ThomasDickey/xterm-snapshots/blob/master/button.c>；xterm.js `MouseStateService.ts` 与浏览器 `MouseService.ts` <https://github.com/xtermjs/xterm.js/tree/master/src/common/services>、<https://github.com/xtermjs/xterm.js/blob/master/src/browser/services/MouseService.ts>。
- 关键结论：DECSET 1007 只在备用屏将滚轮转换为 cursor up/down，不启用 button press/release/drag；1000/1002/1003 分别决定 button、drag 和 all-motion。xterm 的 `BtnCode` 直接读取当前 X event `state`，xterm.js 也从当前 `mouseup`/`mousemove` 读取 Shift/Alt/Ctrl。xterm.js 按协议暴露 DOWN/UP/WHEEL/DRAG/MOVE 独立事件位，而不是一个总布尔值。
- 对实施计划的影响：Rust snapshot 拆分 button 与 wheel reporting；Slint 按 button capability 决定 owner/右键菜单、按 wheel capability 决定滚轮；release 使用 pointer-up modifier，cancel 使用最后 pointer motion modifier。高频 motion 采用最新值合并并允许背压丢弃，press/release 保持可观察。
- 未解决问题：Slint 在三平台窗口失活时提供的 cancel modifier 完整性需要用户实机确认；xterm.js 的 DOM 监听实现只作事件能力参考，不复制其源码或引入依赖。
## 2026-08-21 Slint 静态行缓存与终端节点削减

- 时间：2026-08-21 15:25 +0800
- 检索问题：Slint 1.17.1 的 `cache-rendering-hint` 能否由运行时配置控制、哪些 renderer 会实际缓存，以及终端行应缓存哪些内容。
- 检索原因：用户要求把降低 item 数量和行级缓存同时实现为配置项；缓存范围和默认值会直接影响 CPU、GPU 内存、光标闪烁与选区正确性。
- 来源列表：Slint 1.17.1 Skia `visit_layer`/`LayerRenderer` <https://github.com/slint-ui/slint/blob/v1.17.1/internal/renderers/skia/itemrenderer.rs>；Slint 1.17.1 FemtoVG item renderer <https://github.com/slint-ui/slint/blob/v1.17.1/internal/renderers/femtovg/itemrenderer.rs>；Slint 1.17.1 partial renderer 回归 <https://github.com/slint-ui/slint/blob/v1.17.1/api/rs/slint/tests/partial_renderer.rs>；本机锁定的 `i-slint-renderer-{skia,femtovg,software}` 与 `i-slint-core` 1.17.1 源码。
- 关键结论：`cache-rendering-hint` 是可绑定的 bool 属性；Skia/FemtoVG 在启用时为该 Layer 创建并复用离屏图像，software renderer 没有等价 layer image cache。若整个终端行同时包含光标、选区或目标反馈，这些高频交互依赖会使缓存反复失效；缓存层应只包含行背景、文字和固定装饰。默认背景矩形和每 run 的透明包装 Rectangle 可以在不改变终端模型、字形内容或交互覆盖顺序的前提下省略。
- 对实施计划的影响：增加两个独立且可预览的 Appearance 设置；紧凑节点默认开启，静态行缓存默认关闭。TerminalGrid 在紧凑分支只为非默认背景创建 Rectangle，并直接放置 Text/装饰；缓存只包住该静态分支，光标、选区、目标高亮和 IME 保持在层外。
- 未解决问题：Skia cache 的实际 CPU 收益和 Retina 纹理 footprint 必须用相同窗口、pane 数和持续输出 A/B；software 模式不预期从 layer cache 获益，目标平台视觉一致性由用户验收。

## 2026-08-23 连续窗口 resize 的合并与刷新范围

- 时间：2026-08-24 07:32 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `38b3a2a1d445`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：Slint/Winit 及成熟终端在连续窗口 resize 期间如何避免重复 surface/model 重排和闪烁？
- 检索原因：用户观察到改变窗口大小时画面闪烁、不流畅，需要确认应用层 resize 请求与 renderer surface 重配置的低风险处理顺序。
- 来源列表：Winit `WindowEvent::Resized` 与 `RedrawRequested` 文档 <https://docs.rs/winit/latest/winit/event/enum.WindowEvent.html>；Slint issue #12730 <https://github.com/slint-ui/slint/issues/12730>；Slint PR #12733 <https://github.com/slint-ui/slint/pull/12733>；Slint PR #12490 <https://github.com/slint-ui/slint/pull/12490>；WezTerm resize 路径 <https://github.com/wezterm/wezterm/blob/main/wezterm-gui/src/termwindow/resize.rs>；Kitty `pause_resize_notifications_to_child` <https://github.com/kovidgoyal/kitty/blob/master/kitty/window.py>；Alacritty display resize <https://github.com/alacritty/alacritty/blob/master/alacritty/src/display/mod.rs>；Alacritty issue #7898 <https://github.com/alacritty/alacritty/issues/7898>；Slint issue #6259 <https://github.com/slint-ui/slint/issues/6259>。
- 关键结论：窗口 resize 事件会连续到达，Winit 的 redraw 合并不会自动合并应用层 PTY/model resize；成熟终端保存最新尺寸并只在行列实际变化时提交。Slint 的 surface hold/150ms settle 方案仍未形成已合并、跨 renderer 的稳定 API，相关 PR 已关闭或未解决全部 lag，因此先修正 AxSSH 自身的 full workspace refresh 和重复 worker resize。
- 对实施计划的影响：RZ2 将 resize 成功后的刷新改成当前 terminal-only 请求；RZ3/RZ4 在模型与 worker watch/pending 层去重相同行列。平台原生 live-resize 状态、暂停 PTY reflow 和 renderer surface hold 暂不引入，待同负载测量后再决定。
- 未解决问题：Alacritty 的 macOS compositor/vsync 闪烁问题仍开放；目标 macOS 需要人工确认 Software/GPU renderer 下连续拖动、IME、选区、焦点和真实 PTY NAWS/窗口变更表现。
## 2026-08-23 终端按区更新与 macOS software present

- 时间：2026-08-24 12:10 +0800
- 检索问题：Slint software renderer 的终端内容是否适合按固定行区分组更新，应用层 tile 优化能否避免 macOS 最终整帧 surface 提交？
- 检索原因：现有终端已经做了可见行级增量 model 更新，但用户仍观察到 dirty/repaint 时 CPU 较高，需要区分 Slint item/model/glyph 成本与平台 surface present 成本。
- 来源列表：Slint issue #7432 <https://github.com/slint-ui/slint/issues/7432>、#8737 <https://github.com/slint-ui/slint/issues/8737>、#10370 <https://github.com/slint-ui/slint/issues/10370>、#12173 <https://github.com/slint-ui/slint/issues/12173>、#12752 <https://github.com/slint-ui/slint/issues/12752>；Slint PR #12758 <https://github.com/slint-ui/slint/pull/12758>；softbuffer PR #99 <https://github.com/rust-windowing/softbuffer/pull/99>；锁定依赖 `softbuffer 0.4.8` macOS CoreGraphics backend。
- 关键结论：固定 8/16 行 tile 可以减少顶层重复项树遍历、model/property 通知和静态行文字绘制的重复成本；但锁定的 macOS softbuffer backend 报告 `age=0`，`present_with_damage` 忽略 damage 并设置完整尺寸 `CGImage`，所以 tile 不能单独解决最终整帧提交。不能只把 buffer age 改为 1，因为当前 buffer 没有持久 framebuffer，可能产生脏数据。
- 对实施计划的影响：固定 8/16 行 tile 方案已撤回；当前只保留单层 `TerminalRenderLine` model 的 source/render revision 复用、上游 `TermDamage` 和 Slint 自身 dirty-region 追踪。macOS surface 仍按上游完整单 layer present，不把应用层行更新当作 framebuffer partial present。
- 未解决问题：同负载 GPU/Skia 与 Software 的 CPU/footprint A/B 仍需目标平台采样；若热点仍集中在 `WinitSoftwareRenderer`/CoreAnimation，应单独评估 renderer/backend，而不是重新引入应用层 tile。

## 2026-08-26 Core Animation 局部失效与 sRGB 实验

- 时间：2026-08-26 14:00 +0800
- 检索问题：`CALayer` 的 `setNeedsDisplayInRect` 是否能把当前每 tile `setContents(CGImage)` 路径变成真正的局部 backing-store 更新，以及显式 sRGB 是否可能减少 Core Animation 的 ICC/vImage 转换。
- 检索原因：用户采样显示 `present_tiles -> CATransaction::commit -> CA::Render::copy_image` 和 `CGColorTransformConvertUsingCMSConverter` 为主要主线程热点，需要先拆分色彩空间因素，再评估 delegate/backing-store 方案。
- 来源列表：Apple `CALayer.draw(in:)` <https://developer.apple.com/documentation/quartzcore/calayer/draw%28in%3A%29>；Apple `CALayerDelegate` <https://developer.apple.com/documentation/QuartzCore/CALayerDelegate>；Apple `CALayer.setNeedsDisplay`（含 `setNeedsDisplayInRect:`）<https://developer.apple.com/documentation/quartzcore/setneedsdisplay>；Apple `CALayer.display()` <https://developer.apple.com/documentation/quartzcore/calayer/display%28%29>；Apple `IOSurface` <https://developer.apple.com/documentation/IOSurface>；本机 `objc2-quartz-core 0.3.2` 和 `objc2-core-graphics 0.3.2` 生成 API。
- 关键结论：公开 API 支持对 layer 的矩形区域失效，并在 delegate 的 `drawLayer:inContext:` 中使用被裁剪的 CGContext；但 `setContents(CGImage)` 没有图片内部的 patch 语义，直接写 provider 后备内存不安全。持久 backing-store 可能减少应用侧每 tile image 创建，但不保证 Core Animation 内部完全不复制；必须用 delegate 生命周期、不可变帧快照和目标机 sample 验证。显式 `kCGColorSpaceSRGB` 可作为独立低风险 A/B，不能预先假设会消除 ICC 转换。
- 对实施计划的影响：新增 COLOR1（只切换 `DeviceRGB` -> `kCGColorSpaceSRGB`）和 COLOR2（同负载 release sample）；COLOR3 的 `setNeedsDisplayInRect` 原型仅在 COLOR2 之后实施，保持当前 tile 路径可回退。
- 未解决问题：macOS 目标 layer 的实际色彩空间、`CA::Render::copy_image` 是否因显示器 profile 或 pixel format 触发、delegate backing-store 的异步读取/撕裂风险，以及 IOSurface 在 CPU-only softbuffer 路径中的公开可移植性仍需目标机实验。

## 2026-08-26 sRGB release sample 复核

- 时间：2026-08-26 14:32 +0800
- 检索问题：显式 sRGB release 候选是否减少了 `CA::Render::copy_image` 下的 ICC/vImage 驻留。
- 检索原因：需要用同一二进制 sample 决定是否继续实施普通 `CALayer` backing store。
- 来源列表：用户提供的 Instruments sample、当前 `target/release/ax_ssh` Mach-O UUID/SHA-256、锁定的 macOS softbuffer 提交路径。
- 样本：`/Volumes/albert_xin/2026/Download/Sample of ax_ssh-srgb-10s.txt`，macOS 15.7.7 ARM64，PID 66769，10 秒/1 ms；UUID `75D434E2-B1D0-3C6A-ABFB-7504F41A0663` 与当前 `target/release/ax_ssh` 一致，当前 SHA-256 为 `f838a4c3b9893f93b143091622f7e212e534d1438c52b7bab1347eb9addaacd4`。
- 主线程 8317 个采样中，有 2933 个驻留在 DisplayLink 回调栈；其中 1666 个驻留在 `CA::Transaction::commit`，1635 个驻留在 `CA::Render::copy_image`，1613 个驻留在 `CGContextDrawImage`，1087 个驻留在 `CGColorTransformConvertUsingCMSConverter`，984 个驻留在 `vImageConverterConvert`。这些是调用树驻留次数，不是实际回调次数或独立 CPU 百分比，且子项有嵌套关系。
- 关键结论：显式 sRGB 没有消除 Core Animation 的 ICC/vImage 路径；当前 `setContents(CGImage)` 仍让 CA 在提交时准备/复制图像。应用侧 damage 只决定哪些 layer 被替换，尚未形成 CGImage 内部的矩形 patch。worker/PTY 线程主要停在 `read`、信号量或条件变量等待，不是本次热点。
- 对实施计划的影响：COLOR2 完成；COLOR3 需要单变量原型验证，并保留安全 snapshot、残影/撕裂和目标机 CPU 验收。
- 未解决问题：sample 没有独立 CPU meter，且不能从嵌套驻留次数推导总 CPU 降幅；CA 内部格式选择仍不公开。

## 2026-08-26 macOS dirty backing-store 实施检索

- 时间：2026-08-26 15:00 +0800
- 检索问题：`CALayer.setNeedsDisplayInRect`、普通 delegate 和 `CATiledLayer` 中，哪条 CPU-only 路径适合 AxSSH 当前 pane-aware softbuffer layer。
- 检索原因：sRGB sample 仍由 `CA::Render::copy_image`、ICC/vImage 主导，需要避免把失效标记误认为公开保证的局部提交。
- 来源列表：Apple `CALayer`、`CALayerDelegate`、`drawLayer:inContext:`、`drawInContext:`、`setNeedsDisplay`、`contentsFormat`、`CATiledLayer`、`CGDataProvider` 和 Quartz 2D bitmap image 文档；锁定 objc2 0.3.2 生成源码与当前 vendor backend。
- 需求定义：CPU-only software renderer 已经把 Slint damage 映射到窗口内的 presentation layer，但当前每个 dirty layer 仍通过 `setContents(CGImage)` 提交，目标是减少应用侧整块 image 构造并观察是否降低 `CA::Render::copy_image`/ICC/vImage；不能牺牲终端顺序、Retina 坐标、窗口关闭安全和帧一致性。
- 官方 API 事实：`CALayer.setNeedsDisplayInRect:` 只标记指定区域失效；layer 有 delegate 时，Core Animation 可通过 `drawLayer:inContext:` 请求绘制，传入的 CGContext 可能已经按需要绘制的区域裁剪，实际 clip 可用 `CGContextGetClipBoundingBox` 查询。`contentsFormat` 只是期望存储格式提示，默认值为 `RGBA8Uint`，不能当作局部 patch API。来源：Apple `CALayer`、`CALayerDelegate`、`drawInContext:`、`setNeedsDisplay` 和 `contentsFormat` 文档。
- `CATiledLayer` 事实：官方说明它会在需要时于一个或多个后台线程调用绘制回调，并允许用 `setNeedsDisplayInRect:` 使区域失效；更新是异步的，下一次显示可能仍是旧内容。官方还明确禁止直接修改 `CATiledLayer.contents`，否则会失去 tiled behavior。来源：Apple `CATiledLayer` 文档。
- ownership 事实：`CGDataProvider` 的 direct-access pointer 只向 Core Graphics 提供只读访问；应用不能在 Core Graphics 释放 pointer 前移动或修改数据。因此不能把当前可复用的 `softbuffer::PixelBuffer` 直接作为长期 provider 后备并继续写入；当前独立 `Vec<u32>` provider 的所有权边界是正确的。
- 候选方案比较：
  - `CALayer` delegate backing store：与当前主线程 layer 生命周期最接近；保留一个 layer backing store，由 `setNeedsDisplayInRect` 合并脏矩形，`drawLayer:inContext:` 只从不可变 snapshot 绘制 clip 区域。优点是失效语义清晰、可保留完整帧；缺点是 Core Animation 仍可能在提交时复制/色彩转换，公开 API 没有保证零 copy。
  - `CATiledLayer`：由 Core Animation 管理 tile cache，适合大 surface/异步绘制；需要设置固定 `tileSize`、`levelsOfDetail=1`、`fadeDuration=0`（若 API 可用），并接受至少一帧的异步可见延迟。不能同时调用 `setContents`，也不能把最新 framebuffer 当作可变共享内存直接读写。
  - 继续 `setContents(CGImage)`：实现最小、当前视觉语义稳定，但 `damage` 只能减少替换 layer 数量，不能阻止每张替换图像在 CA 内部被准备/转换；本次 sRGB sample 已验证该限制。
- 推荐实施顺序：
  1. 保留当前路径作为基线，先单独测试 `contentsFormat`/像素排列变量并重新 sample。
  2. 实现普通 `CALayer` delegate 原型：固定 layer geometry；由 `CGImpl` 保留两个有界 frame snapshot（front/back），提交前完成 framebuffer -> back snapshot 的脏矩形复制，交换后只在主线程调用 `setNeedsDisplayInRect`；delegate 只读取 front snapshot，使用 CGContext 的 clip 绘制。
  3. 处理 resize、scale、invalidate、空 damage、窗口关闭和 delegate weak lifetime；在任何 snapshot 正在被 CA 使用时禁止写入，必要时丢弃/合并过期 frame。
  4. 用同负载 sample 对比 `CA::Render::copy_image`、ICC/vImage、主线程驻留和外部 CPU meter；只有确认收益后再考虑 `CATiledLayer` 独立异步实验。
- 证据缺口：Apple 文档没有承诺 `setNeedsDisplayInRect` 一定避免 backing-store 全量复制，也没有公开 CA 内部 `copy_image` 的格式选择规则；因此“减少应用侧 copy”是可合理验证的目标，“消除 CA copy/ICC”只能作为待测假设。当前 COLOR3 原型已为 objc2 0.3.2 启用普通 `CALayerDelegate` 所需的 `CGContext` 接线；`CATiledLayer` 和 `CGBitmapContext` feature 仍未启用，避免在同一次 A/B 中引入异步 tile 或另一种像素所有权模型。
- 关键结论：普通 `CALayer` delegate 与当前 layer 生命周期最接近，可以保留同步、低延迟和明确的 weak delegate 所有权；`CATiledLayer` 是异步 cache，不能直接沿用 `setContents`，不适合作为首个终端实验。
- 对实施计划的影响：先用环境变量实现普通 delegate 原型并保留默认基线；只有目标机视觉一致且 sample 有收益时才考虑默认启用，`CATiledLayer` 后置。
- 未解决问题：Apple 不保证局部失效会消除内部 backing-store copy/颜色转换；目标 macOS 的实际 clip、方向、Retina、残影和性能必须实测。

## 2026-08-26 macOS dirty backing-store 光标错位复核

- 时间：2026-08-26 18:06 +0800
- 检索问题：为什么实验 backing-store 连续输出时约每 4 行出现一次白色光标错位，以及 `setNeedsDisplayInRect` 与 delegate clip 应使用哪套 tile-local Y 坐标。
- 检索原因：目标机截图中的错位间隔与默认 4 行 presentation block 完全一致；默认 `setContents(CGImage)` 不出现该问题，需要区分 Slint cursor damage、layer 排序和局部坐标转换。
- 来源列表：[Apple Core Animation Basics](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/CoreAnimationBasics/CoreAnimationBasics.html)、[Apple CALayer setNeedsDisplay](https://developer.apple.com/documentation/quartzcore/calayer/setneedsdisplay(_:))、[Apple CALayer geometryFlipped](https://developer.apple.com/documentation/quartzcore/calayer/isgeometryflipped)、当前 `vendor/softbuffer/src/backends/cg.rs` 与用户提供的目标机截图。
- 关键结论：Slint/softbuffer damage 和 CPU framebuffer 使用左上原点物理像素，而 macOS standalone `CALayer` 自身坐标默认使用左下原点。原型只除以 Retina scale，未在 `setNeedsDisplayInRect` 和 delegate clip 读取之间翻转 tile-local Y，因此完整 block 首帧正常，局部 cursor damage 在 block 内上下镜像；`geometryFlipped` 不改变 layer 内容渲染，不能代替本次局部矩形转换。
- 对实施计划的影响：只在实验 delegate 边界双向转换 tile-local Y；完整矩形、底边、顶部和 Retina clip 往返由 focused tests 覆盖。默认 `setContents`、pane/fallback layer 顺序、Slint dirty region、终端模型和 SSH 边界不变；目标机重新验收连续输出、光标 blink、1/4/8/16 行、跨 block、Retina/resize 和分屏后，才进行 CPU/sample A/B。
- 未解决问题：静态坐标往返已验证，但目标机 Core Animation 实际 clip 合并、连续 blink 残影和性能收益仍需用户视觉复验与同负载 sample。
## 2026-09-17 SFTP 原生拖放目标路由

- 时间：2026-09-17 00:00 +0800
- 检索问题：当前外部文件拖入按活动 SFTP 目录上传、macOS promise 回拖在整个窗口接收，如何与标准拖放的目标协商语义对齐。
- 检索原因：用户要求删除旧的目录猜测上报并按标准接口处理 SFTP 上传、下载拖动，以避免跨平台和跨窗口的不兼容行为。
- 来源列表：[Winit 0.30.13 WindowEvent](https://docs.rs/winit/0.30.13/winit/event/enum.WindowEvent.html)；[Slint DragArea](https://docs.slint.dev/latest/docs/slint/reference/drag-and-drop/dragarea/)；[Slint DragAction](https://docs.slint.dev/latest/docs/rust/slint/language/enum.DragAction)；[Apple Drag Destinations](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/DragandDrop/Concepts/dragdestination.html)；[Apple draggingLocation](https://developer.apple.com/documentation/appkit/nsdragginginfo/dragginglocation)；[Apple file promise destination](https://developer.apple.com/documentation/appkit/nsfilepromiseproviderdelegate/filepromiseprovider%28_%3Awritepromiseto%3Acompletionhandler%3A%29?changes=l_3&language=objc)。
- 关键结论：标准流程由实际 drop target 在拖入/悬停时协商 copy 或拒绝，并在释放位置生成意图；Winit 的 `DroppedFile(PathBuf)` 没有坐标，必须使用同一外部文件悬停期间最新的 `CursorMoved` 坐标，缺失时不能猜测。Slint `DropArea` 已对进程内拖放提供目标局部位置和 action 协商。AppKit destination 应只覆盖可接受区域并在 enter/update/perform 都拒绝区域外的操作。
- 对实施计划的影响：UI 声明 Remote/Local files 的纯命中与几何契约。Winit bridge 只在有效外部 hover 坐标命中 Remote files 时调用现有上传 queue；macOS promise destination 的原生子视图仅附着到启动时的 Local files 矩形，区域外的回拖由 AppKit 拒绝。既有有界 intent、worker 和同名文件拒绝策略不变。
- 未解决问题：Winit 无法向所有平台原生文件拖入提供 Slint `DropArea` 同等的 hover 光标反馈；需要用户在目标平台手工确认 Remote/Local 区域、分离窗口、窗口缩放和 Finder 回拖的视觉与交互反馈。

## 2026-10-02 备用屏幕滚轮与复制光标行首

- 时间：2026-10-02 22:59 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `f5dda6afb0f9`；仅表示记录入库时间，原事件具体时分未记录。

- 原始时间：2026-10-02 +0800
- 检索问题：保持通用终端行为时，能否在滚轮移动复制光标后固定行首？
- 检索原因：用户确认滚轮修复可用，进一步要求避免光标在不同行尾左右跳动。
- 来源列表：[xterm alternateScroll](https://www.invisible-island.net/xterm/manpage/xterm.html)；[tmux start-of-line](https://man.openbsd.org/tmux#start-of-line)；本机 tmux 3.6a 隔离 socket 默认配置实验。
- 关键结论：alternate-scroll 发送上下方向键，未规定额外 Home；光标列由远端程序决定。本机复制模式在 Home 前坐标为 49,5，Home 后为 0,5，之后 Up/Up/Down 分别为 0,4 / 0,3 / 0,4。AxSSH 已支持普通/application-cursor Home 序列。
- 对实施计划的影响：保持既有标准输入可先按一次 Home；用户进一步澄清为“滚轮时一直在行首”，因此启用策略必须对每个方向键发送 `Home + Up/Down`，不能在整批方向键结束后才发送 Home。不识别 tmux 名称，也不改绘制层光标。
- 未解决问题：实际远端自定义 key binding 可能不同；用户已于 2026-10-02 选择默认关闭的通用逐步行首选项，实施见当前计划。

## 2026-10-02 滚轮移动后的目标行首复核

- 时间：2026-10-02 22:59 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `f5dda6afb0f9`；仅表示记录入库时间，原事件具体时分未记录。

- 原始时间：2026-10-02 +0800
- 检索问题：怎样让每一步行首动作作用于上下移动后的目标行？
- 检索原因：用户要求修正滚轮行首行为；前置 Home 作用于原行，不能仅凭发送字节断言目标行位置。
- 来源列表：沿用前轮核查的 [xterm Wheel mice](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Wheel-mice)；本机 tmux 隔离 socket、空配置和 emacs copy-mode 的坐标实验；`src/app/terminal_bridge.rs`。
- 关键结论：每步使用 `Up/Down + Home` 请求目标行首。不同长度行和空行的本机 copy-mode 实验从 14,4 开始，上移三步为 0,3 / 0,2 / 0,1，下移三步为 0,2 / 0,3 / 0,4；每一步均为第 0 列。测试 server 已清理，无用户 tmux 配置修改。
- 对实施计划的影响：本条替代此前前置 Home 的实施结论；输入策略按完整组合逐步重复，不保留旧顺序分支，不覆盖本地绘制光标。设置保留默认关闭，本轮无新增联网检索。
- 未解决问题：实际远端自定义按键绑定或应用模式可能改变 Home 含义；GUI 和目标程序行为仍需用户验收，不能承诺所有程序效果相同。

## 2026-10-09 系统图标官网与社区来源补查

- 时间：2026-10-09 08:56 +0800
- 检索问题：Iconify 未覆盖的发行版和非发行版系统，能否取得无文字图标及上游原生配色变体？
- 检索原因：用户允许全网搜索并扩展到 BSD、NAS、路由、虚拟化等系统/平台。
- 来源列表：[openKylin 品牌资源](https://www.openkylin.top/about/brand-cn.html)、[Bazzite 官方仓库](https://github.com/ublue-os/bazzite)、[Ubuntu Studio 品牌资源](https://ubuntustudio.org/wiki/content/ubuntu-studio/UbuntuStudio/Artwork/)、[DietPi 原始图片目录](https://dietpi.com/images/)、[IPFire 品牌页](https://www.ipfire.org/docs/press/branding)、[Volumio 媒体包](https://volumio.com/media-kit/)、[CNCF artwork](https://github.com/cncf/artwork)、[Papirus](https://github.com/PapirusDevelopmentTeam/papirus-icon-theme)、[Font Logos](https://github.com/lukas-w/font-logos)。逐项原始链接和版本将保存在 `assets/os-icons/manifest.json`。
- 关键结论：可补齐此前多项遗漏；官网部分只提供 PNG 或带完整名称的标志。Papirus/Font Logos 可提供更多社区系统符号，但不代表官方或系统仍维护。品牌资源不能沿用 Iconify 集合许可证；原生配色名称和主题含义也必须分别记录。
- 对实施计划的影响：扩展到 SVG/PNG，保留原文件及哈希，按系统/平台/桌面环境分类；单列官方品牌条款与社区许可；新增检索覆盖和缺口资料，不裁字或自行配色。
- 未解决问题：Amazon Linux 与 RancherOS 暂未确认专用系统图形，通用 AWS/Rancher 品牌不得标成专用系统标志；最终选择和检查见资源清单。

## 2026-10-10 SSH Tab 延迟探测

- 时间：2026-10-10 14:42 +0800
- 检索问题：如何使用锁定 russh 0.64.1 的已认证连接连续测量 SSH 响应时间，且不污染终端或堆积无响应请求？
- 检索原因：用户要求评估 Tab 状态点按延迟配色的检测方式，随后授权按建议实施。
- 来源列表：[固定版本 client Handle](https://github.com/warp-tech/russh/blob/e5d80a07dc554480b6ea3b2e14adfc190407b627/russh/src/client/mod.rs)、[固定版本 global request 入队](https://github.com/warp-tech/russh/blob/e5d80a07dc554480b6ea3b2e14adfc190407b627/russh/src/client/session.rs)、[RFC4254 全局请求](https://www.rfc-editor.org/rfc/rfc4254.html#section-4)；本机 Cargo.lock 和对应依赖缓存源码。
- 关键结论：`keepalive@openssh.com` 配合 want-reply 可使用现有 SSH transport；REQUEST_FAILURE 也是有效往返响应，未知请求须按协议拒绝；全局响应按顺序匹配，没有请求 ID。固定版本 send_ping 忽略 oneshot 关闭，因此直接使用 send_global_request 区分拒绝与断线。既有保活仅在无入站数据时运行，不能独立提供持续 RTT。
- 对实施计划的影响：RT2 使用拥有单个 pending future 的 worker 内 sampler，展示超时后保留该请求；成功或拒绝返回测量，断线错误停止采样。RT3 只保存临时延迟，复用代次 guard 和有界 Tab 刷新，不引入 ICMP、新连接或远端 exec。
- 未解决问题：数值包含 SSH 队列、传输和服务端处理时间；真实高延迟、不同服务端、前后台/重连与各平台视觉仍需用户原生验收。

## 2026-10-10 终端语义高亮的标准分类与匹配实现

- 时间：2026-10-10 23:53 +0800
- 检索问题：哪些成熟匹配实现和有出处的等级/HTTP 规则可以替代手写词表及三位数字扫描？
- 检索原因：用户要求按标准化方案替换旧识别器，需要区分协议/等级定义与产品显示规则。
- 来源列表：[regex 1.13.1 API](https://docs.rs/regex/1.13.1/regex/)、[iTerm2 Highlight Text](https://iterm2.com/documentation-triggers.html)、[OpenTelemetry 日志等级](https://opentelemetry.io/docs/specs/otel/logs/data-model/#severity-fields)、[RFC 5424 等级表](https://www.rfc-editor.org/rfc/rfc5424#section-6.2.1)、[RFC 9110 状态码类别](https://www.rfc-editor.org/rfc/rfc9110.html#section-15)、[RFC 9112 状态行](https://www.rfc-editor.org/rfc/rfc9112.html#section-4)、[OpenTelemetry HTTP 属性](https://opentelemetry.io/docs/specs/semconv/registry/attributes/http/)、[Apache Common/Combined Log Format](https://httpd.apache.org/docs/2.4/logs.html#common)。
- 关键结论：regex 返回字节区间，可预编译复用；RegexSet 仅返回命中的规则编号，不能独自提供着色区间。OpenTelemetry 定义 TRACE/DEBUG/INFO/WARN/ERROR/FATAL；syslog 等级表补充 Emergency/Alert/Critical/Warning/Notice/Informational。HTTP 合法类别为 1xx–5xx，普通三位数不构成 HTTP 上下文；状态行、访问日志请求字段后的码以及明确 HTTP 状态属性提供上下文。不存在由这些标准规定的统一终端配色或普通状态词词典。
- 对实施计划的影响：HL1 以预编译 regex 的命名捕获区间替代旧关键词/短语/数字扫描和逐字节标记；等级映射为现有 Information/Warning/Error，HTTP 1xx/3xx 为 Information、2xx 为 Success、4xx 为 Warning、5xx 为 Error。这些显示颜色映射为 AxSSH 产品选择，非协议要求。HTTP/2、HTTP/3 的文本状态摘要作为显示约定，不宣称其线上帧是 HTTP/1 状态行。
- 未解决问题：不跨样式段或物理行拼接，非 ASCII 字词不在此层识别；真实 GUI 效果需用户验收，不引入用户规则执行动作。


## 2026-10-11 URL 目标识别与中文行文边界

- 时间：2026-10-11 08:22 +0800
- 检索问题：如何标准化 URL 候选识别与语法校验，并隔离终端行文标点？
- 检索原因：用户原文 `https://www.rfc-editor.org/rfc/rfc9110.html#section-15)。HTTP` 被旧 parser 作为完整 URL 返回。
- 来源列表：[Alacritty 终端 hints](https://alacritty.org/config-alacritty.html#hints)、[rust-url 2.5.8](https://docs.rs/url/2.5.8/url/)、[Url::parse](https://docs.rs/url/2.5.8/url/struct.Url.html#method.parse)。
- 关键结论：成熟终端将 regex 候选识别、启发式尾部后处理和显式 OSC 8 metadata 分开；rust-url 实现 WHATWG URL 标准，提供主机/端口/IPv6 校验。合法 URL 语法本身不能判定行文中的中文句号或引号是否属于链接，因此明确中文行文分隔是 AxSSH 提取规则，非 URL 标准的禁用字符声明。
- 对实施计划的影响：HL6 预编译 HTTP(S) matcher，再处理平衡括号/尾随标点并使用 Url::parse 验证；保留原始文本及字节到字符/cell 区间，保留 Unicode 路径、IPv6、查询与片段，不修改 OSC 8 提供的 URI。
- 未解决问题：包含中文行文标点的字面 URL 需要 percent encoding 才能避免与 prose 边界冲突；GUI 双击/悬停效果仍由用户验收。

## 2026-10-11 终端 agent 的剪贴板与麦克风链路

- 时间：2026-10-11 10:23 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `94f6a10eb536`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：文本/图片剪贴板、SSH 远端麦克风和 Codex CLI 的标准能力如何接合？
- 检索原因：用户要求核对标准实现，原有协议尺寸/授权有缺口，音频与图片未实现。
- 来源列表：[xterm OSC 52](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)、[arboard](https://docs.rs/arboard/3.6.1/arboard/struct.Clipboard.html)、[RFC 4254](https://www.rfc-editor.org/rfc/rfc4254.html)、[CPAL 0.17.3](https://docs.rs/crate/cpal/0.17.3/source/README.md)、[PipeWire pipe source](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_pipe_source.html)、[Apple 麦克风用途](https://developer.apple.com/documentation/bundleresources/information-property-list/nsmicrophoneusagedescription)、[whisper.cpp CLI](https://github.com/ggml-org/whisper.cpp/tree/master/examples/cli)、[Codex configuration](https://learn.chatgpt.com/docs/config-file/config-reference)。
- 关键结论：OSC 52 只承载文本；arboard 错误可报告且 Linux owner 要保持存活；独立 SSH session 可承载 PCM 至用户级 FIFO 音源；CPAL 0.18 提高 macOS 要求因此固定 0.17.3；/voice 属上游实验功能。
- 对实施计划的影响：CM1–CM7；使用私有 SFTP 附件和明确启停的独立音频通道，不引入第二 SSH/UI 框架。
- 未解决问题：真机系统权限、远端音频服务及各 CLI 对文件附件/音源的选择须 CM7 验收。

## 2026-10-11 SFTP 外部编辑流程

- 时间：2026-10-11 17:58 +0800
- 检索问题：主流 SFTP 客户端如何选编辑器、检测保存、回传及处理文件生命周期。
- 检索原因：用户要求检索后采用系统关联程序，并完全移除内置编辑器。
- 来源列表：[WinSCP 编辑](https://winscp.net/eng/docs/task_edit)、[WinSCP 冲突检查](https://winscp.net/eng/docs/ui_pref_editor)、[FileZilla](https://filezillapro.com/docs/v3/advanced/how-to-set-text-editor-in-three-different-ways/)、[Cyberduck](https://docs.cyberduck.io/cyberduck/edit/)、[Windows ShellExecute](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew)。
- 关键结论：内置/外部没有统一强制要求；外部模式为本地临时副本、关联/指定程序、保存检测、后台回传。FileZilla 可提示上传，Cyberduck 保存上传。WinSCP 不把复用进程的启动器退出当作文件关闭，并提供远端时间戳冲突检查。
- 对实施计划的影响：采用用户要求的系统文件关联打开；删除内置文本 API；监听目录以兼容原子替换；自动上传可关闭；冲突/失败保留副本，迟到结果不得改写其他编辑上下文。
- 未解决问题：系统文件关联与权限需目标平台人工验收；size/mtime 无法提供服务器端锁定或精确内容冲突检测。

## 2026-10-11 23:21 +0800 原会话精确中断接口

- 时间：2026-10-11 23:21 +0800
- 时间依据：原始标题已明确记录的时间；本轮只补机器字段。

- 检索问题：如何只中断指定原会话的执行，并确认接手后不再同时修改项目。
- 检索原因：用户明确授权中断原会话；本机控制 socket 的原始 JSONL 请求未成功，需要确认其真实传输协议。
- 来源列表：[OpenAI Codex App Server](https://learn.chatgpt.com/docs/app-server)；本机已安装 Codex 的 CLI help 与生成的 TurnInterruptParams/ThreadReadParams schema。
- 关键结论：Unix socket 使用 WebSocket HTTP Upgrade；初始化后可按 threadId 和 turnId 发送 turn/interrupt，thread/read 的 runtime status 可确认是否 idle。实际中断请求成功，原会话由 active 变为 idle；其遗留本项目构建进程另行按已核实的 PID 和 cwd 停止。
- 对实施计划的影响：WI2 的最终验证由本会话独立执行，保留既有已落盘源码。工程实现本身不依赖外部资料，不改变依赖或平台接口版本。
- 未解决问题：无；用户视觉与 Windows/Linux 原生验收仍按 WI3 保留。

## 2026-10-12 标准系统录音请求与按需 SSH 音频

- 时间：2026-10-12 08:42 +0800
- 时间依据：本文件首次收录该标题的 Git 提交 `f359a9713a6d`；仅表示记录入库时间，原事件具体时分未记录。

- 检索问题：如何让任意使用系统音频接口的 CLI 触发采集，避免匹配某个 CLI 的命令或显示文本。
- 检索原因：用户明确要求 CLI 自动调用及标准化，现有手动媒体行和连续采集不符合该要求。
- 来源列表：[PulseAudio FAQ](https://wiki.freedesktop.org/www/Software/PulseAudio/FAQ/)；[PulseAudio stream/source 定义](https://github.com/pulseaudio/pulseaudio/blob/master/src/pulse/def.h)；[pactl 官方实现](https://github.com/pulseaudio/pulseaudio/blob/master/src/utils/pactl.c)；[PipeWire Pipe Source](https://pipewire.pages.freedesktop.org/pipewire/page_pulse_module_pipe_source.html)。
- 关键结论：Pulse 客户端使用 PULSE_SOURCE 或默认系统音源；录音流有 source 与 corked 状态；pipe-source 支持固定 PCM/FIFO。以所属音源的非暂停录音流为需求边界，避免用整个设备 RUNNING 状态推断用户录音。音频接口能力不等于某个 CLI 已实现转写。
- 对实施计划的影响：Slint 移除媒体手动入口；本地 CLI 使用原生 OS 输入，Linux SSH 会话准备标准虚拟音源并监听录音需求，只有请求期间采集，有界 PCM 与可观察清理继续由 worker 所有。
- 未解决问题：真实 OS 录音权限、远端 Pulse/PipeWire、ALSA default 路由和多平台设备验收需用户环境；直接使用硬件设备或不支持音频接口的 CLI 不属于虚拟音源能力。

## 2026-10-12 内嵌多语言与类似工具标准

- 时间：2026-10-12 14:14 +0800
- 检索问题：如何新增界面语言而尽量不增加运行时加载、解析或常驻任务？类似桌面工具如何分发翻译？
- 检索原因：用户明确要求低运行负担并对照同类工具，再授权实施。
- 来源列表：[Slint 翻译指南](https://docs.slint.dev/latest/docs/slint/guide/development/translations/)；[Slint 语言选择](https://docs.slint.dev/latest/docs/rust/slint/fn.select_bundled_translation)；锁定 Slint 1.18.1 的 compiler/core translations.rs；[GNOME Console Gettext](https://github.com/GNOME/console/blob/main/po/meson.build)；[Qt 翻译](https://doc.qt.io/qt-6/i18n-source-translation.html)；[VS Code language packs](https://code.visualstudio.com/docs/configure/locales)。
- 关键结论：Slint 官方内嵌在构建期解析 PO 并生成静态数组，运行时按 language index 读取；GNOME 使用 Gettext 目录，Qt 加载 QTranslator，VS Code 安装语言扩展。无可直接比较的统一性能 benchmark，不声称零 RSS 或跨工具速度排名。
- 对实施计划的影响：沿用 bundled translations、不启用 Gettext feature；PO 译文统一来源，维护期生成 search arrays；新增 ja，保留事务/locale 同步与静态 UI 边界，并测量 binary 增量。
- 未解决问题：用户的日语用词、实际字体/窄窗口和 GUI 焦点验收；Windows/Linux 原生 CI；二进制分段实际驻留内存没有测量。

## 2026-10-12 22:15 +0800 LLVM coverage 本机复现

- 时间：2026-10-12 22:15 +0800
- 检索问题：如何复现 CI 的 LLVM/LCOV 测量并生成同平台前后报告？
- 检索原因：本机没有 cargo-llvm-cov，测量工具及独立 target/cache 管理影响验证路径。
- 来源列表：[cargo-llvm-cov 官方项目](https://github.com/taiki-e/cargo-llvm-cov)。
- 关键结论：使用 cargo install cargo-llvm-cov --locked；工具依赖匹配 Rust 的 llvm-tools-preview；支持 LCOV/JSON/report 和独立 coverage target directory；no-run/report 时须防止旧 profraw 混入。
- 对实施计划的影响：补齐工具，先测未修改源码基线，最终清理 profile 后同命令重测，输出至 target，不排除业务源码来提高比例。
- 未解决问题：本机与 Linux Codecov 平台不同；最终 Linux 指标仍需原生 CI。

## 2026-10-13 14:36 +0800 macOS 文件承诺事件与回调契约

- 时间：2026-10-13 14:36 +0800
- 检索问题：Slint 延后 moved 回调能否使用 AppKit currentEvent 启动拖拽，文件承诺失败后如何收尾？
- 检索原因：用户授权修复 SFTP 拖出偶发失败，原生事件和 completion 契约影响实现。
- 来源列表：[Apple beginDraggingSession](https://developer.apple.com/documentation/appkit/nsview/begindraggingsession%28with%3Aevent%3Asource%3A%29)；[Apple currentEvent](https://developer.apple.com/documentation/appkit/nsapplication/currentevent)；[Apple file promise completion](https://developer.apple.com/documentation/appkit/nsfilepromiseproviderdelegate/filepromiseprovider%28_%3Awritepromiseto%3Acompletionhandler%3A%29)；锁定 Slint 1.18.1 的 `vendor/i-slint-backend-winit/{winitwindowadapter,event_loop}.rs` 和本机 Winit 0.30.13。
- 关键结论：AppKit drag 使用起始 mouse-down，currentEvent 只指最近事件；Slint 会先补发 moved 再处理 release，因此后读 currentEvent 不可靠。写 promise 后须调用 completion；断线/重复/非法目标应保留一次性 owner，不可静默丢弃回调。
- 对实施计划的影响：原生 press hook 保存事件；启动按窗口和实际按键校验；promise 绑定 SSH attempt 并由 monitor 生命周期清理，错误映射到 SFTP 状态。
- 未解决问题：实际 Finder 操作和用户的目标目录/文件系统尚未人工验证；无截图或真实远端连接验证。

## 2026-10-14 09:23 +0800 Slint 平台文本编辑边界

- 时间：2026-10-14 09:23 +0800
- 检索问题：如何补充 macOS Control 编辑并让多种输入框复用，且保持其他平台、Unicode、IME、撤销和密码保护？
- 检索原因：用户授权实现通用编辑；Slint 公开选区接口不足以安全重建完整编辑行为，平台修饰键含义影响实现。
- 来源列表：[Slint 键盘与平台修饰键](https://docs.slint.dev/latest/docs/slint/reference/keyboard-input/overview/)；[Slint TextInput API](https://docs.slint.dev/latest/docs/slint/reference/keyboard-input/textinput/)；[锁定 1.18.1 标准编辑映射](https://github.com/slint-ui/slint/blob/v1.18.1/internal/core/input.rs)；本机锁定 core/compiler 源码与仓库现有 Slint 集成。
- 关键结论：macOS Control 对应 Slint meta，Command 对应 control；标准输入缺少 A/E 等 Control 字母映射。标准事件已有段落、grapheme、删除、行移动、选区、IME 和撤销；游标 offset getter 被 compiler 标为内部测试用途，不用于生产光标状态。
- 对实施计划的影响：共享 EditingTextInput 只传按键事件，应用集中读取真实聚焦输入策略并通过 item 标准事件执行已匹配动作；Windows/Linux/macOS 主快捷键原样处理，密码内核和适配器均保留保护。
- 未解决问题：平台真实键盘/IME 和原生 popup 行为仍需用户验收；升级 Slint 时必须核对版本适配。Windows/Linux 本机 SDK 不可用，原生 CI 保留。

## 2026-10-14 15:28 +0800 Slint Tab 标准控件与悬浮提示

- 时间：2026-10-14 15:28 +0800
- 检索问题：Slint 标准控件能否直接替代现有工作区 Tab，并扩大 SSH RTT hover 的触发区域？
- 检索原因：用户要求核对切换/关闭标准实现并展示 hover 延迟，控件契约决定是否重构工作区。
- 来源列表：[Slint TabWidget](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/views/tabwidget/)；[Slint Tooltip](https://docs.slint.dev/latest/docs/slint/reference/window/tooltip/)；项目锁定 Slint 1.18.1 的 Cargo 编译结果。
- 关键结论：TabWidget 提供 Tab/title/current-index 等基础页面切换，公开契约没有现有关闭、重排和窗口分离意图；Tooltip 可在元素上提供延时悬浮提示和自定义单子元素内容。编译确认 Tooltip 不能直接挂 HorizontalLayout，需使用非布局容器。
- 对实施计划的影响：保留已有工作区生命周期和指针路径，在透明主体容器上复用标准 Tooltip；组合已有摘要与译文，不增加依赖、网络请求或计时器。
- 未解决问题：GUI 视觉、平台原生 hover/焦点和真实 SSH 更新由用户验收；本机编译与无窗口指针测试不能代替其他平台原生验证。

## 2026-10-14 17:12 +0800 连接等待期终端几何与焦点约定

- 时间：2026-10-14 17:12 +0800
- 检索问题：连接前的终端展示和侧栏 Enter 激活应由哪一层控制，能否独立于 transport 更新布局？
- 检索原因：用户截图与确认表明窄列只存在于连接期间，Enter 则因焦点仍在侧栏而重复新建连接。
- 来源列表：[Slint FocusScope](https://docs.slint.dev/latest/docs/slint/reference/keyboard-input/focusscope/)；[WAI Tree View 键盘交互](https://www.w3.org/WAI/ARIA/apg/patterns/treeview/)；[RFC 4254 PTY 与窗口尺寸](https://www.rfc-editor.org/rfc/rfc4254.html)。
- 关键结论：Slint 键盘事件沿聚焦元素链分发；WAI 将 Enter 定义为聚焦节点的默认激活，选择与焦点不同。RFC 定义远端 PTY 尺寸和 window-change，但不要求本地历史展示等待连接。将新终端作为等待焦点目的地是本应用的交互选择，不是 RFC 强制规则。
- 对实施计划的影响：移除本地实测尺寸上报的 connected 前提，保留有界 resize 路径；新终端以非文本 FocusScope 承接焦点，IME/网络输入仍要求 connected，安全覆盖层优先；不禁止主动创建多连接。
- 未解决问题：真实 SSH 连接期间的 GUI、键盘/IME 手感仍需用户验收；其他平台原生行为交 CI 和人工验证。


## 2026-10-15 09:15 +0800 Windows 扩展路径的地址栏展示边界

- 时间：2026-10-15 09:15 +0800
- 检索问题：为何本地路径出现内部前缀，何时可以提供常见盘符/UNC 格式？
- 检索原因：用户 Windows SFTP 截图中本地地址显示 canonicalize 返回的扩展路径，需避免格式化改变路径语义。
- 来源列表：[Rust canonicalize](https://doc.rust-lang.org/stable/std/fs/fn.canonicalize.html)；[Microsoft 文件命名与命名空间](https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file)。
- 关键结论：Rust Windows canonicalize 使用扩展长度格式；Microsoft 说明扩展前缀会关闭部分 Win32 解析，特殊命名空间、末尾点/空格与设备保留名不能简单裁去前缀。普通盘符与 UNC 地址提供可读格式是本应用的展示选择。
- 对实施计划的影响：只在本地地址栏的纯格式化中转换普通盘符/UNC；编辑和复制使用同一完整可读值，特殊路径保留前缀；不修改原始状态、刷新/历史/文件身份/传输或远端路径。
- 未解决问题：macOS 上的无窗口回归可覆盖文案和意图路由，Windows 原生文件系统/GUI 仍需 CI 和用户验证。


## 2026-10-15 09:41 +0800 macOS 与 Slint 标准快捷键和窗口归属

- 时间：2026-10-15 09:41 +0800
- 检索问题：独立窗口的 Command 编辑快捷键应由何处注册、派发和消费？
- 检索原因：子窗口排除 MenuBar，现有终端局部 key-pressed 不能替代完整的原生菜单链路。
- 来源列表：[Apple Event Architecture](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/EventArchitecture/EventArchitecture.html)；[Slint Window/MenuBar](https://docs.slint.dev/latest/docs/slint/reference/window/window/#menubar)；[Slint FocusScope](https://docs.slint.dev/latest/docs/slint/reference/keyboard-input/focusscope/)；锁定源码 `vendor/i-slint-backend-winit/{muda,event_loop,winitwindowadapter}.rs`。
- 关键结论：AppKit 区分 key equivalent、文本绑定和普通输入，菜单 action 依 target/响应链派发；Slint 每窗口声明一份 MenuBar，隐藏可保留快捷键，FocusScope 的 capture/焦点/父链决定普通按键消费。锁定后端原生菜单项携带所属窗口 ID，窗口激活时切换菜单。
- 对实施计划的影响：保持每窗口菜单和生成回调作为应用快捷键入口；终端局部处理继续兜底并保持单次消费；主页面动作在子窗口禁用，Settings/About 由主窗口承接，新分离/恢复注册同一原生输入 hook。
- 未解决问题：无窗口测试可验证生成菜单和当前窗口意图，不能替代用户在 macOS 原生菜单、IME、窗口激活及拖出/恢复过程中的验收。

## 2026-10-15 10:41 +0800 分段重绘的帧合并与协议边界

- 时间：2026-10-15 10:41 +0800
- 检索问题：同版终端解析器为何会显示不同的重绘中间光标，如何兼顾稳定性与输入响应？
- 检索原因：用户确认刷新期间临时跳位，并指出参考软件中没有相同现象，需要区分解析语义与应用呈现策略。
- 来源列表：[xterm 控制序列](https://www.invisible-island.net/xterm/ctlseqs/ctlseqs.html)；[同步输出规范](https://github.com/contour-terminal/vt-extensions/blob/master/synchronized-output.md)；[xterm.js RenderDebouncer](https://github.com/xtermjs/xterm.js/blob/master/src/browser/RenderDebouncer.ts)；[Tmux 同步能力探测](https://github.com/tmux/tmux/blob/master/tty-keys.c)；[Alacritty 读取后唤醒](https://github.com/alacritty/alacritty/blob/master/alacritty_terminal/src/event_loop.rs)；锁定 Alacritty 0.26.0/VTE 0.15.0 源码和当前应用呈现逻辑。
- 关键结论：DECTCEM 只定义显隐，不定义重绘事务；2026 才声明同步更新，能力可用 DECRQM 探测。xterm.js 合并到下一动画帧；立即发布一个输出 chunk 并不代表完整逻辑画面。只读行为比较表明参考软件额外保留已提交快照并以显隐启发式延迟提交，当前项目没有这种兼容等待；这足以解释差异，但不证明实际会话是哪组控制序列触发。
- 对实施计划的影响：保留标准解析与同步输出，采用短且不滑动延期的帧调度合并跨批输出；软件路径也按显示节奏提交，输入/协议应答不等待。新增真实时间分片和长期隐藏/真实位置更新回归，不引入参考源码、框架或猜测输入行。
- 未解决问题：缺少用户实际控制流，有限时间窗口不能承诺合并任意长分段；真实 SSH/tmux 与视觉手感需用户在修复构建中验收。

## 2026-10-15 15:18 +0800 清空型终端输入代理与撤销历史

- 时间：2026-10-15 15:18 +0800
- 检索问题：终端按 Command+Z 为何会导致进程退出，如何保留普通文本字段和远端 Control 输入？
- 检索原因：用户确认 14:56 的退出由终端 Command+Z 触发，需要在应用输入所有权边界修复而非吞掉所有平台控制键。
- 来源列表：[Slint 撤销崩溃问题 #9024](https://github.com/slint-ui/slint/issues/9024)；[Slint TextInput](https://docs.slint.dev/latest/docs/slint/reference/keyboard-input/textinput/)；锁定 Slint 1.18.1 的 TextInput::key_event/undo、InternalKeyEvent::shortcut 与本项目输入链路。
- 关键结论：上游记录过程序清空输入后撤销访问旧位置的问题；本地锁定版本的独立无原生窗口程序仍能复现空字符串索引越界。终端代理编辑后清空，但未处理 Command+Z 会进入 TextInput 标准撤销；macOS Slint control 对应 Command，物理 Control 经终端编码器生成控制字节。
- 对实施计划的影响：既有应用快捷键和可编码终端输入优先；仅在终端代理的未处理编辑组合键 fallback 消费撤销/重做，保持 IME 提交、普通字段撤销和传输契约。
- 未解决问题：安装版原始第一层 panic 未保留，系统报告只保留最终 abort 链；真实 AppKit、IME、主/独立窗口手感由用户在修复构建中验收。

## 2026-10-15 16:39 +0800 Windows 滚轮原生证据与模式分发

- 时间：2026-10-15 16:39 +0800
- 检索问题：为什么 macOS 可滚动 Codex 输出而 Windows 切换输入历史，现有跨平台测试覆盖了哪一段？
- 检索原因：用户要求各平台原生复现，并确认在另一台 Windows 主机运行编译产物；源码共同分支不能直接解释平台差异。
- 来源列表：[Windows 原生 CI job](https://github.com/xinalbert/ax_ssh/actions/runs/38677130493/job/116233176549)；[xterm Wheel mice](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h2-Wheel-mice)；锁定 Winit 0.30.13 Windows WM_MOUSEWHEEL、仓库 Slint 后端及 TerminalPane/TerminalModel 源码。
- 关键结论：Windows 原生 CI 在 `25335e0` 通过 check/严格 Clippy/build 和 814 项测试，但实际组件滚轮测试绕过操作系统鼠标输入。备用屏无鼠标上报时滚轮产生方向键符合 xterm Alternate Scroll；该分支跨平台共用，只能作为现象候选，不能据此确认故障主机根因。Windows LineDelta 与 macOS 可用的 PixelDelta 进入共享 cell 累计链路。
- 对实施计划的影响：补齐 native-wheel、local-history、wheel-route 和 wheel-enqueue 的脱敏元数据；保持滚轮策略不变，按同一构建与远端运行条件比较首次分歧。环境与可执行能力另记于环境审计，不用 CI 通过代替 GUI/Codex 复现。
- 未解决问题：当前无故障 Windows 主机访问方式，实际启动模式、鼠标模式及 tmux 路由尚待两端日志确认；最终修复必须以现场证据决定。

## 2026-10-16 10:48 +0800 Windows 主动收尾的 ConnectionAborted 证据边界

- 时间：2026-10-16 10:48 +0800
- 检索问题：Windows 10053 与已放行的 ConnectionReset 是否为同一错误？
- 检索原因：用户 CI 全部字节断言后在主动关闭阶段返回 IO ConnectionAborted，现有 helper 不放行。
- 来源列表：[Microsoft Windows Sockets Error Codes](https://learn.microsoft.com/en-us/windows/win32/winsock/windows-sockets-error-codes-2)；用户本轮原生 Windows CI 日志；当前滚轮接收夹具。
- 关键结论：10053 为 WSAECONNABORTED，10054 为 WSAECONNRESET；10053 本身也可能表示超时或协议问题，不能只凭错误码宣称正常。此次错误出现在全部断言通过并显式开始关闭之后，可在该测试夹具的 Windows 收尾边界内处理。
- 对实施计划的影响：保留显式关闭标记与严格错误分类；仅 Windows 放行收尾后的 ConnectionAborted，提前断线与其它错误仍失败，不改生产 transport。
- 未解决问题：没有该 CI run/revision，当前会话也没有 Windows 执行环境；修改后原生 CI 与实际 OS 鼠标/Codex 复现分别记录。

## 2026-10-16 指定依赖更新与 rubato 5 接口迁移

- 时间：2026-10-16 11:08 +0800
- 检索问题：六项指定新版是否保持 Rust 1.92/macOS 11，并如何保持流式重采样和 base64 行为？
- 检索原因：用户在依赖审计后授权四项兼容更新及 rubato/base64 跨约束迁移。
- 来源列表：[Tokio 1.53.3](https://github.com/tokio-rs/tokio/releases/tag/tokio-1.53.3)、[arboard 3.7.0](https://github.com/1Password/arboard/releases/tag/v3.7.0)、[serde_json 1.0.154](https://github.com/serde-rs/json/releases/tag/v1.0.154)、[uuid 1.28.0](https://github.com/uuid-rs/uuid/releases/tag/v1.28.0)、[rubato 5.0.1](https://github.com/HEnquist/rubato/tree/v5.0.1)、[base64 0.23.1](https://github.com/marshallpierce/rust-base64/blob/v0.23.1/RELEASE-NOTES.md)、[CPAL 平台下限](https://github.com/RustAudio/cpal/blob/v0.18.2/README.md#minimum-supported-versions)；本机下载的 rubato/audioadapter API 与 Cargo manifest。
- 关键结论：rubato 5.0.1 MSRV 1.87，Fft + FixedSync::Both 替代 FftFixedInOut；重导出 SequentialSliceOfVecs 可借用已有平面缓冲，无需自建 adapter 或新直接依赖。process_all 自动裁剪延迟，可独立对照分块处理。base64 新增 SIMD 引擎，现有 GENERAL_PURPOSE STANDARD/STANDARD_NO_PAD 调用不改引擎或 padding。CPAL 新版 CoreAudio 下限 14.2，因此保留 0.17.3。
- 对实施计划的影响：D2 仅更新六项及 rubato 必要传递依赖；D3 对照整段转换验证尾部、延迟、空/短输入、非整数比例和声道，另检抗混叠；D4 保留 native CI 和既有安全公告边界。
- 未解决问题：Windows/Linux 原生 CI、真实麦克风/剪贴板验收；RSA 无上游修复版本及 bincode/ttf-parser 停止维护提示不由本次更新消除。

## 2026-10-16 14:44 +0800 X11 cookie 与本机启动的隔离边界

- 时间：2026-10-16 14:44 +0800
- 检索问题：其他远端进程能否触发当前 SSH 的 X11 通道，以及如何防止错误 cookie 在被拒绝前启动本机 X server？
- 检索原因：用户报告未使用 X11 时软件启动，提出服务器其他用户请求触发的可能性并要求隔离。
- 来源列表：[RFC 4254 §6.3](https://datatracker.ietf.org/doc/html/rfc4254#section-6.3)、[OpenSSH clientloop.c](https://github.com/openssh/openssh-portable/blob/master/clientloop.c)、[OpenSSH channels.c](https://github.com/openssh/openssh-portable/blob/master/channels.c)、[sshd_config X11UseLocalhost](https://man.openbsd.org/sshd_config#X11UseLocalhost)、[XQuartz FAQ](https://www.xquartz.org/FAQs.html)、[Apple launchd socket activation](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html)；本机 x11 transport、两次拒绝日志及 XQuartz launch agent 只读核对。
- 关键结论：X11 转发不是广播；进程连接当前会话的服务器 DISPLAY 代理端口后，sshd 转为所属 SSH 的 X11 channel，再由客户端校验 cookie。来源 IP/port 不能证明 UID/PID，loopback 不等于按用户隔离。OpenSSH 也先连接本机 display 再校验 cookie，RFC 未强制这一先后；launchd socket 连接可启动 XQuartz，因此只关闭显式 launch 不足以阻止副作用。Untrusted 需要在请求前生成本机受限授权，不能照搬 trusted 的全部惰性准备。
- 对实施计划的影响：XI2 在 SSH channel 确认后先限时/限长读取并核验当前连接 cookie，通过前不做任何本机准备；拒绝只关闭该 relay。XI3 用假本机 peer 和注入准备步骤验证顺序、流完整性及取消；保留 untrusted 预授权与到期检查。
- 未解决问题：无法从现有日志识别触发者，也不能保证阻止持有正确 cookie 的合法后台程序；真实 XQuartz 启动行为与 Windows/Linux 原生链路需用户/CI 验收，服务器 UID 级隔离需要管理员边界。

## 2026-10-24 11:18 +0800 egui 原生菜单与 Linux 宿主边界

- 时间：2026-10-24 11:18 +0800
- 检索问题：eframe/winit 下如何实现 macOS、Windows、Linux 菜单，且不破坏现有事件循环？
- 检索原因：用户要求先实现原生菜单栏并兼顾 Linux/Windows；Slint 已有 File/Edit/View/Pane/Window/Help 菜单，需要跨前端保持 action 语义。
- 来源列表：[muda 0.19.3 README](https://github.com/tauri-apps/muda/tree/muda-v0.19.3)、[muda Menu API](https://docs.rs/muda/0.19.3/muda/struct.Menu.html)、[muda MenuEvent API](https://docs.rs/muda/0.19.3/muda/struct.MenuEvent.html)、[egui 0.37 MenuBar source](https://github.com/emilk/egui/blob/0.37.0/crates/egui/src/containers/menu.rs)、[eframe 0.37 CreationContext source](https://github.com/emilk/egui/blob/0.37.0/src/epi.rs)。
- 关键结论：muda 0.19.3 的 AppKit/Win32 后端可直接绑定 NSApp/HWND；Windows accelerator 依赖消息循环 TranslateAcceleratorW，而 eframe 的 winit loop 不暴露该路由，因此本实现让 egui 处理 Windows 快捷键并用 muda 显示菜单。muda 的 Linux 后端要求 GTK 3/4 窗口及开发库；当前 eframe 是 winit X11/Wayland 窗口，接入 GTK 会引入第二套宿主运行时，所以 Linux 使用 egui MenuBar。MenuEvent 通过 bounded sync channel 转发到 UI。
- 对实施计划的影响：新增 `muda` 仅在 macOS/Windows target dependency 且 default-features=false；Linux 不解析 muda/GTK。Settings/Tab 菜单共用 action 表，保留原生窗口边框；Windows/Linux 快捷键采用精确非重复消费，macOS accelerator 由 AppKit 处理。菜单与设置的 GUI、DPI、可访问性仍需各平台原生验收。
- 未解决问题：当前机器只有 macOS SDK；Windows 完整 crate 构建需 Windows SDK/windows.h，Linux GTK 原生菜单未实现且不属于当前 winit 宿主边界。

## 2026-10-24 E7P 原生 Option 输入接口核对

- 时间：2026-10-24
- 检索问题：如何避免为 Option 产生的特殊字符错误添加 Escape 前缀？
- 检索原因：egui 的 Text 已经过平台输入法/键盘转换，必须在原生层保留 Alt 原始字符。
- 来源列表：[winit 0.30.13 WindowExtMacOS](https://docs.rs/winit/0.30.13/winit/platform/macos/trait.WindowExtMacOS.html)；本机锁定 eframe 0.37.0 `epi.rs`、egui-winit 0.37.0 输入和 winit 0.30.13 macOS view 源码。eframe docs.rs Frame 页面本次加载失败，不作为已核验来源。
- 关键结论：WindowExtMacOS 的 OptionAsAlt 会忽略 Option 重音转换并保留原始字符和 Alt 修饰；eframe 提供当前 winit window。策略需随终端焦点恢复，设置/编辑器继续使用平台文字输入。
- 对实施计划的影响：macOS egui 对已有 winit 版本新增可选直接声明，不引入第二事件循环；原生策略状态同步到终端编码，IME commit 不加 Meta 前缀。缓存源码另确认 eframe RequestPaste 会归一化 CRLF 并忽略空文本，故 OSC 52 原始文本读取改用显式授权后的单个后台读取 job，不复用普通 Paste 事件。
- 未解决问题：实际 Option/dead-key/IME 及多平台原生 GUI 验收仍独立保留。

## 2026-10-25 E7A 续作：格式、临时授权与需求触发

- 时间：2026-10-25 11:56 +0800
- 检索问题：实验前端如何传递音频格式和连接级授权，保持现有设备权限、需求触发及清理边界？
- 检索原因：接续未收口的 E7A；复核此前记录所引用但 research 中缺失的音频依据，先读官方正文，再补回归与文档。
- 来源列表：[CPAL 0.17.3 使用及流启动](https://docs.rs/cpal/0.17.3/cpal/)、[PulseAudio 16.0 source-output 的 source/corked 字段](https://www.freedesktop.org/software/pulseaudio/doxygen/structpa__source__output__info.html)、[PulseAudio 16.1 module-pipe-source 实现](https://raw.githubusercontent.com/pulseaudio/pulseaudio/v16.1/src/modules/module-pipe-source.c)、[Tokio 1.53.3 watch 文档源码](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/watch.rs)、[egui 0.37.0 Ui 实现](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/ui.rs)。已读取相关正文：CPAL 配置/回调/play、Pulse 字段与 pipe-source 参数、watch 最新值/borrow_and_update、Ui 输入与 sizing。Tokio/egui docs.rs 页面不可访问，使用对应 tag 官方源码补足。
- 关键结论：以上是官方 API 契约及参考实现，并非统一的远端麦克风协议标准。锁定 CPAL 0.17.3 支持按配置建流，部分后端建流即启动，不能套用新版统一暂停语义；设备访问必须继续留在录音需求之后。Pulse 的 source-output 提供所属音源及暂停状态；既有桥接以自有音源的非暂停流触发。pipe-source 接受 rate/channels/FIFO，watch 只保留最新值，适合非秘密格式配置。项目决定：UI 只改草稿，workspace 保存成功后发布格式；新 SSH worker 捕获该格式。连接选项默认关闭、不进入 profile/工作区，仅当前连接及已授权重试沿用；替换/主动断开清除。复用设备租约、有界 PCM、取消/join，认证与主机信任前不准备音源。
- 对实施计划的影响：E7A 保持 in_progress，先核对现有接线，补全采样率/声道选择及保存失败、重连和撤销回归，更新双语说明，完成两前端完整门禁、组合检查和双 macOS target check/严格 Clippy；无依赖升级或新原生权限实现。
- 未解决问题：真实 macOS TCC、Windows/Linux 设备与远端 Pulse/pipewire-pulse 行为、GUI 由原生环境和用户验收；未重新核验 Apple 权限实现，不将其记作本轮已读证据。附件/手动转写继续独立后续阶段。

## 2026-10-25 E7I 剪贴板图片附件入口

- 时间：2026-10-25
- 检索问题：egui 图片粘贴如何接入原有私有 PNG/SFTP 附件，同时避免后台任务向已切换终端插入路径？
- 检索原因：继续 E7R 图片入口；核对锁定事件、像素格式、阻塞任务取消和 SFTP 成功回执。
- 来源列表：[egui 0.37.0 Event](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/data/input/event.rs)、[egui-winit 0.37.0 clipboard](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui-winit/src/clipboard.rs)、[eframe 0.37.0 WGPU paste dispatch](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/eframe/src/native/wgpu_integration.rs)、[ecolor 0.37.0 Color32](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/ecolor/src/color32.rs)、[Tokio 1.53.3 blocking task](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/task/blocking.rs)、[image 0.25.10 PngEncoder](https://docs.rs/image/0.25.10/image/codecs/png/struct.PngEncoder.html)、[SFTP v3 draft §6.3–6.6](https://datatracker.ietf.org/doc/html/draft-ietf-secsh-filexfer-02#section-6.3)。已读取对应正文并核对本机锁定源码。Open Group 单引号正文两版本均返回 403，未作为已读证据；保持项目现有单引号转义并用真实 shell 回归验证。
- 关键结论：egui/eframe 是 API/参考实现，文本表示优先，无文本时产生 Arc<ColorImage>；Color32 为预乘 alpha，后台须转回非预乘 RGBA。Tokio 已开始的 blocking job 不能 abort，取消必须保留并 join。PNG 编码接收指定颜色类型的像素；复用项目有界 encoder。SFTP v3 文档是过期草案而非正式 RFC；WRITE 和 CLOSE 都必须等 STATUS，既有上传已遵循此约定。
- 对实施计划的影响：新增 E7I，先计划后代码。egui 适配器持有单个有界像素转换 job、焦点/终端/视口票据和取消状态，后台编码后发送至 controller；controller 拥有每终端单个附件准备任务、私有文件和同连接 SSH 请求，成功后仅发布 ready token，界面复核原焦点/可见 pane/连接/视口再确认插入。PNG 不经过 PTY，无自动换行；限 4096²/16 MiB/32 附件，Telnet/Serial 拒绝。取消撤销 token，disconnect/replace/close/exit join 后清理；远端复用已有 worker 的会话末清理。无需新依赖或文件选择器；OSC 52 保持独立文本流程。
- 未解决问题：上游原生剪贴板读取及首次 ColorImage 构建在 eframe 内发生，应用只能在事件后限制尺寸；egui 文本优先与默认前端图片优先有差异，明确记入双语使用说明。真实 OS 剪贴板/焦点和 GUI 交用户验收，远端断网清理仍尽力而为；其它原生平台/MSRV 本机不可验证。

## 2026-10-25 E7H 会话侧栏遮罩设置

- 时间：2026-10-25 16:30 +0800
- 检索问题：怎样复用默认前端的会话地址遮罩，在 egui 提供单字符草稿、实时预览和原始连接数据隔离？
- 检索原因：E7R 对照发现 egui 只有完整 endpoint 悬停提示，缺少遮罩地址行和 Workspace 遮罩字符控件；先核对 API 再施工。
- 来源列表：[egui 0.37.0 TextEdit](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/text_edit/builder.rs)、[egui 0.37.0 Response](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/response.rs)、[Rust 1.95.0 Ipv4Addr](https://raw.githubusercontent.com/rust-lang/rust/1.95.0/library/core/src/net/ip_addr.rs)。已阅读 singleline/char_limit/id_salt、on_hover_text 的 AccessKit description 行为及 octets 的四字节/稳定版本契约，并对照本机锁定 egui 源码；Rust 版本化 HTML 不可访问，采用官方 tag 正文。
- 关键结论：这些是官方 API 契约而非地址脱敏的正式标准。TextEdit 可限制输入字符数，仍须复用 config 的规范化以处理空白/非法草稿；稳定 ID 不随语言改变。on_hover_text 同时发布可访问性描述，完整 endpoint 仍会出现在悬停/辅助技术中。默认规则只遮用户名中段和 IPv4 第二段，域名、IPv6、串口不改；旧使用文档的 192.*.202 示例与源码不符，应纠正为 192.*.1.202，保持现有行为。
- 对实施计划的影响：新增 E7H；将既有纯格式化函数移至共享 controller 子模块并由默认适配器 re-export，仅导出一个有意供两 binary 使用的函数。egui 列表增加遮罩地址，保留原完整悬停详情/搜索/编辑/连接；Workspace 草稿字符限一，显示前用既有 WorkspaceSettings normalizer，预览关闭恢复保存值，保存失败不发布。仅 UI 表示与纯格式化跨层，无 transport、凭据、持久 schema 或任务生命周期变更。复用两条既有 PO 键；验证协议/Unicode/原值不变、全部语言/预览/失败恢复和两前端门禁。
- 未解决问题：列表遮罩不是秘密保护边界，完整地址仍可搜索、编辑并由提示/辅助技术读取；名称是用户数据，保持原样。真实 GUI/辅助技术交用户验收；MSRV 1.95 与其它原生平台仍依赖 CI。

## 2026-10-25 E7B 文字亮度与粗体亮色实施前核对

- 时间：2026-10-25 16:56 +0800
- 检索问题：egui 终端如何应用已有文字亮度/粗体亮色设置，同时保留反显、淡色、隐藏和装饰颜色顺序，且不误改透明度？
- 检索原因：E7R 外观对照发现两个已保存字段没有 egui 控件及 Painter 消费；本条在实现代码修改前记录。
- 来源列表：[xterm SGR](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)（Patch #411，已读 SGR 1/2/7/8 与颜色段）、[xterm boldColors](https://invisible-island.net/xterm/manpage/xterm.html#VT100-Widget-Resources)（已读 boldColors/boldFont/boldMode）、[egui 0.37.0 Color32 官方源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/ecolor/src/color32.rs)（已读 alpha、to_srgba_unmultiplied/gamma_multiply）、[egui 0.37.0 Slider 官方源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/slider.rs)（已读整数、clamping 与 step_by）。本机 Cargo.lock、config normalizer、Slint terminal_render 及回归作为项目事实。
- 关键结论：SGR 区分 bold/faint/inverse/hidden；xterm 的 0–7 映射至 8–15 是可选 boldColors 策略，不是所有终端必须采用的形式标准。Color32 使用预乘 alpha，gamma_multiply 同时缩放 alpha，不能替代现有 HSL 亮度。Slider 支持 5 点步长；范围 60–120、默认 100 和 dim 0.70 是既有项目策略。
- 对实施计划的影响：新增 E7B，先提取默认前端已有 HSL 纯算法至 controller/text_colors，仅导出一个 RGB 数组函数，默认适配保持行为。egui Appearance 加两控件并复用已有 PO；先提升低 ANSI 索引，再反显，再对可见文字/下划线/删除线应用亮度及 dim，最后 hidden；背景/光标/选区不参与。设置纳入行缓存键，无 PTY 输出也即时预览/还原；保存仍归 workspace 原子事务。无新依赖/schema/任务或安全授权。
- 未解决问题：两前端主题 ANSI 配色差异、semantic/custom theme/刷新设置仍属后续 E7R；本切片只共享亮度算法，不扩大颜色协议支持。GUI/辅助技术由用户验收，MSRV 1.95 与其它原生平台依赖 CI。

## 2026-10-25 E7R-Theme 自定义调色板实施前核对

- 时间：2026-10-25 18:16 +0800
- 检索问题：egui 0.37 如何把已有 Light/Dark 自定义主题草稿映射到全局视觉，同时保持 CSS hex 解析边界、egui 主题隔离和可读性保护？
- 检索原因：E7R 对照发现 Slint 已有 26 个自定义调色板输入，而 egui 只有主题/调色板选择，无法编辑 `ThemePaletteKind::Custom`；需要在施工前确认锁定 API 和颜色格式边界。
- 来源列表：[egui 0.37.0 Context 源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/context.rs)（已读 `set_visuals_of`/`set_theme` 的 Light/Dark 主题槽位）、[egui 0.37.0 Color32 源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/ecolor/src/color32.rs)（已读 sRGBA 预乘 alpha、非预乘转换和 gamma-space 操作）、[CSS Color 4 §5.2](https://www.w3.org/TR/css-color-4/#hex-notation)（已读 3/4/6/8 位 hex 语法）、[WCAG 2.2 §1.4.3](https://www.w3.org/TR/WCAG22/#contrast-minimum)（已读普通文字 4.5:1、大文字 3:1 对比度要求）。对照 Cargo.lock 的 egui/eframe 0.37.0、本地 `ThemeSettings::normalized` 与 `ThemePalette::normalize_for_brightness`。
- 关键结论：`Context::set_visuals_of` 是 egui 官方的按 Light/Dark 槽位设置 Visuals API；应用应在 UI 帧前应用当前保存/草稿主题，不从 Painter 或窗口回调改全局状态。Color32 内部使用预乘 alpha，编辑字段应读取普通 sRGBA；但 config 的自定义主题已允许 `#RRGGBB`/`#RRGGBBAA`，egui 主题中的文字/表面颜色应使用其规范化结果。CSS 允许 3/4/6/8 位，但项目 config 的主题规范化是更窄的可读性策略，不能在 egui 侧放宽。WCAG 对比度是可访问性依据；项目现有 `ThemePalette::normalize_for_brightness` 已在保存/规范化时保护表面、文字、边框、状态和终端颜色。
- 对实施计划的影响：新增 E7R-Theme。egui Appearance 将在选择 Custom 时展示 Light/Dark 两组稳定 ID 的 13 个 hex 字段，使用 `TextEdit::char_limit` 限制输入；预览调用 `ThemeSettings::normalized` 生成临时 `ThemeSettings`，不改变 workspace/config。`egui_app::theme::Theme` 接收规范化后的主题并通过 `set_visuals_of` 应用两个槽位，当前模式继续由 `set_theme` 选择。保存仍交给现有 catalog 原子事务，失败保留原草稿。非法/低对比输入显示可读规范化结果，空字段不成为全局主题颜色；不新增 schema、依赖、transport、任务或权限。
- 所有权与偏差：主题领域/config 继续拥有颜色 schema、归一化和 contrast 规则；egui 只负责草稿编辑和 `ThemeSettings` 的临时解析。egui 文本字段允许 alpha 以复用现有 config；没有在 UI 引入 CSS 颜色名或 3/4 位短式，以保持两前端保存契约一致。W3C/WCAG 是可访问性标准，egui API 只是实现 API，不构成颜色标准。
- 未解决问题：真实系统高对比度/辅助技术和窗口视觉仍交用户验收；默认 Slint 的 Theme token 与 egui Visuals 并非一一对应，自定义 palette 仅覆盖现有 13 个共享角色。

## 2026-10-25 E7Q 语义高亮实施前核对

- 时间：2026-10-25 17:24 +0800
- 检索问题：如何将默认前端日志级别/HTTP 高亮和四类颜色迁移到 egui，避免 UI 正则处理、终端颜色覆盖及无界缓存？
- 检索原因：E7R 对照发现已保存 semantic 字段未在 egui 消费；本条与执行计划先于实现修改。
- 来源列表：[OpenTelemetry Logs Data Model](https://opentelemetry.io/docs/specs/otel/logs/data-model/#field-severitynumber)（当前 1.62.0，已读 SeverityText/SeverityNumber 与映射）、[RFC 5424 §6.2.1](https://www.rfc-editor.org/rfc/rfc5424.html#section-6.2.1)（已读 severity 表）、[RFC 9110 §15](https://www.rfc-editor.org/rfc/rfc9110.html#section-15)（已读 100–599 与五类语义）、[regex 1.13.1 官方性能/非可信输入](https://docs.rs/regex/1.13.1/regex/#untrusted-input)（已读编译复用、captures_iter 的 O(m*n²) 上界）、[epaint 0.37.0 LayoutJob/Section 官方源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/epaint/src/text/text_layout_types.rs)（已读字节范围顺序/覆盖及 section 排字）。Cargo.lock、默认 highlighting/eligibility、config 规范化和现有语义回归为本地依据。
- 关键结论：RFC/OTel 规定状态类别/级别含义，不规定 UI 颜色；四类配色/同义词/HTTP 诊断文本上下文是既有项目策略，HTTP/2/3 文本不是 wire status line。regex 必须复用编译对象且限制输入和匹配迭代；默认 512 字节、ASCII 一字节一格、默认前背景及非 inverse/dim/hidden eligibility 保持。egui section byte range 必须有序无缺口，固定网格每字符偏移继续保留。
- 对实施计划的影响：E7Q 提取现有识别及 eligibility 至共享 controller；默认适配保留原 split/装饰行为。egui 在 controller publish 时计算可见行每 run 的范围/类别，无正文副本，按源 Arc 身份复用，仅保留当前可见行；预计算与开关无关以支持即时草稿预览，绘制回调不编译/匹配正则。Painter 用范围改变每字 section 前景再应用既有亮度，保留布局/背景/显式装饰/原终端风格；开关与规范化四颜色进入缓存键。控件复用既有译文，稳定 ID 与短文本上限；配置 schema/SSH/权限/worker 取消所有权保持。
- 未解决问题：仅同一个合格终端 run 内识别，不跨样式边界/折行/Unicode；宽/组合文字保留原样。链接交互/链接颜色、自定义主题和其它显示设置另属 E7R；真实 GUI/辅助技术及其它原生平台/MSRV 仍待验收。


## 2026-10-25 E7R-FPS 实施前核对

- 时间：2026-10-25 20:57 +0800；本条与计划先于 Rust 修改。
- 检索问题：egui 后台唤醒、Tokio 定时取消和最新焦点通知如何配合已有解析任务，实现前台/后台输出快照限频而不延迟协议处理？
- 检索原因：config/Slint 已有 focused/unfocused FPS，而实验 controller 固定 16 ms 发布、egui 未提供控件；需先核对锁定 API 的计时和唤醒契约。
- 来源列表：[egui 0.37 Context 官方源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/context.rs)（已读 request_repaint、request_repaint_after、回调锁约束）、[Tokio 1.53.3 Sleep 官方源码](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/time/sleep.rs)（已读 sleep_until 与取消）、[Tokio 1.53.3 Interval 官方源码](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/time/interval.rs)（已读立即首 tick、默认 Burst 和 Skip）、[Tokio 1.53.3 Watch 官方源码](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/watch.rs)（已读 latest-value、borrow_and_update 与关闭约束）。docs.rs 版本页访问失败后改读官方版本 tag 正文；不依赖检索摘要。对照锁文件、Slint TerminalPresentationPolicy、config 60/4 默认及 1–120 规范化范围。
- 关键结论：eframe 的 repaint 回调允许后台任务唤醒 UI；request_repaint_after 是延迟重绘请求，不能作为精确输出限频器。sleep_until 在等待中不做工作，可由 select 取消；绝对 deadline 避免持续输出反复延后。watch 只保留最新值，适合性能性质的焦点提示；不跨 await 保留 borrow。官方 API 不规定产品 FPS，60/4 与 1–120 是项目既有策略。
- 对实施计划的影响：替换 controller 固定 16 ms ticker，以最近一次实际发布和保存 FPS 计算 deadline；只在有脏内容时等待，合并同步输出 hold，错过期限后只发布当前帧。状态/用户动作可即时发布，字节解析/协议响应继续处理，因此设置约束的是输出快照而非全应用帧率。焦点独立 watch 避免命令队列满或旧代次让后台终端滞留前台策略；DEC1004 命令仍保持现有代次/有界约束。复用 UI 已有 focus/suspend 通知和 workspace 保存事务，不增加任务或依赖。
- 未解决问题：Slint 的持续输出自适应降频/隐藏路由不直接移植，egui 使用可配置的输出快照上限；未聚焦及隐藏终端统一后台频率。真实 GUI 帧率、功耗和原生焦点仍待用户验收；其它平台/MSRV 交既有 CI。


## 2026-10-25 E7R-FPS 焦点生命周期补充核对

- 检索问题：隐藏/鼠标释放失败或连接代次重置时，如何保证呈现焦点提示反映当前 UI，同时不重复发送 DEC1004？
- 检索原因：最终检查发现 suspend 在取消远端鼠标错误后提前返回，generation 变化会清除 UI 的 focused 标志；仅依赖布尔变化通知不足以覆盖这些路径。此补充先于修正代码。
- 来源列表：[egui 0.37 Response 官方源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/response.rs)（2026-10-25 已读 has_focus/gained_focus/lost_focus/request_focus/surrender_focus，has_focus 同时检查原生窗口和 widget 焦点）、[Tokio 1.53.3 Watch 官方源码](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/watch.rs)（复用本轮 latest-value/关闭与短 borrow 核对）。
- 关键结论：呈现焦点是当前 UI 状态；egui 的 widget/native 焦点与连接代次无关。只有焦点变化时发送协议报告仍合适，但不能用该变更缓存推断最新呈现提示已经送达。
- 对实施计划的影响：保留 focus(bool) 的既有有界协议行为，增加明确的公开呈现提示方法作为 library 到 binary 的窄接口，不写 transport。UI 每个准入帧发布提示，suspend 在可能失败的鼠标释放之前先清除此提示；不会增加协议报告。提示和协议仍各自受既有所有权/代次边界约束。增加重复提示不入命令队列及旧 UI 代次提示测试，再重跑门禁。
- 未解决问题：这是状态正确性修正，不对原生焦点/功耗作验收结论；GUI 和其它平台边界保留。


## 2026-10-25 21:52 +0800 E7R-Icons 实施前核对

- 检索问题：如何复用既有系统图标目录，匹配 Slint alpha colorize、本地 OS 身份/覆盖与 egui 草稿事务，且不在 UI 解析 SVG/读取系统文件？
- 检索原因：已存 system_icons_follow_theme/local_shell_icon_override 在 egui 未消费，Sidebar/Tab 仍为通用字形；需要确认锁定资源/颜色 API 与构建 gate。
- 来源列表：[resvg 0.48.1 render 官方源码](https://raw.githubusercontent.com/linebender/resvg/v0.48.1/crates/resvg/src/lib.rs)、[usvg Options](https://raw.githubusercontent.com/linebender/resvg/v0.48.1/crates/usvg/src/parser/options.rs)、[ImageHrefResolver](https://raw.githubusercontent.com/linebender/resvg/v0.48.1/crates/usvg/src/parser/image.rs)、[tiny-skia 0.12 Pixmap](https://raw.githubusercontent.com/linebender/tiny-skia/v0.12.0/src/pixmap.rs)、[egui 0.37 Image/Button](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/image.rs)、[Button 实现](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/button.rs)、[epaint ColorImage](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/epaint/src/image.rs)、[TextureHandle 生命周期](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/epaint/src/texture_handle.rs)、[Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)、[Cargo optional dependencies](https://doc.rust-lang.org/cargo/reference/features.html#optional-dependencies)、[os-release 规范](https://www.freedesktop.org/software/systemd/man/latest/os-release.html)。已读正文/API；本地对照锁文件、Slint SystemPlatformIcon、config 规范化及原 local_platform/cache 逻辑。
- 关键结论：resvg 输出 sRGB 预乘 RGBA，可直接映射 ColorImage::from_rgba_premultiplied；Image tint 为乘色，需白色 alpha mask 才能复现单色轮廓。TextureHandle 最后引用 drop 时释放。usvg 默认允许本地 image href，构建必须禁用 string/data resolver；Cargo 生成物置 OUT_DIR 并声明 rerun 输入，不能写源目录或用 host OS 判断 target 平台。os-release 优先 /etc 再 /usr/lib，不执行 shell；原代码保留 ID 精确匹配/不继承 ID_LIKE 与 8KiB 限额，UI 只读启动缓存。48px 与最多 128 项是本项目资源预算，不是外部规范。
- 对实施计划的影响：私有 build 模块消费 catalog.json 指定的 SVG/PNG（限定仓库内路径、2MiB 输入；PNG 8192 边长/32Mi 像素，覆盖现有 8001×3151 素材），生成 48×48 premultiplied bytes 与静态目录；增加同锁定版本的 egui 可选 resvg build-dependency，无新运行时 decoder。UI 每个 owner 最多 128×2 native/mask 纹理，最小化/clear/退出释放；图标选择只用 canonical ID。提取已有纯平台映射到库，Linux 启动 blocking 发现；Sidebar 使用已校验 profile cache/override，Tab 以当前聚焦 pane 决定 local/remote icon。Appearance 编辑本地覆盖与主题轮廓，预览/还原立即显示，保存沿用事务；新文案不得声称即时落盘。
- 未解决问题：本轮不新增远端探测/会话级图标编辑器，已有 override/cache 可显示；原生视觉/高 DPI/辅助技术由用户验收，其它平台/MSRV 交 CI。无 SSH 信任/凭据/权限/schema 变更，无多 agent/Worker。生成器仅调整共享输出路径，按 Python code style 保留行为。

- E7R-Icons 身份读取补充决策（实现前）：官方 os-release 规定只有 /etc/os-release 缺失才回退 /usr/lib；原读取器对所有错误都回退。本轮提取时增加可注入读取器，保留 8KiB/ID 不继承规则，修正为仅 NotFound 回退，其它错误使用通用 Linux；补严格来源优先测试。不执行内容或新增网络探测。


## 2026-10-25 23:28 +0800 E7R-Shells 实施前核对

- 检索问题：怎样迁移已有 shell 发现/选择缓存，保留自定义失效项和满列表当前选择，且不在 UI 查 PATH/执行程序或因发现而自动保存？
- 检索原因：egui 只有原始路径输入，未消费 known_shells/后台 discover_shells；原审计将兼容组名字段与未提供的列表增删页混为未迁移，需要先对照真实实现。
- 来源列表：[egui 0.37 ComboBox 官方源码](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/containers/combo_box.rs)（已读 from_id_salt/selected_text/show_ui）、[egui 0.37 TextEdit](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/text_edit/builder.rs)（已读 singleline/id/char_limit，按字符限制）、[Tokio 1.53.3 spawn_blocking](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/task/blocking.rs)（已读池上限、启动后不可 abort、shutdown 等待及 bounded work 约束）。版本与 Cargo.lock 匹配；本地对照 discover_shells、TerminalSettings::merge_known_shells/normalize_shell_settings、Slint Settings 异步候选和 egui 事务。
- 关键结论：标准选择控件应给稳定 ID 并保留当前值，自定义执行路径继续用有界单行输入；UI 显示原始 shell 名/路径，不翻译用户值。blocking 任务一旦开始不能靠 abort 取消，因此启动前单任务必须 await 完成，不丢弃 join。32 项、256 字符、ASCII 大小写去重与 System default 是项目契约，不是通用 shell 标准。原默认前端是发现/选择缓存，没有独立候选增删页；组名长度只有兼容传递，当前两前端均未消费。
- 对实施计划的影响：复用现有 discover_shells，在 GUI 建立前 blocking 发现并 join；失败仅警告并给空发现集，保留已保存/自定义选择。Context 本地 Arc 候选先经 domain 规范化；UI 候选副本以当前选择、已知项、新发现排序，复用 domain 限额和去重，满列表也保留当前值。打开设置/预览/发现不改草稿缓存或保存值；Save 对提交副本合并候选，成功后 config/workspace 应用到新终端，失败保留草稿，既有 worker 不重新启动。控件复用已有两条 PO，避免创建无效组名控件；兼容字段留待紧凑侧栏决策。
- 未解决问题：原生 UI/键盘/平台 PATH 检测由用户/CI 验收；其它平台/MSRV 仍未在本机可运行。单次发现不提供动态重扫/列表删除，也不执行 shell 校验；选中失效项在明确新建终端时按现有 resolver 报错。无依赖/schema/平台 cfg/SSH 安全变化，无多 agent/Worker。


## 2026-10-26 05:34 +0800 E7R-SFTP-Key 实施前核对

- 检索问题：如何把已保存 SFTP 快捷键接到现有 companion 的实际操作，同时保证精确 pane/控件焦点、不偷本地终端 Ctrl+M、不重复执行或自动认证，并补齐八项保存键的冲突检测？
- 检索原因：当前仅五项设置键可编辑，SFTP companion 已实现但无菜单/快捷键入口；导入/导出还没有实验 controller 操作，不能先增加无效编辑入口。
- 来源列表：[egui 0.37 Response](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/response.rs)（已读 has_focus/request_focus/scroll_to_me）、[egui 0.37 Modifiers](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/data/input/modifiers.rs)（已读 matches_exact 与 command 平台语义）、[Muda 0.19.3 MenuItem](https://raw.githubusercontent.com/tauri-apps/muda/muda-v0.19.3/src/items/normal.rs)（已读 set_enabled/set_key_accelerator）。版本匹配 Cargo.lock；本地对照共享菜单表、现有 native adapter、ShortcutSettings 八字段与 SFTP companion 连接契约。
- 关键结论：Response 请求焦点必须在控件创建后、Context 锁闭包外执行；用稳定 ID，且禁止 sizing pass/禁用状态产生副作用。精确匹配不吞重复或额外 Alt/Shift。Cmd/Meta 是项目现有解析别名；冲突需按解析后的键比较而非字符串。原生菜单可沿用同表和已有同步适配，不增加平台 API。焦点定位不是外部协议标准，是本项目对 companion 的交互决策。
- 对实施计划的影响：View 增加既有 PO 标签 SFTP，使用保存的 open_sftp（默认 Ctrl+M），只在当前已保存 SSH pane/ready/无模态下启用；UUID 请求同帧验证并只消费一次，Remote path 使用稳定 ID/可访问名称，抑制同帧 terminal focus。快捷键页保留五个旧索引后追加 SFTP，所有八字段参与解析/语义与固定菜单保留键校验；导入/导出不添加入口。预览不改变路由，成功 Save 后生效，失败保留草稿。无新任务/channel/网络/凭据/持久化契约。
- 未解决问题：临时 SSH 没有保存 profile，既有 companion 无法通过 Connect 使用它，因此本次不启用此入口；不自动连接/认证/List。原生菜单/焦点/键盘由用户验收，Windows/Linux 与 MSRV 1.95 由 CI 补证，Intel 只做交叉 check/Clippy。无多 agent/Worker。

- E7R-SFTP-Key API 补充核对（修正前）：锁定版 TextEdit builder 无 accessible_name；对照上述 Response 官方源码的 labelled_by 文档和本地同版本实现，使用控件 Response.labelled_by(可见 label.id)，符合标准标签关联方式，不增加自定义无障碍名称接口。

- E7R-SFTP-Key 平台语义补充（修正前）：上述 Modifiers 官方定义 Windows/Linux 的 command 与 ctrl 同值，且 matches_exact 将两者视作同一物理键模式；保存前比较不能只用未归一的 KeyboardShortcut 结构相等。决定仅在非 macOS 校验键中把 Cmd/Meta 合入 Ctrl，macOS 保留二者独立；用显式平台参数的私有纯函数在宿主同时验证两种语义，不改变菜单/终端输入映射。


## 2026-10-26 05:46 +0800 E7R-Transfer 实施前核对

- 检索问题：如何复用既有 Slint 会话/分组 JSON 协议，保证未知格式拒绝/脱敏/新 UUID/冲突命名/失败原子性，并把有界后台结果安全交给显式 egui Copy？
- 检索原因：egui 尚无导入导出，规则被 UI binary 私有持有；现有 controller 事务可复用，不能重写另一套格式或在绘制回调解析/写盘。
- 来源列表：[Tokio 1.53.3 spawn_blocking](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/task/blocking.rs)（已读 bounded work/不可 abort/运行时等待）、[egui 0.37 TextBuffer](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/text_edit/text_buffer.rs)（字符索引/char_limit 与字节不同）、[egui 0.37 Context](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/context.rs)（对照本地锁定源码 copy_text 输出命令）、[Serde container attributes](https://serde.rs/container-attrs.html)（已读 deny_unknown_fields；现有 envelope 使用该属性，profile 继续既有兼容规则）。egui/Tokio 与 Cargo.lock 匹配，Serde 文档用于现有锁定 derive 属性，不升级依赖。
- 关键结论：有限 CPU/文件任务适用 spawn_blocking，启动后不可取消，必须 await/join；char_limit 是字符数，提交前仍需字节上限；剪贴板写入应只由当前有效请求结果触发 Context.copy_text，不读取系统剪贴板，也不向 terminal 注入 JSON。deny_unknown_fields 只约束 envelope，不能宣称所有嵌套兼容字段都严格拒绝。
- 对实施计划的影响：提取项目既有纯规则到共享库，保留 format axssh-session-export/version1、256 KiB、128 profiles、空组、Copy 命名和脱敏（nil 输出 UUID、新导入 UUID，清除 credential refs/vault marker/host trust/detected OS）。共享规则在候选副本上执行，成功验证/写盘后才发布；controller 增 import/export catalog 意图及最多 256 KiB 的脱敏结果，沿用 64 总容量/精确请求和旧 revision 拒绝。egui 使用显式粘贴导入对话框（256Ki 字符、提交 256KiB）和目标选择/Copy 导出对话框，关闭/旧请求结果只消费丢弃，背景 terminal 全禁用；全部八键可编辑。使用既有 PO 标签，必要时只补确需的新提示，不建文件选择器或自动 clipboard read。
- 未解决问题：默认前端原自动读取剪贴板与 egui 显式粘贴有交互差异，需双语说明和原生验收；系统 keyring/vault 不迁移，不自动连接、不导出密码/信任。其它平台原生链接/运行、Intel 运行与 MSRV 1.95 交 CI；无多 agent/Worker。

- E7R-Transfer 对话框补充核对（改用 Modal 前）：已读 [egui 0.37 Modal 官方实现](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/containers/modal.rs)，Modal backdrop 阻断背景输入，should_close 处理 Close/Esc/背景点击并尊重 popup；普通 Window 本身不是模态。采用 Modal，保留显式 Close，并在父 sizing pass 不创建/提交/消费回执，避免独立 Area 不继承父 sizing 标记。


## 2026-10-26 06:23 +0800 E6R-Pause 实施前核对

- 检索问题：如何把既有下载暂停/继续接入实验 owner/UI，正确区分能力与等待回执，在队列满、迟到 Progress、断开和旧连接命令下不误报状态或泄漏任务？
- 检索原因：实验快照只有 status 字符串/conflict，UI 无 Pause/Resume；共享 SFTP-only worker 已有命令和回执，但 ActiveSftpTransfer::is_pausable 仅 Download 为真，不能把上传也展示为可暂停。
- 来源列表：[Tokio 1.53.3 bounded mpsc try_send](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/mpsc/bounded.rs)（已读立即返回、Full/Closed 与值未发送）、[Tokio 1.53.3 Notify](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/notify.rs)（已读通知无数据、单 permit/登记与丢唤醒说明）、[egui 0.37 Ui](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/ui.rs)（已读 stable scope/UUID push_id、disabled/sizing 行为，适配标准 add_enabled Button）。docs.rs 入口不可用，采用版本化官方源码并对照本地同锁定版本实现；未升级依赖。
- 关键结论：try_send 成功仅代表入队，不能视作已经暂停；必须等本项目 Paused/Resumed 回执，失败不得进入等待态。Notify 只是唤醒，已有 TransferCancellation 的原子 flag/循环复验才是状态，继续复用，不添加第二套暂停实现。暂停/继续不是 SFTP wire 标准能力，而是本项目下载 chunk 调度；既有 in-flight 请求可能先结束，连接断开不可续传。标准禁用控件仍需 owner 权限/代次复验与每帧去重。
- 对实施计划的影响：给传输快照增加阶段与下载暂停能力，pending 状态不被 Progress 冲掉，terminal 阶段吸收迟到非终态事件；新任务在既有 64 history 中只回收终态。上传在发送前全量登记，未知 child Queued 仅来自共享 worker 的递归下载，Started/Progress 不凭空创建新记录。Pause/Resume/Cancel 在当前 generation/connected/UUID/能力/阶段复验后发给原 worker，成功才进入等待状态。UI 复用 Pause/Resume 两条 PO；不扩框架、协议、权限、凭据或持久配置。
- 未解决问题：本轮只实现单项下载控制，批量选择、远端重命名/删除、原生拖出继续保留；原生 GUI/真实服务器暂停延迟由用户验收。自动回归用固定信任 peer、读取闸门与独立 SystemRelease FIFO 回执作为命令处理 barrier；不以 sleep 猜测暂停成功。其它平台/MSRV/Intel 原生运行仍需 CI，无多 agent/Worker。

- E6R-Pause 控件可达性补充（改动前）：已读 [egui 0.37 ScrollArea 官方实现](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/containers/scroll_area.rs) 的 max_height/默认按需滚动条及裁切；当前传输历史直接铺满最多 64 行，尾部暂停/继续可能被 pane 裁切。决定沿用当前目录区域的标准纵向 ScrollArea，新增独立稳定 ID，历史区域最多 160px、空列表自收缩，不改变 64 条预算或终端输入规则。
- E6R-Pause 能力来源补充：本地共享 worker 的 upload_queued_event 明确发送空 remote_path；新记录只对 Queued 且 remote_path 非空授予下载暂停能力，未知上传即便未预登记也保守拒绝。不是根据显示 name/status 或服务器文案推断。


## 2026-10-26 07:25 +0800 E6R-Batch 实施前核对

- 检索问题：怎样在既有 64 行/64 命令预算内实现批量选择/控制，处理旧选择和发送中途失败，并明确“入队”与实际完成的区别？
- 检索原因：默认前端按活动列表过滤选择，但 request_selected_transfer_actions 先发送全部再更新 UI，发送中途失败可能掩盖前面的已入队项；实验端应复用能力规则并显式记录部分成功，不能直接复制该处理顺序。
- 来源列表：[egui 0.37 Button](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/button.rs)（已读 selectable/selected 与 disabled 标准按钮）、[egui 0.37 Ui](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/ui.rs)（对照本地 selectable_label、stable push_id/UUID）、[Tokio 1.53.3 bounded mpsc](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/mpsc/bounded.rs)（已读 try_send 的 Full/Closed 与成功只代表入队）。版本匹配 Cargo.lock，不升级依赖。
- 关键结论：标准选择按钮只展示传入 selected 值，UI owner 可在 clicked 后按 frame 单次提交；disabled 仍须业务 owner 资格复验。多个 try_send 不是原子事务，后项失败不能撤销先项；所以每项成功后马上更新 pending 状态，失败继续收集，不能宣称整个 batch 已完成。批次合并/最新结果是项目决策，不是 SFTP wire 标准能力。
- 对实施计划的影响：一个有界 batch command（1–64 unique UUID）沿用 generation，复验 current connected/每项 typed eligibility；缺失或不适用计 skipped，发送失败保留原阶段并收集最多 64×512 字符错误，已成功项保留等待态。快照只保留最近汇总，不引入须可靠消费的每请求结果队列或 UI pending ticket，重连/断开清空。每 pane 独立选择、活动列表 pruning、全选/清空、每帧/sizing/禁用 guard 与 UUID 控件；复用既有批次标签，新增 Queued ({0}) 译文明确只入队。最新结果无副作用、不会迟到触发命令。
- 未解决问题：批量远端文件操作、原生拖出和真实 GUI/多服务器仍后续；上传不可暂停，取消不回滚已完成文件。其它平台/Intel 原生运行与 MSRV 1.95 交 CI，无多 agent/Worker。


## 2026-10-26 20:37 +0800 E6R-Mutate 实施前核对

- 检索问题：怎样复用既有 remote Rename/Remove，在确认框关闭/导航/队列失败/早到或迟到回执下保留单项结果，避免误作用于新条目？
- 检索原因：默认 rename 复验窗口/原始单选/目录 readiness，delete 直接发既有写操作；egui 尚无行入口，现 edit_event 会把未知 Failed 也当外部编辑错误，须先按 UUID 路由新操作。
- 来源列表：[SFTP v3 草案 02](https://www.ietf.org/archive/id/draft-ietf-secsh-filexfer-02.txt)（已读 6.2、6.5、6.6、6.8：名称/REMOVE/RENAME/RMDIR/LSTAT）、[egui 0.37.0 Modal](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/containers/modal.rs)（已读 backdrop、top modal、Esc/popup 和独立 Area）、[Tokio 1.53.3 bounded mpsc](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/mpsc/bounded.rs)（try_send Full/Closed 与成功仅入队）。russh-sftp 3.0.1 docs.rs 入口不可用，沿用 checked-in execute_sftp_write 及锁定 API，不新增原生 API。
- 关键结论：v3 草案为历史协议定义而非已发布 RFC，RENAME 已有目标应失败，REMOVE 不删目录，RMDIR 不递归且非空应失败；现 worker 已做 lstat/类型/目标不存在/超时，并仅使用常规 rename，不改语义。独立 egui Modal 不继承父 sizing/disabled，须在根展示前 guard，终端原始输入另受 workspace blocked 控制。异步命令错误必须有精确请求结果，不能依赖可合并快照；单保留槽是本项目的有界串行决策。
- 对实施计划的影响：typed action + 私有 frozen target；client 在短锁内 try_send 并登记单槽，不持锁 await，队列失败不占槽。owner 复验代次/目录 request/可见条目 type/size/mtime，只在 worker 接纳后登记 pending；精确 Completed/Failed 写结果，导航不回跳；关闭标记放弃，断开清空。重命名沿用 trim/512 字符/禁斜杠反斜杠/控制字符规则；UI 明确确认单项 Delete，保留失败草稿并阻断快捷键/终端输入。
- 未解决问题：本地可见元数据与 lstat 不能建立服务器原子身份保证；本轮不扩展身份协议、递归删除或覆盖重命名。不增批量远端选择/原生拖出；GUI/真实服务器、其它平台/Intel 运行与 MSRV 交后续验收。无多 agent/Worker。

- E6R-Mutate 控件边界补充（改动前）：已读 [egui 0.37.0 Label](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/label.rs) 的 truncate 与默认 elided tooltip；4096 字符路径若直接换行可能挤走确认按钮，改用标准单行截断/悬停完整文本，错误区保留最高 120px 标准滚动；行操作以 path push_id 保持排序后的对象身份。


## 2026-10-26 21:54 +0800 E6R-Select 实施前核对

- 检索问题：怎样迁移路径多选和批量下载，兼顾目录递归、部分发送失败、旧选择/本地目标变动和有界发现任务？
- 检索原因：默认路径 checkbox/全选不含范围选择，默认 queue_remote_downloads 逐项发根；实验现有 queue_download 给目录预建可取消 root 行；完整核对表明目录首文件复用 root UUID，空/全过滤目录返回 DiscoveryFailed，应保留根行/发现取消。共享 OpenSftpFile 入口尚未使用已有 available_sftp_transfer_slots，批次放大后须补齐同一个 512 槽预算。
- 来源列表：[egui 0.37.0 Button](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/button.rs)（已读 selectable/selected 与禁用语义）、[egui 0.37.0 Ui](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/ui.rs)（稳定 push_id/作用域，与既有 UI 路由对照）、[Tokio 1.53.3 bounded mpsc](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/sync/mpsc/bounded.rs)（已读 try_send Full/Closed，成功仅入队）、[SFTP v3 草案 02](https://www.ietf.org/archive/id/draft-ietf-secsh-filexfer-02.txt)（已读 6.7 READDIR/EOF/CLOSE、6.8 LSTAT）。本地 Cargo.lock 与 checked-in worker/discovery 为实际版本/生命周期依据，不升级依赖。
- 关键结论：v3 草案不是正式 RFC，递归/批次/选择不是 wire 能力，继续复用本项目有界发现和传输；常规根返回同 UUID，目录首文件复用 root UUID、其余子项使用新 UUID，空/全过滤目录返回根 DiscoveryFailed，根入队不能宣称文件完成。标准 selectable 只展示 selected 值，owner 按 clicked/frame 更新可避免多 pass 重复翻转；路径 ID 不能依赖列表下标。多个 try_send 不构成原子事务，保留先前成功并报告后项失败。
- 对实施计划的影响：一条有界 paths 意图绑定 generation/remote request/local revision，owner 重验整个目标再逐项处理；失效/链接/过滤跳过，失败不建行不回滚，最新摘要导航/断开清空。保留根行与发现阶段取消、首子项复用身份和空目录失败，子文件仍受 64 行/超额取消。共享发现入口使用既有 512 active/pending/queued/discoveries 总预算并发既有 DiscoveryFailed，使重复批次不能累积无界发现任务；无新事件、队列或持久配置。UI 选择仅当前 pane/目录最多 64，保留明确的单项修改确认。
- 未解决问题：本轮不迁移批量修改/多选拖放/本地多选上传与原生拖出，空目录沿用不创建本地目录。真实 GUI/文件管理器/服务器与其它平台/MSRV/Intel 运行另验；自动测试用固定信任 peer，不绕过验证，无多 agent/Worker。

- E6R-Select 实施中校正：完整读到 discover_initialized_download_requests 末尾后确认空目录返回错误，经既有 DiscoveryFailed 结束根行；初始“根行无法结束”的判断撤销，已恢复原 queue_download 行登记，不改变发现阶段 Cancel。研究影响仍是路径选择/部分入队和共享入口预算，未新增 wire/事件。


## 2026-10-26 22:29 +0800 E6R-UploadSelect 实施前核对

- 检索问题：怎样把本地多选接入现上传准备，防止旧目录/文件替换/取消后发布，保留有界阻塞任务与冲突语义？
- 检索原因：现 upload_roots 接受多根，local_source 仅承载一个拖入 path/revision；local_files 私有目录快照目前只保留普通文件，选中目录也需保留元数据并在后台验证。
- 来源列表：[Tokio 1.53.3 blocking](https://raw.githubusercontent.com/tokio-rs/tokio/tokio-1.53.3/tokio/src/task/blocking.rs)（已读 spawn_blocking 有界工作、开始后不能 abort、shutdown 等待）、[egui 0.37.0 Button](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/button.rs)（selectable/disabled/truncate）、[Rust 1.95.0 fs](https://raw.githubusercontent.com/rust-lang/rust/1.95.0/library/std/src/fs.rs)（metadata/symlink_metadata、mtime/TOCTOU）。版本化 HTML 不可用，使用官方源码并对照本地共享 validators/native 上传 identity；不升级依赖。
- 关键结论：spawn_blocking 不能靠 abort 取消，应沿用单任务/协作 flag/退出 join；源对象验证只能作为观测，不能宣称消除 TOCTOU。普通文件复用 validate_local_file_for_open 的打开后指纹与 canonical parent；目录不能假设 File::open(directory) 跨平台可用，因此用 symlink_metadata 类型/长度/mtime/归属检查，后台发现仍过滤链接并有硬上限。标准 selectable 的点击由 owner 每帧提交，路径稳定 ID 不依赖行下标。
- 对实施计划的影响：新增有界 UploadSelected 绑定当前可见路径，私有来源 enum 扩展到 selection snapshot，保留单项拖入语义；扫描前与构造请求后复验来源，owner 提交前再检验 local revision/源快照与远端 request/generation。目录快照留在 library，local open 明确拒绝目录；本地/远端导航撤销准备，已接纳上传继续由 worker 所有。UI 只展示选择与 Preparing/现传输行，冲突仍明确逐项，不把扫描等同上传成功。
- 未解决问题：目录元数据不是原子内容快照，原生 worker 仍只上传发现并校验的普通文件；不新增树级原子保证。新按钮不改变现拖放载荷、Keep both/应用批次仍须完整界面对照；GUI/其它平台/MSRV/Intel 运行交后续，未使用多 agent/Worker。

## 2026-10-26 E6R-UploadConflict 实施前核对

- 检索问题：如何在锁定 egui 0.37.0 中实现上传冲突的 Keep both 与“将本次选择应用于上传批次”，让模态对话框阻断背景输入、关闭行为可预测，并保证多 pass/禁用状态不会重复提交意图？
- 检索原因：默认 Slint 已有冲突对话框和批次复选框；实验 egui 目前只提供 Overwrite/Skip，controller 的 `Command::ResolveConflict` 仍固定传 `apply_to_batch = false`，而 worker 已实现 KeepBoth 与同 batch 策略，必须先对齐版本化 UI API 与现有所有权边界。
- 来源列表：[egui 0.37.0 Modal](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/containers/modal.rs)（已读 `show` 的前景层/遮罩/背景输入阻断以及 `ModalResponse::should_close` 的 Esc、遮罩、关闭响应语义）、[egui 0.37.0 Checkbox](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/checkbox.rs)（已读点击直接切换布尔值、禁用响应与稳定控件信息）、[egui 0.37.0 Button](https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/widgets/button.rs)（已读 selectable/disabled/小按钮行为）；Tokio/russh 的实际批次决策则核对 Cargo.lock 对应项目源码 `src/ssh/worker/sftp.rs` 与 `src/ssh/worker.rs`。
- 关键结论：`Modal::show` 是 egui 0.37 的标准阻断容器，使用稳定 UUID 派生 ID；冲突窗口只持有 UI 临时 `apply_to_batch` 草稿，点击三种决定按钮后由 owner 一次提交 `resolve_conflict_with_batch`，遮罩/Esc 默认按 Slint 规则提交 Skip 并清除窗口；复选框不写配置、不触发网络。controller 保留旧 `resolve_conflict` 兼容调用，新增带布尔参数的最小方法并原样传给既有 worker；worker 继续拥有 batch policy、KeepBoth 编号命名和有界 pending 决策。
- 对实施计划的影响：扩展 `Command::ResolveConflict` 为携带 `apply_to_batch`，handler 不复制批次逻辑；egui `TransferControls` 增加单个 pending conflict UUID、临时勾选值和稳定 Modal，行内按钮只负责打开窗口，窗口按钮才发送决策。测试覆盖 KeepBoth、checkbox、单次提交、多 pass、关闭/禁用与跨 generation 清理，再执行默认/egui/组合和双 macOS target 门禁。
- 未解决问题：真实 GUI 的视觉尺寸、键盘/辅助技术顺序和真实服务器冲突延迟仍由用户/平台验收；KeepBoth 的最终编号由 worker 按现有远端路径规则决定，不新增 wire、依赖、持久化 schema 或多 agent/Worker。
## 2026-10-27 E6R-SelectedDrag 实施前标准核对

- 检索问题：egui 的 Local files/Remote files 已选路径如何实现同一 pane 内的多项拖放，同时保留一次性提交、连接代次和目录快照边界？
- 检索原因：当前按钮批量上传/下载已经存在，但拖动已选行仍只提交单项；需要先按锁定 egui 版本确认 payload 与释放语义，再复用 controller 的批量边界。
- 版本与日期：2026-10-27；Cargo.lock 锁定 egui 0.37.0，当前 Rust edition 2024/MSRV 1.95 约束不变。
- 来源列表：
  - egui 0.37.0 `Response::dnd_set_drag_payload`、`dnd_hover_payload`、`dnd_release_payload`：<https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/response.rs>
  - egui 0.37.0 `Ui::dnd_drag_source`、`Ui::dnd_drop_zone`：<https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/ui.rs>
  - egui 0.37.0 `DragAndDrop` payload 所有权与取消规则：<https://raw.githubusercontent.com/emilk/egui/0.37.0/crates/egui/src/drag_and_drop.rs>
- 关键结论：官方 API 要求 payload 为 `Any + Send + Sync`，拖动开始时由 source 写入 Context；释放 API 只在当前 response 命中指针且本帧发生释放时取走 payload。`dnd_drop_zone` 适合绘制独立 drop 区，但当前两栏已经使用带 native 坐标和裁剪矩形的 `Response`，继续用 response 级 API 可保持既有 Wayland/坐标门禁。payload 应为拥有的数据，不能借用选择 map；egui 不替 owner 复验连接、目录或文件身份。
- 既有实现对照：`src/egui_app/sftp_drop.rs` 已将单路径 payload 绑定 pane/generation/revision，并在 owner 侧调用 `upload_local_drop`/`download_drop`；controller 已有 `upload_selected`/`download_selected` 的 1–64 路径限制、目录 request/revision 复验、逐根 skip/failure 摘要和后台准备任务。
- 对实施计划的影响：把 payload 扩展为有界拥有的路径列表；当拖动的行属于当前选择时携带该选择的稳定路径快照，否则保留单项路径行为。目标侧按列表长度选择既有 selected API 或单项 API；不在 UI 读文件、不新增 task/channel/schema/wire 协议，所有本地元数据和远端目录复验继续由 controller/worker 执行。保留原生文件管理器拖入和 egui 原生拖出未实现。payload 只在当前 egui Context 内短暂存在；pane/generation 不匹配拒绝，controller 再检查 request/revision 和已列出的条目；失败不清除选择，也不绕过 SSH/SFTP trust。路径列表最多 64 项且通过现有 controller 边界验证。
- 未解决问题：系统文件管理器 file promise、真实平台拖放视觉/辅助技术与真实 SFTP 服务器验收仍不由本切片证明。
