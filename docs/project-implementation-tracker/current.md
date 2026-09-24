# 当前项目实施记录

## 当前目标

- 目标 ID：20260923-skia-layer-cache-memory
- 目标：定位约 1.3 GB footprint 的主要来源，并修复动态终端行销毁后 Skia layer GPU 图像仍被缓存的问题。
- 交付物：锁定 Slint 1.18.1 的 Skia 缓存生命周期补丁、依赖锁定、双语架构说明和采样/验证记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`vendor/i-slint-renderer-skia/`、`Cargo.toml`、`Cargo.lock`、第三方声明、双语架构和 tracker 文档。
- 本轮范围：Skia renderer 的 per-component layer cache 释放；用现有进程的 sample、vmmap、heap 与锁定源码确定问题边界。
- 不在本轮范围内：终端模型、PTY/transport、SSH host-key trust、凭据、worker ownership 或自动 GUI 截图验收。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 本轮依赖升级

- 目标 ID：DEPS-SLINT-20260923
- 目标：升级兼容依赖锁定版本，并将 Slint 及本地 Winit backend patch 统一适配到 1.18.1。
- 状态：实施与本机验证完成；Linux/Windows CI runner 验证待执行。
- 已完成：升级 Slint 1.18.1、fontdb 0.24、argon2 0.6、chacha20poly1305 0.11、russh-sftp 3.0 和兼容锁定依赖；Slint 字体桥接切换到 fontique-011；本地 Winit patch 迁移至 1.18 window-adapter input dispatch API；补齐凭据加密与 SFTP v3 新配置适配；同步双语架构、项目地图和版本基线。
- 约束：`base64` 0.22 仍由 `alacritty_terminal` 的 `^0.22` 约束要求；Slint 依赖图同时解析出自身所需的 0.23。未扩大 alacritty API 约束。
- 验证：macOS ARM64 fmt/check/严格 Clippy/test（库 280、应用 267、Doc tests 0）通过；Windows MSVC target 已尝试但 macOS 缺 Windows SDK/MSVC headers，Linux targets 和 macOS x86_64 未安装，交由对应 CI runner 验证。
- 风险/待办：Windows、Linux x86_64/ARM64 和 macOS x86_64 的 target-specific check/Clippy/build/test 仍待 CI；Slint UI 的视觉/焦点行为由用户验收。

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SFTPLIVE1 | completed | 返回 SFTP Tab 和断线重连后保留本地选择，同目录刷新继续上传，目录切换时锁定目标 | 状态定向测试、Slint 重编译、离线 Rust 门禁和 diff 已通过 | 复用现有 Tab/worker；真实窗口切换与服务器上传由用户验收。 |
| SFTPUPLOADBATCH1 | completed | 本地/Finder 多选和目录递归上传、单命令批次队列、远端目录创建 | 递归发现与远端目录定向测试、Slint 重编译、完整离线门禁 | 并发仍为每 Tab 2 条；超出扫描上限整批拒绝；真实服务器和 GUI 行为待用户验收。 |
| SFTPCONFLICT1 | completed | 远端同名上传标准弹窗、按批次选择、worker 端重验与安全发布 | SFTP 状态/传输定向测试、Slint 重编译、完整离线门禁和翻译检查 | 默认询问；覆盖需 POSIX rename 扩展；真实服务器和视觉行为待用户验收。 |
| NOTICE1 | in_progress | 活动 pane 的连接/剪贴板 notice 在窗口居中限宽、长文换行滚动、窄窗口动作纵排与重复标题清理 | Slint 重编译、notice 定向回归、fmt/check/Clippy/test/diff；用户视觉验收 | 保持 pane UUID 动作路由和非阻塞 Tab 切换；不改 SSH 信任或凭据边界。 |
| MEM1 | completed | Sample/vmmap/heap 与 1.17.1/1.18.1 缓存生命周期根因核对 | 进程类别、配置和锁定源码交叉核对 | 运行中安装版为 1.17.1；GPU row cache 已启用。 |
| MEM2 | completed | 1.18.1 Skia `layer_cache.component_destroyed` 本地补丁和 Cargo patch | Cargo locked/offline 编译、严格 Clippy、测试 | 本地 vendor 源码与上游仅差该一行；根锁文件仅改变 crate 来源。 |
| MEM3 | completed | 双语架构、项目地图、采样复核说明和完整门禁 | fmt/check/Clippy/test/build、tracker/diff；用户真实负载对照 | ARM64/x86_64 macOS 编译门禁通过；内存降幅仍待同负载采样。 |
| TITLEBAR5 | completed | macOS 空白标题栏显式拖窗，Tab/按钮手势互斥，左侧原生命中仅限标题栏高度 | Slint 重编译、fmt/check/Clippy/test/build/diff；用户实际拖动验收 | 复用 Winit `drag_window()` 和现有 AppKit content-view subclass；不增加依赖。 |
| TITLEBAR6 | completed | 将 macOS 标题栏前沿多余空白转移到终端分栏按钮前 | Slint 重编译、fmt/check/Clippy/test/diff；用户视觉验收 | 保留红绿灯安全区和 TITLEBAR5 的拖动/手势边界；只调整标题栏空白分配。 |
| TITLEBAR7 | completed | 允许拖动标题栏分栏按钮前的空白区域移动窗口 | Slint 重编译、locked/offline Cargo 门禁、diff；用户实际拖动验收 | 仅 macOS 主窗口；Tab 和标题栏按钮保留原手势。 |
| TITLEBAR8 | completed | Windows/Linux 主窗口 Tab 外空白区域支持拖窗 | Slint 重编译、locked/offline Cargo 门禁、diff；目标平台实际拖动验收 | 将 Winit `drag_window()` 回调扩展到桌面平台；Tab、分栏、连接按钮保留原手势。Windows target 跨编译受本机缺少 MSVC/Windows SDK 阻断，Linux target 未安装。 |
| DRAW1 | completed | 持久化 TerminalDrawingPreference、设置 UI 和安全禁用 Canvas | 配置回归、Slint 重编译、fmt/check/Clippy/test/translation/tracker/diff | 库 280/应用 267 测试通过；Canvas 未实现且控件禁用。 |
| TITLEBAR1 | completed | macOS 主窗口隐藏原生标题文字，将 Tab 条延伸到红绿灯所在标题栏区域 | Slint 重编译、fmt/check/Clippy/test/build、翻译/diff | 保留原生红绿灯并为其留空；显式 target check/Clippy 被 Skia 下载阻断，视觉由用户验收。 |
| TITLEBAR2 | completed | 固定 macOS 顶部 Tab 起点，不随侧栏展开/收起移动 | Slint 重编译、fmt/check/Clippy/test/build、翻译/diff | Tab 横向固定在红绿灯右侧、纵向固定在窗口顶部；视觉由用户验收，显式 target 门禁仍受 Skia 下载限制。 |
| TITLEBAR3 | completed | 阻止 macOS Tab 拖动同时移动主窗口 | fmt/check/Clippy/test/build/diff | 主窗口内容视图仅在红绿灯留空内允许原生标题栏拖窗，Tab 重排仍由 Slint 处理；实际拖动由用户验收。 |
| TITLEBAR4 | completed | 让 macOS 标题栏底部分隔线横贯整窗 | Slint 重编译、fmt/check/Clippy/test/build/diff | 单条线包含红绿灯留空与 Tab 区；其他平台保留侧栏右侧起点，视觉由用户验收。 |
| DRAW2 | pending | 在不改 parser/model 的前提下实现 Canvas/custom-paint 最小终端网格原型 | 定向 glyph/grid tests、Slint/Cargo 编译、交互/字体度量检查 | 进入前需核实 Slint 1.17.1 绘制 API；不替换默认 renderer。 |
| DRAW3 | pending | 验证 renderer 切换兼容性并决定 Canvas 是否可选/默认 | 宽字符/fallback/选区/IME/hyperlink/光标/软件 backend 回归和 A/B 采样 | 只有行为和平台验证完成后才能开放选择；GUI 由用户验收。 |
| SFTPDRAG1 | completed | 目标命中、Winit/AppKit 路由和标准 copy/drop 契约 | SFTP 定向回归、Slint 重新编译、locked/offline Cargo 门禁和差异检查 | 外部文件只可投到 Remote files；原生远端回拖只可落到 Local files；缺少可靠目标一律拒绝。 |
| SFTPFOLLOW1 | completed | macOS 原生 drop 坐标和普通下载完成语义 | 状态回归、Slint/Cargo 重新编译、locked/offline Cargo 门禁和差异检查 | macOS 在 drop 时读取 AppKit 坐标，普通下载仅完成并保留文件；hover 前置条件已由 MACDROP3 移除。 |
| MACDROP2 | completed | macOS 原生坐标读取失败时 fail-closed | macOS 定向回归、Slint/Cargo 重新编译、locked/offline Cargo 门禁和差异检查 | 读取失败不得回退到旧 Winit 坐标；MACDROP3 仅移除不可靠 hover 前置条件。 |
| X11MODE1 | completed | 每服务器 X11 模式枚举、旧布尔迁移与编辑器 DTO | 配置/编辑器定向回归、Slint/Cargo 重新编译 | `true` 迁移到原有的 trusted 行为，`false` 迁移为关闭；不保存 cookie。 |
| X11MODE2 | completed | `-X` 受限授权与 `-Y` 既有信任转写 | X11 unit/loopback 回归、worker 生命周期审阅 | `-X` 使用短时 `xauth generate … untrusted`，`-Y` 保持惰性真实 cookie 转写。 |
| X11MODE3 | completed | 中英文用法/架构、翻译、月度记录和完整门禁 | 翻译/Markdown/tracker、fmt/check/Clippy/test/diff | GUI 控件和目标平台 X server 行为由用户验收。 |
| SFTPDIAG1 | completed | 脱敏内部拖放生命周期、落点和入队诊断 | SFTP payload 定向测试、Slint 重新编译、fmt/check/Clippy/test/diff | 不记录路径、主机、文件内容或凭据；macOS 实际拖放仍待用户复现。 |
| MACDROP3 | completed | macOS `DroppedFile` 以实时 AppKit 坐标命中 Remote files，不依赖前置 hover；固定拒绝阶段 | macOS 定向回归、Slint 重新编译、locked/offline Cargo 门禁和差异检查 | 仍拒绝无坐标、非 SFTP/未启用/区域外目标；不按活动目录回退。 |
| MACDROP4 | completed | 原生拖放以可见 Remote files 几何命中，上传入口以实时 SFTP 状态最终校验 | SFTP 状态定向回归、Slint 重新编译、locked/offline Cargo 门禁、构建和差异检查 | UI 快照滞后不能静默拒绝；未连接、loading 或空目录仍由 Rust 拒绝且不入队。 |
| SFTPTRANSFER1 | completed | SFTP transfer 显式区分上传/下载状态和列表显示 | 状态定向回归、Slint 重新编译、locked/offline Cargo 门禁和差异检查 | 上传不再复用 `Downloading`；每条记录显示 `Upload` 或 `Download` 方向；上传 admission 也计算正在打开的下载 subsystem，保持每 Tab 最多两个 active/opening transfer。 |
| TERMSTD1 | completed | 终端模型采用标准同步输出、光标/SGR 状态和鼠标编码 | `terminal` 定向回归、完整 Cargo 门禁 | `?25` 仅控制光标可见性；`?2026` 使用标准结束序列或上游超时释放；查询应答保持有界并回写原 transport。 |
| TERMSTD2 | completed | Slint 传递网格几何和指针像素坐标，Local/SSH PTY 上报字符与物理像素尺寸 | UI DTO/状态定向回归、Slint 重新编译、完整 Cargo 门禁 | Telnet 仍只发送 RFC 1073 NAWS 字符尺寸；Serial 只调整本地模型。 |
| TERMSTD3 | completed | 滚动与网格锚点、双语架构说明、研究记录和完整门禁 | fmt/check/clippy/test/diff、Markdown/tracker 检查 | 代码与文档交付完成；GUI 视觉、真实终端程序和目标平台 PTY 行为由用户验收。 |
| STDUI1 | completed | 终端标题、ResetTitle、Bell 事件进入有界应用状态 | terminal/model/app focused tests，随后 Cargo 门禁 | 动态标题只改运行时 Tab，不写入 profile/workspace；空标题合法；Bell 只触发 bounded UI hint。 |
| STDUI2 | completed | OSC 8 URI 经过有界渲染 DTO，并保持安全 target 约束；F13-F24 普通编码 | terminal/render/input/app focused tests | 仅显式 HTTP(S) 目标可打开，其他 scheme inert；Kitty CSI-u 不默认启用。 |
| STDUI3 | completed | 更新双语标准化边界，明确 OSC 52 与图形协议的 opt-in/后续设计 | Markdown/tracker/link checks | OSC 52 默认关闭，显式开启后仅允许有界远端写入本机默认剪贴板；图形协议不伪装成文本网格能力。 |
| STDUI4 | completed | 完整 Rust/Slint 离线验证与差异检查 | fmt/check/clippy/test/diff | GUI、真实 TUI 和目标平台手工验收留给用户。 |
| OSC52-1 | completed | 默认关闭的 OSC 52 写入策略与有界终端事件 DTO | 配置/终端 focused tests，随后 Cargo check | 只允许远端写入本机默认剪贴板；读取和图形协议不在本阶段。 |
| OSC52-2 | completed | UI 线程剪贴板写入与所有终端 transport 接入 | app focused tests、Slint 重编译 | 不阻塞 worker，不记录或持久化剪贴板内容。 |
| OSC52-3 | completed | 双语边界说明、实施记录和完整离线门禁 | fmt/check/clippy/test/diff、tracker validator | 目标平台剪贴板和真实 TUI 仍需用户验收。 |
| OSC52READ1 | completed | 读取事件 formatter 与 Tab-local 一次性 pending 请求状态 | terminal/model focused tests | 不向 Slint DTO 暴露 formatter；仅默认 clipboard、默认关闭策略。 |
| OSC52READ2 | completed | 非阻塞确认 notice、允许/拒绝动作与 20 秒超时 | AppState/route focused tests | 允许前不读取剪贴板；请求失效时 fail-closed。 |
| OSC52READ3 | completed | UI 线程读取剪贴板、当前 worker 回写、断开/关闭清理 | bridge/transport focused tests | 不记录、不持久化剪贴板内容，回写失败也清除 pending。 |
| OSC52READ4 | completed | 双语边界说明、项目地图和完整离线门禁 | fmt/check/clippy/test/diff | 真实 TUI、目标平台剪贴板和 GUI 视觉仍需用户验收。 |
| STDUI5 | completed | F13-F24 使用 xterm/terminfo 标准下发序列，并覆盖带修饰键编码 | `src/terminal/input.rs` 定向测试、完整 Cargo 门禁 | 不改变 Kitty CSI-u 默认关闭策略。 |
| STDREP1 | completed | 协议响应队列、CSI `t` 查询覆盖和 1016 依赖语义形成明确标准化结论 | terminal focused tests、双语文档、tracker/diff 检查 | 不伪造缺少窗口位置/屏幕几何 DTO 的响应；保持队列有界。 |
| TELNETSTD1 | completed | Telnet TTYPE 协商回报 `xterm-256color`，并明确 Telnet/Serial 能力边界 | Telnet loopback、完整 Cargo 门禁、双语文档 | Telnet 不伪造 PTY 行规程；Serial 保持原始字节流；其它 Telnet 扩展继续关闭。 |
| KEYSTD1 | completed | Slint/Winit/终端输入标准矩阵和 Shift/修饰键边界 | `terminal::input::tests`、`app::input::tests` | 保持布局解析事件文本优先，逻辑键只作空文本 fallback；物理小键盘不再进入专用 application-keypad/SS3 路径；IME、AltGr、NumLock 与应用快捷键不被终端编码器抢占。 |
| KEYSTD2 | completed | 修正 Shift 文本 fallback、补齐 xterm 导航/功能键和 keypad fail-closed 回归 | focused tests、Slint/Cargo 重编译 | 原生事件文本优先，逻辑键只作空文本 fallback；不启用 Kitty keyboard protocol、CSI-u 或 `modifyOtherKeys`。 |
| KEYSTD3 | completed | 双语输入契约、tracker/environment 记录和完整离线门禁 | fmt/check/Clippy/test/translation/diff/tracker | 目标平台真实键盘布局、IME、NumLock、应用快捷键和 GUI 焦点仍需用户验收。 |
| KEYSTD4 | completed | 移除物理数字小键盘 application-keypad/SS3 专用拦截并回归标准输入路径 | fmt/check/Clippy/test/translation/diff/tracker | 物理小键盘保留事件元数据但不改变标准文本/逻辑键路由；Kitty keyboard protocol、CSI-u 和 `modifyOtherKeys` 继续关闭。 |
| IOSTD1 | completed | 四类 transport 输出统一为带接收时间的 `TerminalOutputChunk`，应用展示统一消费时间戳 | `cargo check --locked --offline`、定向输出测试、diff | 不记录原始终端内容；队列、上限和 UI 线程边界不变。 |
| IOSTD2 | completed | 输入统一分配应用序列号，标记类型、字节数、结果和耗时；补齐 monitor 输出诊断 | `cargo fmt --all -- --check`、严格 Clippy、定向测试 | SSH 既有 transport 内部序列保留，应用序列用于跨 transport 对齐。 |
| IOSTD3 | completed | SFTP 目录请求/响应关联 request ID，过期响应 fail-closed；同步双语架构和环境记录 | 全量 Cargo 门禁、tracker validator、diff | 只接受当前 Tab request ID；关闭/重连会使旧请求失效。 |
| SFTP-ID-20260923 | completed | 将目录 request ID 收敛为 AppState 唯一 owner；首个目录页不关联导航 ID，修复首屏永久 Loading | 定向 SFTP 状态回归、全量 Cargo 门禁、diff | SSH trust、凭据和传输并发不变；真实服务器交互由用户验收。 |
| XPLATCFG1 | completed | 平台专属 helper/import/caller/test 的 `cfg` 对齐，并拆分 guarded keyboard event match | macOS 全量 Cargo 门禁；Linux/Windows target 命令已尝试并记录工具链限制；diff | 不使用 `allow(dead_code)` 或放宽 Clippy；保持 macOS 行为不变。 |
| XPLATCFG2 | completed | 将平台边界和 target-specific 严格 Clippy 要求固化到 `AGENTS.md` | tracker/environment 记录、diff | 目标平台 SDK/linker 缺失时必须记录限制，不能以未验证交叉编译替代 CI。 |
| XPLATCFG3 | completed | 修正 ARM Linux 暴露的键盘事件 arm/import 边界，并把整条平台专属 match arm 纳入 `cfg` | macOS fmt/check/Clippy/test/diff；ARM target 命令已尝试并记录标准库限制 | 不保留 cfg-only body 的未保护绑定，不增加 `allow`/`expect`；非 macOS 继续由 `_` 兜底。 |

## 已完成

- 已定位 2026-09-23 22:31 安装版 Sample 的 1.3 GB footprint 主要在 Metal 图形资源：同 PID 的 `vmmap -summary` 报约 1.1 GB `IOAccelerator (graphics)`、80 MB `IOSurface`，malloc 实际分配约 76 MB；`heap` 有数千个 AGX texture。安装版是 Slint 1.17.1，而当前源码锁定 1.18.1；这不是修复后内存测量。
- 已确认本机持久化设置选择 GPU 且打开 terminal row render cache。Slint `ItemCache` 的组件指针缓存要求销毁时执行 `component_destroyed`，两版 Skia 的 `free_graphics_resources` 均漏掉 `layer_cache`；修复在当前 1.18.1 的本地补丁中补齐，并保留其他 renderer、配置与应用状态边界。
- 已完成 TITLEBAR5：macOS 主窗口在 Tab 下层空白区域按下左键时通过 `AppWindow` callback 同步请求 Winit 系统拖窗；Tab 和按钮仍在上层处理原手势。AppKit content view 仅允许顶部标题栏前沿的红绿灯留空原生拖窗，避免侧栏被误判为标题栏。TITLEBAR6 将前沿从 96px 收紧为 60px，并把剩余 36px 放到终端分栏按钮之前；TITLEBAR7 让这段尾部空白也可拖动窗口；TITLEBAR8 将 Tab 外空白拖窗回调扩展到 Windows/Linux。

- 已完成 OSC52-1–3：Terminal Settings 新增默认关闭的 `osc52_clipboard`；开启后 `alacritty_terminal` 使用 `Osc52::CopyPaste`，但应用层只接受默认 clipboard，selection 仍拒绝；协议事件限制为 64 KiB 解码文本并通过有界 DTO 传递。
- 已完成 OSC52 UI bridge：Local、SSH、Telnet、Serial 四类 transport 共用 `TerminalOutputEffects`，monitor 取得剪贴板事件后经 `dispatch_ui` 调用平台默认剪贴板 API；不记录、不持久化、不在 worker 线程触碰 UI/平台剪贴板。
- 已完成 OSC52 回归与文档：默认关闭、默认目标、selection 拒绝、超限丢弃、读取确认、拒绝/超时/断开/重试清理、Settings preview/save 和中英文架构/用法说明均已覆盖；Sixel、Kitty、iTerm2 图形协议仍明确排除。

- 已完成 STDUI5：F13-F16 改为 xterm-256color 的 `CSI 1;2P` 到 `CSI 1;2S`，F17-F24 改为标准扩展 tilde 序列；带 Shift/Alt/Control 修饰键时保留相同的 xterm 参数位，不复用旧的错误 F13-F20 或 F21-F24 编码。
- 已完成 STDREP1：协议事件队列继续保持容量 16，队列满时记录有界诊断而不静默；`CSI 13 t`、`CSI 15 t`、`CSI 19 t` 因当前模型没有窗口位置/完整屏幕几何而不伪造回答；`1016` 明确只作为 `1006` SGR 的像素扩展，单独启用时仍走 legacy 坐标。
- 已完成 TELNETSTD1：Telnet 接受 `DO TTYPE`，回报 `WILL TTYPE`，并将 `TTYPE SEND` 回写为 `IS xterm-256color`；Telnet 裸 `LF`/`CR`/`NUL` 不被应用层伪造成 SSH PTY 行规程，Serial 的无 TERM/PTY/NAWS 边界和 Telnet 可选扩展关闭状态已同步记录。

- 已完成 KEYSTD1–3：终端字符键优先使用 Winit/原生事件的布局解析文本，逻辑键只在事件没有文本时回退；物理数字小键盘移除 application-keypad/SS3 专用拦截并回归标准文本/逻辑键路径；补充 Shift 字符、组合输入、F1-F24、导航键、NumLock/IME/AltGr 和无稳定 xterm 定义组合的回归。独立修饰键、带修饰 Escape/Tab/Return 不误发私有扩展序列；Kitty keyboard/CSI-u 与 `modifyOtherKeys` 继续关闭。
- 已完成 KEYSTD4：删除物理数字小键盘的 application-keypad/SS3 专用 hook、终端 keypad 类型和 mode 编码参数；普通小键盘、NumLock、IME、AltGr、快捷键及带修饰输入统一回归标准 Slint/Winit 文本或逻辑键路径。

- 已为 SFTP 内部拖动增加 `ax_ssh::sftp_drag` debug target：记录 Local/Remote 来源、开始、copy/非 copy 结束、远端落点收到/解析载荷、目标确认、本地文件校验与上传入队结果；字段只含固定阶段、面板、文件数和字节数。
- 已从用户的最新运行日志确认：内部 Local-to-Remote 已经到达 `upload-queued`；Finder 的尝试没有到达既有 `sftp.drop-native-file` action，因此 SFTP worker、路径校验和上传队列不是该失败点。
- 已完成 MACDROP3：macOS 每次收到原生 `DroppedFile` 都以当前 AppKit 位置命中 Slint 的 Remote files 区域，不再因缺少前置 `HoveredFile` 而拒绝；仍严格拒绝无位置、未启用、非 SFTP 或其他区域。`ax_ssh::sftp_drag` 现可在上传前记录原生事件、位置可用性和固定目标结果，不含路径、主机、坐标或秘密。
- 用户最新复现已确认 `DroppedFile` 和 AppKit 命中均到达，但三次均被 Slint 返回的 `remote-unavailable` 拒绝；该结果来自 presentation snapshot，而 Rust 上传入口仍有独立的实时连接、loading 和远端目录校验。
- 已完成 MACDROP4：原生路由只以可见 Remote files 的有限几何选择目标；`handle_native_dropped_file_on_remote_pane` 在入队前重新校验当前窗口的 SFTP Tab、连接、loading 和远端目录。被实时状态拒绝时会显示明确状态并记录 `native-upload-target-rejected`，不会静默失败或按活动目录回退。
- 已完成 SFTPTRANSFER1：SFTP transfer 状态增加显式上传/下载方向；上传使用独立的 `Uploading` 活动阶段，状态快照和 Slint 行 DTO 传递 `Upload`/`Download`，列表文件名前固定显示方向，终态继续分别显示 `Uploaded`/`Downloaded`。
- 已完成 SFTP-ID-20260923：将远端目录 request ID 收敛为 AppState 唯一 owner，首个目录页使用无关联事件，普通导航、分页和上传后刷新都端到端传递同一 ID，修复首屏持续显示 `Loading directory...`。
- 已将 SSH profile 的 X11 转发从布尔开关改为专用的 Off、Untrusted (`-X`)、Trusted (`-Y`) 枚举；旧的缺失/true 值迁移为 Trusted，false 迁移为 Off，cookie 与凭据均不进入 profile。
- 已让 Session Editor 只为 SSH profile 提供 X11 forwarding 下拉框；X server provider/path 继续属于全局本机环境设置，不再以其决定某个服务器的 `-X`/`-Y`。
- 已让 `-X` 在 host key 已被接受且认证完成后才创建私有短生命周期 xauth authority，受限 cookie 在 20 分钟后拒绝新的远端 X11 channel；准备失败只报告 X11 不可用，不阻断 shell。Trusted (`-Y`) 仍保持首个 X11 channel 才准备本机 X server/cookie 的惰性路径。
- 已将 SFTP 原生拖放改为目标区域驱动的 copy/drop 路由：Slint 为 Remote/Local files 发布只读、有限的窗口坐标几何 DTO；主窗口和 detached 窗口共享该契约。
- 已删除以活动 SFTP Tab 或当前目录猜测外部落点的上传入口。非 macOS 平台仅在当前外部 hover 的最后 `CursorMoved` 坐标命中已启用 Remote files 区域时才排入上传；macOS 对每个已送达的 `DroppedFile` 读取当前 AppKit 坐标；坐标缺失、加载中或命中其它区域均拒绝。
- 已让 macOS `DroppedFile` 严格 fail-closed：仅使用 drop 时读取的 AppKit 位置；读取失败即拒绝，macOS 不再编译 Winit 光标缓存、`CursorMoved` 事件路径或其回退转换。
- 已定位普通远端下载自动打开的来源：`connection_monitor` 在 `Completed` 后无条件派生后台平台 opener；本轮改为直接结束为 `Downloaded` 并保留本地路径，系统打开仅由用户显式的 Local files 动作触发。
- 已将 macOS file-promise 的接收视图从全窗口收窄为拖动启动时经校验的 Local files 矩形；区域不可用时 Finder 仍可接收 promise，但回拖至 AxSSH 会被拒绝。
- 已复用既有有界 SFTP intent/worker 和安全本地 writer；未改变 SSH host-key 信任、凭据生命周期、远端写入或传输并发上限。
- 已实现启动阶段 Skia 选择失败时的单次 `winit-software` 重试；显式 `SLINT_BACKEND` 仍保持最高优先级。
- 已实现 Automatic macOS 的非秘密 renderer fallback marker：创建、显示或 event loop 向上报告 GPU/Skia/Metal fault 后，下次 Automatic 使用 software；正常 Skia 退出会清除标记。
- 已加入 renderer 请求/实际选择/来源/回退原因、Metal device、实时窗口数及有界 fault 计数；raw Skia stderr shader source 明确标为 application-errors-only，避免把不可观测错误伪造成统计数据。
- 已核对参考实现的行为思路；未引入其依赖、路径或源码。
- 已确认 AxSSH 已有软换行/reflow、宽字符、SGR/X10/UTF-8 mouse reporting、滚轮与可靠/可丢弃 worker 入队的完整边界，故本轮不重复实现这些契约。
- 已将 `CSI ?25l/h` 收敛为纯光标可见性状态；Local、SSH、Telnet 与 Serial monitor 只按 `CSI ?2026h/l` 的标准结束或上游 deadline 控制呈现，parser 和 `PtyWrite` 协议应答仍即时执行，且没有旧 frame 时不发布局部首帧。
- 已恢复左键双击的 URL 优先、否则语义单元选择；URL 可跨连续软换行且排除末尾终端标点，普通选区继续由终端核心的标点、空白、宽字符和配对括号边界决定。相同短序列第三击选中逻辑行，连续软换行的物理行会一并选中，硬换行仍为边界。
- 已实现互斥的 1005/1006/1015 鼠标编码状态、xterm 横向滚轮 6/7、Back/Forward 侧键 8/9 与有界辅助按键 10/11 编码；未分类硬件按键不作不可靠猜测。
- 已修复大量输出 Tab 切换后的视口回退：主屏 Detached `TerminalModel` 在 resize 后恢复有界 `display_offset`，新建 `TerminalPane` 首次 resize 等待两个 frame，避免瞬时最小网格触发重排。
- 已将长文本粘贴接入统一 `KeyboardEvent.is_paste`；终端模型执行 CRLF/LF 到 CR、ESC 清理和 DEC 2004 完整 wrapper，4 MiB 上限跨越四类 transport，底层写出按 16 KiB 分块。

## 验证

- 已完成 MEM1–3：排除未使用的上游 crate `Cargo.lock` 后，vendor 与上游 1.18.1 `diff -ru` 仅 `layer_cache.component_destroyed(component)` 一行；根 `Cargo.lock` 仅改变 Skia 来源。`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 280、应用 268、Doc tests 0）、`cargo build --locked --offline` 均通过；x86_64 macOS 显式 target check、严格 Clippy、build 通过，未在 ARM64 上运行 x86_64 测试。
- 未完成 MEM 运行时对照：补丁尚未安装到正在运行的旧版进程；其相同 pane/尺寸/输出负载下的 footprint、`IOAccelerator (graphics)` 和 AGX texture 数需要新二进制长期采样。
- 已完成 TITLEBAR5：定向 macOS 命中边界测试 3 项、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 280、应用 268、Doc tests 0）及 `cargo build --locked --offline` 通过；两个 macOS target 的显式 check、严格 Clippy 和 build 也通过。Slint 入口重新编译。首次缺失的 Slint 1.18.1 依赖通过本机 7897 代理按锁文件获取，锁文件未改；x86_64 仅编译链接，没有在 ARM64 主机运行测试。
- 未完成：macOS 标题栏空白拖窗、Tab 重排、按钮点击及红绿灯留空/侧栏的实际指针行为，待用户在目标窗口确认。

- 已完成（DRAW1）：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 280、应用 267、Doc tests 0）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py`（474 条）、tracker validator 与 `git diff --check` 通过；`ui/app.slint` 已由 Cargo 重新编译。
- 已完成 SFTP-ID-20260923：定向 SFTP 事件回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 280、应用 267、Doc tests 0）和 `cargo build --locked --offline` 通过；目标平台真实 SFTP 服务器与 GUI 重连由用户验收。
- 未完成：目标 macOS 需重启新二进制，并以 `ax_ssh::sftp_drag=debug` 复现 Local files 到 Remote files 的内部拖放，提供对应日志阶段。
- 已完成：MACDROP3 的 `macos_file_drop_uses_appkit_position_without_hover_state` 定向回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 259、应用 254、Doc tests 0）、`cargo build --locked --offline` 和 `git diff --check`；`ui/app.slint` 已由 Cargo 重新编译。Finder 图形手势仍待用户目标 macOS 复验。
- 已完成：MACDROP4 的 `active_sftp_upload_target_revalidates_live_readiness` 与既有 `macos_file_drop_uses_appkit_position_without_hover_state` 定向回归，`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 259、应用 255、Doc tests 0）和 `cargo build --locked --offline` 通过；`ui/app.slint` 已由 Cargo 重新编译。Finder 图形手势仍待用户目标 macOS 复验。
- 已完成：SFTP bridge 11 项、Winit 外部文件命中 2 项、macOS native drop region 2 项定向回归；`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings` 均通过，`ui/app.slint` 已由 Cargo 重新编译。
- 已完成：`cargo test --locked --offline -- --skip local_pty_output_modes_translate_linefeeds_before_shell_start` 通过，以及 `git diff --check`。未跳过的完整测试会在既有、非本轮改动的 `src/local_shell.rs` PTY 子进程等待中卡住；已用 macOS `sample` 确认阻塞位置，未修改该用户工作区改动。
- 已完成：tracker validator 已运行；本轮新增条目通过，但校验仍由既有 2026-08/09 历史条目格式债务阻断。新增 Markdown 链接均为外部标准来源，未新增相对链接；目标 macOS 的 Finder/外部拖入手工验收待执行。
- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 251、应用 250、Doc tests 0）和 `git diff --check`。
- 已完成：新增 renderer fault 分类、有界错误文本和实际 software 选择回归通过；`ui/app.slint` 已随 Cargo check 重新编译。
- 受阻但不影响本轮实现：tracker validator 已运行，但仍被既有 2026-08/09 历史记录字段/时间格式债务阻断；本轮 RENDERER1 条目字段齐全。
- 未完成：目标平台需要用户在出现或复现 Skia shader timeout 后提供 `ax_ssh::diagnostics` 记录及 stderr，以确认 fault 是否能从 Slint 返回到应用边界。
- 已完成：下载终态和外部文件指针定向回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 257、应用 253、Doc tests 0）、462 条中文翻译检查、Markdown 相对链接检查和 `git diff --check`。macOS Finder 拖入与系统 opener 行为仍需用户手工验收。
- 未完成：本机对 `x86_64-pc-windows-msvc` 的离线 `cargo check` 在 `aws-lc-sys` C 探测阶段因缺少 Windows SDK 的 `stdlib.h`/`windows.h` 终止，未进入 Rust 代码层；Windows CI/目标机仍需完成 check、Clippy、build 和 native test。
- 已完成：MACDROP2 的 AppKit 读取失败与有效位置定向回归；`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 257、应用 254、Doc tests 0）均通过，`ui/app.slint` 已由 Cargo 重新编译；462 条中文翻译、Markdown 新增相对链接和 `git diff --check` 通过。tracker validator 已运行，本轮条目未新增问题，但仍报告既有 2026-08/09 历史记录与 research 的字段/时间格式债务。
- 已完成：X11MODE1–3 的配置迁移、编辑器转换、worker 授权生命周期与受限 xauth 参数回归通过；`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings` 和完整 `cargo test --locked --offline` 通过，`ui/app.slint` 已由 Cargo 重新编译；465 条中文翻译、Markdown 相对链接和 `git diff --check` 通过。tracker validator 已运行，本轮条目字段有效，但仍由既有 2026-08/09 历史记录与 research 的字段/时间格式债务报告失败。
- 已完成 TERMSTD1：`alacritty_terminal` 负责终端网格、scrollback、光标和 SGR 状态；`CSI ?25l/h` 只改变可见性，`CSI ?2026h/l` 才控制有界呈现事务，首帧没有旧 snapshot 时也不提前发布局部内容；协议写回覆盖 ConPTY 光标位置、动态 OSC 颜色、`CSI 14 t` 文本区像素与 `CSI 16 t` 单元像素查询，并保留默认 X10、UTF-8 1005、URXVT 1015、SGR 1006 与像素 1016 编码。
- 已完成 TERMSTD2：`TerminalPane` 将本地网格尺寸、内容区 cell 度量、字符格指针和物理指针坐标通过 UUID 定向 DTO 送入 `AppState`；Local PTY 与 SSH `request_pty/window_change` 同时接收字符和物理像素尺寸，Telnet 保持 RFC 1073 字符尺寸边界，Serial 不伪造远端尺寸。
- 已完成 TERMSTD3：主屏 Detached display offset、备用屏和 resize/reflow 回归保持在 `TerminalModel`；网格从 pane 顶部开始，底部余量不属于字符格或上报坐标；中英文架构/开发说明已改为 `alacritty_terminal` 当前事实，并明确 `vendor/vt100` 仅为历史许可证审计副本，不在 Cargo 依赖图中。
- 已完成：新增 SGR 1016 像素坐标按测量文本区边界夹位回归；`ui/app.slint` 已由 Cargo 重新编译。
- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 266、应用 256、Doc tests 0）、定向 1016 像素坐标回归和 `git diff --check`；`ui/app.slint` 已由 Cargo 重新编译。
- 已完成 STDUI5/STDREP1 验证：F13-F24 定向输入测试、1016 依赖语义和窗口查询回归、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 281、应用 264、Doc tests 0）和 `git diff --check` 通过；本机 `infocmp -1 xterm-256color` 用于序列核对，tracker validator 通过。
- 已完成 TELNETSTD1 验证：Telnet loopback 覆盖 `DO TTYPE`、`TTYPE SEND`、NAWS、IAC 转义和未知选项拒绝；完整 Cargo 门禁与双语能力边界待本轮最终执行。
- 未完成：GUI 视觉、真实终端程序和目标平台 PTY 行为仍待用户验收。

## 风险与阻塞

- Skia 逐组件缓存持有是源码确认的增长机制，但单次旧版 Sample 不能量化其在 1.3 GB 中的精确贡献；Skia/Metal 仍可能有其它保留资源。当前修复未调整全局 renderer 或用户的行缓存设置。
- TITLEBAR5 无代码阻塞；AppKit/Winit 系统拖窗的真实交互仍需用户验收，尤其是空白、Tab、按钮和侧栏的互斥命中。
- DRAW1 无代码阻塞；Canvas 尚未实现，故当前只能使用 item-tree，未来实现前需验证 Slint custom-paint API 和终端交互兼容性。
- 无代码阻塞。当前剩余风险是目标平台上的 GUI 视觉、真实全屏终端程序行为和 PTY 对物理像素尺寸的实际响应，需要用户在新二进制上验收；超过 16 个同批协议回调时仍会按有界背压丢弃超额响应，并通过诊断可见。
- 代码保持安全边界不变：终端协议应答仍经当前 Tab 的有界 worker 回写；不进入 Slint、持久化或日志，也不扩大 SSH host-key、凭据、Telnet 或 Serial 边界。

## 下一步

- 在新构建上保留 GPU 与当前行缓存设置，以相同窗口尺寸、pane 数和持续输出重复采样启动、持续输出和关闭 Tab 后的 `vmmap -summary` 与 `heap -s`；关注 `IOAccelerator (graphics)`、AGX texture 数是否趋于平台期。若需要立即降低旧安装版占用，可在 Appearance 中关闭 Terminal row render cache 并重启应用；Software renderer 可作独立 A/B，但其 CPU 行为不同。
- 在 macOS 主窗口分别拖动红绿灯右侧空白、Tab、Tab 内按钮和侧栏，确认只有两个空白区移动窗口；Tab 重排和按钮动作各自独立。
- DRAW2：先核实锁定 Slint 1.17.1 的 custom-paint API，再实现保持 parser/model/worker 不变的 Canvas 网格原型；该阶段完成前 Canvas 继续禁用。
- DRAW3：覆盖宽字符、fallback 字体、SGR/盒线、选区、IME、hyperlink、光标、Software/GPU backend 与 damage 更新，并进行同负载 A/B；通过后再决定开放选择或默认值。
- 在 Settings > Terminal 中手工确认 OSC 52 开关的 preview/save 行为；分别用默认目标、selection 目标和超过 64 KiB 的远端写入验证接受/拒绝边界。
- 在目标平台用真实 TUI 验证远端写入本机默认剪贴板，以及读取请求的 Allow/Deny/20 秒超时行为；确认 selection clipboard、Sixel、Kitty 和 iTerm2 图形协议仍保持关闭。
- 验证窗口缩放与 detached scrollback：主屏 Detached 保持历史位置，备用屏不做 reflow；改变终端字体或 Retina scale 后，Local/SSH PTY 收到字符和物理像素尺寸，Telnet 仍只协商 NAWS。
- 验证真实终端的 F13-F24、SGR 1006/1016、UTF-8 1005、URXVT 1015、OSC 4/10/11/12 和 `CSI 14 t`/`CSI 16 t`/`CSI 18 t` 查询；确认 `CSI 13 t`/`15t`/`19t` 不被伪造，检查 block/空心 block/underline/beam cursor、hidden text、双/曲/点/虚线下划线的可见效果。

## 最后更新时间

- 2026-09-24 17:43 +0800：完成 SFTPLIVE1；SFTP 重连保留本地路径和选择，同目录刷新保持上传目标，完整离线 Rust 门禁通过；真实服务器与窗口操作由用户验收。
- 2026-09-24 10:19 +0800：完成 TITLEBAR8，将主窗口 Tab 外空白拖窗扩展到 Windows/Linux；用户实际拖动验收待执行。
- 2026-09-24 09:48 +0800：完成 TITLEBAR7 标题栏剩余空白拖窗命中；用户实际拖动验收待执行。
- 2026-09-23 22:59 +0800：完成 TITLEBAR5 的拖动分区实现、本机 Rust/Slint 门禁及两个 macOS target 的 check/Clippy/build；实际拖动由用户验收。
- 2026-09-23 14:55 +0800：完成 SFTP-ID-20260923；修复首屏目录响应被过期 ID 防护误丢弃的问题，并统一普通导航、分页和上传后刷新的 request ID 所有权。全量 Rust/Slint 门禁和 debug 构建通过；真实 SFTP/GUI 由用户验收。
- 2026-09-23：完成 DRAW1；持久化 terminal drawing preference，保留 item-tree 为唯一可选实现，Canvas 尚未实现并禁用。全量 Rust/Slint、474 条翻译和 tracker 校验通过；目标平台 GUI 视觉由用户验收。
- 2026-09-21：完成 STDUI5/STDREP1/TELNETSTD1；F13-F24 下发序列与修饰键编码按本机 xterm-256color terminfo 修正，Telnet TTYPE 回报 `xterm-256color`，协议队列满有诊断，CSI 窗口查询、1016、Telnet/Serial 能力边界形成明确结论；真实 TUI 和目标平台行为仍待用户验收。
- 2026-09-20：完成 OSC52-1–3 与 OSC52READ1–4；设置字段、默认关闭的有界远端写入、带确认的默认剪贴板读取、UI 线程桥接、四类 transport 接入和中英文说明已同步，等待目标平台真实 TUI/剪贴板验收；selection clipboard 与图形协议仍关闭。

## 9 项复核映射

| # | 问题 | 处理安排 | Step |
| --- | --- | --- | --- |
| 1 | Retina/跨屏 scale 变化后仍使用旧物理区域 | `ScaleFactorChanged` 按窗口重新发布逻辑区域，backend 同时核对 layer scale 并使旧 framebuffer 失效 | SPR2 |
| 2 | 每帧重复锁布局 registry 并 clone 多份 region `Vec` | 应用发布前释放 `MutexGuard`；backend 每个 buffer/present 周期只读一次 generation，只有 generation/scale 变化时 clone snapshot 并重建 layer | SPR3 |
| 3 | 应用状态通过 softbuffer 全局旁路耦合 backend | 将内容收窄为 32 窗口、每窗口 64 region 的短生命周期 opaque presentation hint；只在 macOS Software 启用，窗口释放时注销，不传 terminal/session/SSH 状态 | SPR1、SPR2 |
| 4 | `Hash`/`Hasher` 在非 macOS 构建可能成为未使用 import | 对 import 和 native window key 实现使用 `cfg(target_os = "macos")` | SPR2 |
| 5 | registry 没有窗口注销，依赖容量淘汰遗留旧 entry | 新增 `remove_presentation_layout`，覆盖 detached 恢复失败、返回、关闭、批量释放和主窗口退出 | SPR2、SPR5 |
| 6 | pane 移动、notice/grid clip、scale 与 reset 的布局失效不完整 | pane model revision、独立 notice/layout timer、scale refresh、reset 后逐 pane 重注册共同覆盖 | SPR2、SPR4 |
| 7 | 非 Software renderer 仍执行 presentation callback/timer/layout 工作 | AppWindow 显式下发 macOS Software enable，Slint revision/timer/callback 与 Rust bridge 均先门控 | SPR2、SPR4 |
| 8 | block-row 默认值/范围和 settings `set_rows` 重复 | 配置规范化作为应用唯一默认/范围来源，bridge 不再自建默认或 clamp；softbuffer public API 仅保留边界防御，Settings 统一通过 open-window apply 一次更新 | SPR3 |
| 9 | 测试只覆盖旧固定辅助函数，没有覆盖生产 damage/tile 路径 | production `dirty_tile_mask_for_tiles` 直接由 backend 与测试共用，并新增多 pane、裁剪、重叠、generation 去重和注销回归 | SPR5 |

## 本轮实施计划

- `BACKEND1`：固定上游证据和本地补丁边界，确认 1.17.1 的 `region.iter()` 与 macOS softbuffer age/present 行为。
- `BACKEND2`：引入本地 `i-slint-backend-winit` patch，提交实际多矩形 damage，而不是 bounding box。
- `BACKEND3`：核对 macOS `softbuffer` 的 buffer age、Core Animation layer 和 present 语义；实现 vendor-owned 持久 framebuffer、失效状态和固定物理像素 tile 的 damage-aware present。
- `BACKEND4`：补 backend focused 回归、第三方许可和双语架构说明，执行 locked/offline 完整门禁并记录平台验收边界。
- `BACKEND5`：以 Slint pane 几何替换终端区的固定 tile，按可配置终端行高倍数划分 presentation layer；sidebar/tab 保持 fallback grid。
- `XPLAT1`：定义 `Surface::damage_support()` 能力 DTO 和 helper，不改变 `present_with_damage` 的兼容契约。
- `XPLAT2`：为 Win32、Wayland、X11、KMS、Web、Android、Orbital 和 Core Graphics 实现运行时能力映射。
- `XPLAT3`：让 winit software bridge 在 full-frame/lock-time backend 上直接调用 `present()`，并补能力分类回归。
- `XPLAT4`：同步双语架构、项目地图、月度历史和验证边界，完成静态、离线和可用目标检查。

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| BACKEND1 | completed | 锁定 Slint/winit/softbuffer 证据与 patch 边界 | 本机 crate 源码、PR #12758、macOS sample 对照 | Slint 1.17.1 的 winit bbox 与 macOS softbuffer age=0/full CGImage 已确认。 |
| BACKEND2 | completed | winit software 多矩形 damage forwarding | vendored backend compile、locked check | `PhysicalRegion::iter()` 逐项映射为 softbuffer Rect。 |
| BACKEND3 | completed | macOS softbuffer 持久 framebuffer 与 damage-aware CoreAnimation presentation tiles | macOS 条件编译、buffer-age/invalidate tests、locked check | 固定 256×128 物理像素 tile；有效帧 `age() == 1`；首帧/resize/Retina/restore 强制重新绘制和全 tile 提交；不引入应用层 tile/partition。 |
| BACKEND4 | completed | 文档、许可、focused/完整门禁和平台验收说明 | fmt/check/Clippy/test/translation/diff、vendor rustfmt、用户 macOS 视觉确认 | 已撤销会造成坐标错位的 tile backend；CPU/footprint A/B 仍单独评估。 |
| BACKEND5 | completed | 每窗口有界 pane geometry DTO、1-16 行设置和 row-aligned Core Animation presentation layer | config normalization、Slint 编译、backend partition test、Cargo check/Clippy/test、翻译和 diff | 默认 4 行；终端 layer 横跨 pane，不跨 pane；sidebar/tab/空白区使用 256×128 fallback；新布局视觉验收待用户执行。 |
| XPLAT1 | completed | 定义 `DamageSupport` 枚举、partial/full helper 和 `Surface` 查询入口 | softbuffer focused unit test、Rust API 审阅 | `present_with_damage` 保持向后兼容；能力只描述当前 surface 的消费方式。 |
| XPLAT2 | completed | 实现各平台 backend 的矩形、bounding、tile、driver-dependent 和 fallback 映射 | macOS Core Graphics 编译；可用目标的 cross-check | 非当前目标平台由对应 cfg/CI runner 编译；KMS 运行时收益仍由驱动决定。 |
| XPLAT3 | completed | winit 按能力选择 damage 提交或完整提交 | winit/softbuffer compile、能力分类测试 | full-frame 与 lock-time 不构造无效的局部提交；空 damage 和 age=0 首帧语义不变。 |
| XPLAT4 | completed | 双语契约、项目地图、月度记录和质量门禁 | tracker、Markdown、fmt/check/Clippy/test/diff 检查 | 目标平台 GUI 与真实 driver 行为仍属于平台验收，不由离线构建替代。 |
| WS1 | completed | 为有界 workspace 快照增加菜单保存/打开、路径弹层和异步替换流程 | workspace persistence tests、Slint 编译、完整 Rust/Slint 离线门禁、目标平台菜单与多窗口视觉验收 | 快照只含 Tab/layout/有限终端文本，不含凭据或 live handle；打开前先停止旧 worker，文件 I/O 不阻塞 UI。目标平台菜单、路径输入和 detached 多窗口视觉仍由用户验收。 |
| FONT1 | completed | 修复自带终端字体异步注册后粗体布局仍复用普通回退字体的问题 | Fontique 字重注册回归、Slint 编译、完整 Rust/Slint 离线门禁、`git diff --check` | 字体注册代次驱动终端 `font-weight` 绑定和工作区刷新，同时同步主窗口与 detached 窗口；不改变字体族选择或字体资源。 |
| KPAD1 | completed | Windows 物理数字小键盘映射、终端 application-keypad 模式读取和 DEC/xterm 应用小键盘编码 | 输入/编码/终端模式定向回归、host Cargo check；Windows target 仍待 CI/目标机 | 补充目标 ID：`20260901-windows-keypad-input`；只在远端 `ESC =` 模式且无修饰键时截获，普通 NumLock/IME 路径保持不变。 |
| KPAD2 | completed | 双语输入契约、项目地图、月度记录和离线质量门禁 | fmt/check/Clippy/test、tracker/Markdown、`git diff --check` | 不改变 SSH transport、host-key trust、凭据或持久化。 |
| SHORT1 | completed | 在设置页展示所有应用层快捷键，包括固定的平台快捷键 | Slint 编译、设置搜索回归、翻译检查和完整 Cargo 门禁 | 可配置快捷键保持现有保存契约；固定的 Terminal Select All、Previous Tab、Next Tab 只读展示。 |
| INPUT1 | completed | 统一普通输入框的复制/粘贴入口，并为密码输入提供安全的粘贴菜单 | Slint 编译、输入组件静态审阅、完整 Cargo 门禁 | 普通文本/路径/编辑器支持系统 Copy/Cut/Paste/Select All；SecretTextInput 仍禁止复制，仅允许粘贴，不改变凭据生命周期。 |
| INPUT2 | completed | 统一全应用 Slint/Winit 键盘事件边界并回归标准键盘输入 | 输入归一化回归、Slint/Cargo 离线门禁、双语契约和 tracker 检查 | `KeyboardEvent`/终端上下文 DTO 统一 callback 载荷；`NormalizedKeyboardInput.key` 使用应用级逻辑键，普通文本/IME 不携带物理身份；物理小键盘统一走标准文本/逻辑键路径，`TerminalKey` 只在终端编码边界生成。 |
| CRED1 | completed | 加密保险库缺少用户口令时生成隐藏逐服务器解锁密钥，并同步认证与会话编辑器入口 | 定向凭据回归、Slint 编译、翻译检查、fmt/check/Clippy/完整 test、`git diff --check` | 默认使用应用私有 `0600` 文件自动解锁，不访问系统密钥库；旧 profile 仅在本地解锁文件缺失时迁移一次旧 keyring 条目。随机解锁密钥不进入 profile JSON、UI 或日志。 |

- `ROWMODEL1`：保持单层 `TerminalRenderLine` 和 nested run/background/decoration model 的稳定 identity。
- `ROWMODEL2`：移除应用层 tile/partition 链路；旧配置字段由 Serde 忽略，不做 schema migration。
- `ROWMODEL3`：完成双语架构、项目地图、研究和月度历史同步，并运行 tracker/Markdown/diff 检查。
- `ROWMODEL4`：使用上游 `TermDamage` 产生变化行号，普通输出只更新对应行；首帧、视口变化和渲染 key 失效回退整屏可见行。
- `CRED1`：加密保险库保存可省略用户口令；认证弹窗和会话编辑器为每个 profile 生成隐藏随机解锁密钥并单独保存，不把 SSH 密码后端改写为系统密钥库。

## 已完成

- 已分析用户提供的 421 个 1 ms 样本：主线程 DisplayLink/Slint/Skia/Metal 渲染为主要热点，Tokio、PTY reader 和日志线程大部分时间阻塞。
- 已确认采样进程为 `target/debug/ax_ssh`，空闲时无持续忙循环；高占用与终端输出/工作区更新相关，并受 debug 构建放大。
- 已定位活动终端重复 snapshot/render、全窗口 pane 重建、无条件父模型通知、刷新 follow-up、Local/Serial 逐 read 呈现和全行语义高亮扫描等优化点。
- 已完成 Rust/Slint 架构技能、实施跟踪规则和项目环境快速扫描；本机 Rust/Cargo 1.97.1、rustfmt 1.9.0、Clippy 0.1.97 可用，manifest 保持 Rust 2024、MSRV 1.92.0 和 Slint 1.17.1。
- 已固定 release 对照条件：相同 macOS renderer、窗口尺寸、7 个可见 Local pane 和持续输出命令，各采样 10 秒；debug sample 只作为热点定位输入。
- 已删除 Slint 未消费的 `WorkspaceViewState.terminal` 及其 AppWindow 扁平 render 属性；活动页元数据 snapshot 不再构造终端网格，每个可见 pane 仍生成一次完整 snapshot。
- 已用 `UiRefreshBatch` 请求代次替代单 `AtomicBool`：snapshot 前的请求合并进本轮，snapshot 后的请求才补排；full refresh 覆盖旧 dirty 集合，terminal-only refresh 只构造当前可见 pane tree 中命中的 UUID。
- 已让 nested render-line/cursor model 直接发布变化；浅层 pane/divider 不变时跳过父 `set_row_data`，普通终端 snapshot 不再构造或应用 SFTP rows。
- 已将 Local/Serial 的 parser/协议应答与 UI 呈现解耦：首个输出可立即发布，持续输出按 16 ms、MissedTickBehavior::Skip 合并；错误、断开和 shutdown 保持即时路径。
- 已让 `TerminalModel::snapshot()` 消费上游 `TermDamage` 并复用稳定 `Arc<TerminalStyledLine>`；只有 damage、尺寸或 display offset 需要时才重建有界可见行，删除未消费的扁平 `TerminalSnapshot.text`，并移除逐 cell 临时字符串分配。
- 已让 renderer 按 64-bit 行 revision 和覆盖色表/主题/亮度/粗体/语义设置的 64-bit key 复用 Slint 行；语义高亮改为单次 token 扫描，同时保留 `timed out` 短语和既有优先级。长期运行的行 revision 已从 32-bit 提升为 64-bit，并拆成两个 Slint `int` 防止低位回绕误命中。
- 已生成 `target/release/ax_ssh`（36,360,352 bytes，SHA-256 `45506a8b2c1600d7e1a23e0e32d3cc977a7123f657a103f6df37db4e58a10791`），作为 PERF8 的优化后采样候选。
- 已复核用户提供的 `12:54` sample 和同 PID 的额外 10 秒 sample：PID 89908 仍为 `target/debug/ax_ssh`，与 `11:01` 基线均只能作 debug 结构性对照。按主线程样本归一化，旧样本、`12:54` 短样本和新 10 秒样本的空闲占比分别为 20.0%/68.6%/58.7%，DisplayLink/渲染占比为 59.1%/29.4%/39.5%，Slint item 渲染为 39.9%/20.1%/28.8%，文本渲染为 22.6%/11.9%/16.4%。
- 已确认新 10 秒 debug sample 中 workspace refresh 约为 0.6%、run-model update 约为 0.1%；刷新/snapshot/model 优化已生效，剩余成本主要位于 Slint/Skia/Metal 终端文字绘制。诊断期间 debug 进程约为 28%-35% CPU，但旧基线只有 421 ms 且没有可比 CPU meter，不将该比例解读为总 CPU 降幅。
- 已确认用户 `13:11` sample 来自 PID 91806 的已验证 ARM64 release 二进制，并在同一进程补充 10 秒 sample 和同步 CPU meter。主线程 7,215 个样本中 run-loop 空闲约 67.0%、DisplayLink/渲染约 31.8%、Metal 路径约 31.4%、Slint 组件绘制路径约 24.1%。排除 `top` 首次 0% 初始化读数后，9 次读数平均 27.6% CPU，范围 18.9%-35.6%；physical footprint 约 290.5 MiB，峰值 331.1 MiB。Tokio、日志、7 个 PTY 和 reader 线程大部分时间阻塞。
- 已将 application-owned Local/Serial 持续输出呈现节拍从 16 ms 改为 33 ms；Tokio interval 仍保留立即首 tick 和 `MissedTickBehavior::Skip`。paused-time 回归确认 32 ms 时第二 tick 未完成、33 ms 时完成。SSH/Telnet 16 ms/16 KiB worker 批次、parser、协议应答、错误、断开和 shutdown 路径不变。
- 已生成 33 ms ARM64 release 候选（36,360,352 bytes，SHA-256 `b46058c2c03224449c06ee45a038cc69100d6e665579a70213f59856fb48889d`）。构建时 PID 91806 仍映射旧 inode 16352449，新文件是 inode 16354239；必须重启才会进入新候选。
- 已确认用户 `13:33` software sample 的 Mach-O UUID `9EDF8F81-7062-35AC-820A-221B3CAFEA06` 与当前 33 ms release 完全一致。2,248 个主线程样本中 DisplayLink 占约 67.7%、run-loop 空闲约 31.9%、Core Animation 提交约 36.8%、其中 vImage 颜色转换约 24.9%，Slint software 绘制分支约 25.8%、组件遍历约 24.6%；physical footprint 为 130.3 MiB、峰值 191.3 MiB。日志、Tokio、7 个 PTY 和 reader 线程仍主要阻塞。
- 已完成 CRED1：缺少用户保险库口令时，认证弹窗和会话编辑器为 profile 生成随机隐藏解锁密钥并保存到应用私有 `0600` 文件；SSH 密码仍写入加密保险库，随机值不进入 UI、profile JSON 或日志。旧 profile 仅在本地解锁文件缺失时迁移一次旧 keyring 条目；新的应用设置默认使用 `encrypted-vault`。
- 已从锁定依赖源码确认原版 macOS `softbuffer` 0.4.8 每帧报告 buffer age 0，`present_with_damage` 忽略 damage 并将完整 `CGImage` 设置到单个 `CALayer.contents`；本地 patch 现在保留持久 framebuffer、`age() == 1` 和失效传播，并把 damage 映射到有界 Core Animation presentation layer。终端 pane 按设置的终端行高倍数分区，sidebar/tab/空白区使用固定 256×128 物理像素 fallback。winit patch 保留 Slint 产生的每个独立 damage rectangle，不再先合并 bounding box；macOS backend 只替换相交 layer。
- 已用统一 `TerminalPresentation` 接入 Local、Serial、SSH 与 Telnet monitor：无 dirty 输出时不创建 timer deadline；focused 首个脏更新立即呈现，连续输出前 500 ms/到 2 秒/超过 2 秒分别采用 16/33/50 ms，安静 250 ms 后重置；活动 split tree 中未聚焦 pane 按 Appearance 的 FPS 上限呈现（默认 4 FPS，范围 1-120），隐藏 Tab 无 deadline。`WindowRouter` route revision 和 policy watch 会唤醒有 pending 输出的 monitor，焦点、Tab 或设置变化后立即按新策略重算；SSH 合并批次保留最早 `received_at`，parser、协议应答、错误、断开和 shutdown 仍走即时路径。
- 已生成双策略 ARM64 release 候选（36,376,960 bytes，inode `16356094`，SHA-256 `f419bfbcf7b50e3431062b7b78d5b3053e238265dd6133b5c5a23814ec8d291f`，Mach-O UUID `792FB118-6118-31F4-9359-CA56B5692B8D`）。检查时运行中的 PID 94454 仍映射旧 inode `16354239`，必须退出并重启后才会运行新候选。
- 已将 schema 提升到 25，并贯通默认开启的 `terminal_compact_rendering` 与默认关闭的 `terminal_row_render_cache`：Settings 草稿可即时预览并在关闭时保存，所有存活窗口同步更新；旧文件缺字段时采用默认值。
- 已让 Rust renderer 为每个有界可见行生成合并后的非默认背景 span 和 underline/strikethrough 装饰 span。Slint 紧凑分支直接绘制 Text，旧分支保留为 A/B；可选 `cache-rendering-hint` 只包住静态行内容，选区、光标、目标高亮和 IME/preedit 留在层外。
- 已生成可配置渲染优化的 ARM64 release 候选（36,492,784 bytes，inode `16358516`，SHA-256 `ca1cffe72761baa1c481e9601ff8e07b6f18d5c7f749eaa5c910ad2bcc9a09b6`，Mach-O UUID `8ECE3718-6E3D-370B-94F5-193A455BE533`）。检查时没有运行中的 AxSSH 进程，下一轮可直接启动该候选。
- 已将 `terminal_compact_rendering` 与 `terminal_row_render_cache` 两个开关从 Settings > Terminal 移到 Settings > Appearance 的 RENDERING 分组，与 Renderer 选择同区；Settings 搜索目录和双语 usage 文档同步改为 Appearance 归属，配置字段与默认值不变。
- 已将聚焦与可见未聚焦终端呈现周期改为 `focused_terminal_refresh_fps` / `unfocused_terminal_refresh_fps`，schema v26 默认分别为 60/4 FPS，范围限制为 1-120；Appearance > Rendering 使用 SpinBox，Settings preview/save 同步所有窗口并通过 `WindowRouter` policy watch 立即唤醒 pending monitor，聚焦连续输出的 16/33/50 ms 自适应仍保留。
- 实际 `winit-software` renderer 的可见输出不再使用固定 FPS timer 或窗口物理像素/分屏档位：输出立即进入 `AppState` 单槽 refresh gate，UI 取 batch 前的请求合并到同一批，snapshot 构造后才到达的输出最多补排一次。pending snapshot 在 UI 消费时与更晚的 `TermDamage` 合并，保留脏行并让 UI 看到最新状态；无 dirty row 且 cursor/viewport/mouse-reporting 未变的初始 snapshot 不进入 Slint queue。GPU 和其它 renderer 保留 16/33/50 ms 与持久化 FPS 上限。macOS CoreGraphics 现在只为 damage 相交的 presentation layer 创建独立 `CGImage`，各 layer 的 provider 仍需独立所有权以覆盖 Core Animation commit 后异步读取；若要移除这次 layer copy，仍需 IOSurface/Metal 或可回收多缓冲设计。
- 已新增默认开启的 `terminal_cursor_blink` Appearance 设置，贯通 serde、Settings 预览/保存、主窗口与 detached 窗口；关闭后停止光标闪烁 Timer 并保持光标显示，重新开启时立即恢复可见，不影响终端 cursor visibility、IME 或选区。
- 已将原生窗口激活纳入终端呈现路由：Slint `WindowActiveChanged` 事件作为快速路径，macOS UI 线程每 100ms 读取每个 `NSWindow.isKeyWindow()` 兜底，并通过 `WindowRouter` route revision 唤醒 pending monitor；窗口失焦时该窗口所有可见 pane（包括最后保持焦点的 pane）使用 `unfocused_terminal_refresh_fps`，重新激活后恢复 focused/unfocused pane 分类，隐藏终端和 parser/协议即时路径不变。
- 已将 Tokio runtime 改为显式有界配置：按 `available_parallelism` 取 2-4 个 async worker，blocking 池最多 8 个，blocking 线程空闲 2 秒后允许退出；启动日志记录实际 worker 上限。SFTP 图标预热目标改为 `Weak<AppState>`，最后一个 SFTP Tab 清理扩展 icon 时记录释放数量，迟到 generation 不再持有强状态引用。
- 已复核用户 2026-08-22 19:40 sample：PID 83798 的 Mach-O UUID `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC` 与当前 `target/debug/ax_ssh` 一致，启动日志确认 `worker_threads=4`、`max_blocking_threads=8`；旧样本的 10 个 `tokio-rt-worker` 降为新样本的 4 个 `axssh-tokio`（栈显示为 Tokio blocking pool）。但新样本仍是 `WinitSoftwareRenderer`，本机配置为 `renderer_preference=software`、`terminal_compact_rendering=false`、`terminal_row_render_cache=false`，且旧样本包含本地 PTY 线程、新样本没有，不能作为同负载 renderer/刷新 A/B。新旧 footprint/peak 分别为 128.0/176.7 MiB 与 150.2/183.2 MiB；新样本主线程仍出现 42 个 DisplayLink、35 个 software render 样本，说明软件 surface 整帧路径仍是主要瓶颈，不表示 runtime 生命周期优化未生效。
- 已复核用户 2026-08-22 22:24 sample：PID 89392 的 Mach-O UUID `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC` 与当前 debug 二进制一致；physical footprint/peak 为 229.4/267.2 MiB。主线程 2,254 个样本中约 1,952 个在 CoreFoundation 等待，272 个进入 DisplayLink，271 个进入 Slint Skia `MetalSurface::render`，223 个执行 dirty-region `draw_contents`；4 个 `axssh-tokio` 线程均主要阻塞或等待任务。该样本确认当前运行的是 GPU/Skia/Metal renderer，不是 SoftwareRenderer。
- 内存/线程生命周期机制不按 renderer 分支：Tokio async/blocking 上限、blocking 空闲回收、SFTP 图标缓存清理、`Weak<AppState>`、session/PTY/SSH/SFTP worker shutdown 以及 Rust/Slint 对象 drop 都应对所有 renderer 生效。区分 Software 与 GPU/Skia/Metal 仅用于解释 renderer 自身的 framebuffer、Skia surface、CAMetalLayer drawable、Metal command buffer、Fontique、CoreAnimation 和 allocator 缓存；这些平台级缓存即使 Rust 对象已 drop，也不保证 RSS 立即下降。
- 最新 sample 的 footprint 高于旧 Software sample，不能直接归因于生命周期失效：两者运行时长、负载、renderer 和可见线程条件不同；最新 sample 还是 debug 构建。单次 footprint/peak 不能证明泄漏，必须用同一 release、同一 renderer、同一窗口/pane/负载重复至少三轮，并结合 `vmmap -summary`、线程数和打开/关闭 Settings/Terminal/SFTP 前后对照。
- 已复核用户 2026-08-22 22:38 Software sample：PID 90375 的 UUID 仍为 `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC`；physical footprint/peak 为 120.4/175.1 MiB。主线程 2,033 个样本中约 1,322 个进入 CoreFoundation source、1,320 个 DisplayLink、1,185 个 `WinitSoftwareRenderer`、1,172 个 `draw_contents`，966 个进入 software `render_buffer_impl` 的 buffer 遍历；4 个 `axssh-tokio` 仍存在，且样本也包含 SSH session/monitor 与 UI refresh 调度。与 22:24 GPU sample 相比，Software 的 CPU 整帧路径明显更热、footprint 反而更低；由于启动时长、窗口/pane 和输出量未完全固定，这只能确认 renderer 行为差异，不能作为严格内存 A/B 或泄漏结论。
- 已确认应用退出与 detached 窗口关闭前需要显式清空 Slint model、编辑器文本、SFTP/Terminal 行和安全提示字段；这些应用拥有对象由 Software/GPU 共用的窗口清理函数释放，renderer surface 随 window adapter drop 释放，平台级缓存的 RSS 归还仍不作即时承诺。
- 已实现 `release_window_resources`、`release_detached_windows` 和退出前显式 drop：Return/Close 立即清空 detached UI 并移除 map 强引用；退出先停止 worker，再清理 detached/main window、图标缓存和 UI callback 引用，最后 shutdown Tokio。
- 已完成字体资源审计：JetBrains Mono 四字重约 1.1 MiB 且内嵌；Iosevka Term 四字重约 18.8 MiB、Maple Mono NF CN 两字重约 40 MiB、Monaspace Neon Variable 约 1.6 MiB，均按选中 family 懒加载。Fontique shared collection 没有可靠的运行时卸载契约，已将其视为进程级缓存边界，不做伪释放。
- 已将外部自带字体从 `fs::read`/memory `Blob` 改为经过大小校验的 `PathBuf` source；Fontique 注册阶段使用 mmap，字体数据按需由路径 source/cache 提供，JetBrains Mono 仍使用嵌入 bytes。Maple 注册后通过 family ID 保持唯一 Hani fallback，避免约 39.3 MiB Maple heap 副本由应用加载任务长期持有。
- 已完成本轮 resize 路径审计：Slint pane 已有 16ms latest-size timer，但 resize callback 仍触发完整 active/workspace snapshot；AppState 与 SSH/Telnet/Local worker 对同一尺寸缺少统一入口去重。
- 已完成 resize-only 刷新：成功的 `resize-terminal` callback 通过 `dispatch_terminal_snapshot` 进入现有 dirty terminal gate，只更新当前可见 pane；结构变化仍走 full refresh，隐藏 pane 不进入 UI event loop。
- 已完成尺寸去重：`TerminalModel::size()` 提供当前规范化尺寸，AppState 在 worker 请求前短路同尺寸请求；selection revision 只在真实模型变化时推进，worker 请求失败不改变本地模型。
- 已完成 worker 去重：SSH/Telnet 使用 `watch::Sender::send_if_modified`，Local 使用请求尺寸与 pending latest-value 合并并只在有效变化时唤醒 PTY worker；Serial 继续没有 PTY resize 通道。
- 已删除应用层终端 tile/partition 链路：`TerminalGrid` 恢复单层 `render_lines` repeater，Rust 只保留按行 revision/render-key 的 outer line 与 nested run/background/decoration model 复用。
- 已将终端选区改为固定逐格 fill；每个选中列严格使用统一逻辑 cell geometry，不从混合 Unicode advance 推导背景边界。固定行 tree、稳定 nested model identity 和行 revision 复用继续由 Skia/Software 共用。
- 已修正 Local selection priority 下的 reporting 单击：普通左键按下先保留为本地拖选候选，若释放前未跨格移动，则在释放时向已启用 mouse reporting 的 TUI 发送 press/release；一旦移动仍保持本地选区，Alt/Option 和标准 xterm 模式的既有路由不变。
- `TerminalModel` 的 `TermDamage` 仍只负责终端 snapshot 的受损行重建；Slint renderer 的内部 dirty region 仍由框架负责，二者不再被包装成应用层 tile partial-present 链路。
- 已实现 macOS Software 持久 framebuffer 与 pane-aware CoreAnimation presentation layer：`TerminalPane` 发布窗口相对逻辑几何和实测行高，Rust 按 Retina scale 转为物理区域，backend 在每个终端 pane 内按 1-16 行倍数分带并让每带横跨 pane；sidebar/tab/空白区域保留 256×128 fallback。`present_with_damage` 仅复制 damage 相交 layer 的行并创建独立拥有的 `CGImage`，未变化 layer 保留前帧图像。有效帧返回 `age() == 1`；首帧、resize、DPI/设置/分屏几何变化、occluded/restore 和 surface invalidate 强制重建或完整更新。
- 已完成 session `01a03ba9-f5bb-7cc0-a7c9-021688da1471` 的 9 项复核修复：Retina scale 重算、布局 registry 主动注销、单周期 generation 读取、macOS cfg/Software 门控、pane/notice/reset 失效、Settings 重复更新清理、生产 damage/multi-region 测试和几何 bridge 边界文档均已落地；未改 terminal parser、worker、SSH trust、凭据或 FPS 策略。
- 已新增独立的 `ax_ssh-crash.log`：进程 panic hook 同步记录 panic 消息、位置、线程/进程/平台元数据、renderer 环境和强制 Rust backtrace，随后仍调用默认 panic hook；文件使用私有权限，与缓冲滚动运行日志分离，以保留 Objective-C callback abort 前的首段报告。
- 已完成 COLOR1：macOS CoreGraphics backend 将 `CGColorSpace::new_device_rgb()` 改为显式 `kCGColorSpaceSRGB`，不改变 BGRA/32-bit bitmap 声明、tile 几何、damage mask 或图像独立所有权；同负载 sample 显示 ICC/vImage 仍存在。
- 已完成 COLOR2 采样：目标 sample UUID `75D434E2-B1D0-3C6A-ABFB-7504F41A0663` 与当前 `target/release/ax_ssh` 一致；主线程热点仍在 `CA::Render::copy_image`、ICC/vImage。当前 release SHA-256 为 `f838a4c3b9893f93b143091622f7e212e534d1438c52b7bab1347eb9addaacd4`；此前 14:20 记录的 `17C09EF9...` 是更早候选，不是该 sample 的进程。
- 已完成 COLOR3 原型代码并将其提升为默认：现有 pane/fallback layer 几何和排序不变；每个 layer 的 retained delegate 持有同步像素 backing，present 只复制 tile-local damage 并按 Retina scale 调用 `setNeedsDisplayInRect`，delegate 只为 CGContext clip 创建独立图像。`setContents(CGImage)` 保留为显式回退；重建/析构前清除 weak delegate，不直接共享可变 provider 内存。
- 已完成 CELL1/CELL2：TerminalPane 使用主字体 50 个 Latin cell 的 advance 作为唯一逻辑格宽，中文和盒线保持独立 run 并在一格或两格 span 左边缘绘制；TerminalGrid 选区恢复固定逐格 fill。Software presentation 只注册从 pane 顶边开始的完整顶部对齐行，底部小数余量和不足三行的 pane 留给 fallback 分区。
- 已完成 CELL4：TerminalGrid 的 pointer callback 同时携带所在格与最近插入边界；TerminalPane 用行优先半开索引保存鼠标拖选，并只在绘制/复制前转换为同一包含式首末格。单格、反向和跨行拖选共用该路径；远端 mouse reporting、双击单词和三击逻辑行的既有契约不变。
- 已完成 WAKE1-WAKE3：SSH/Telnet 的 16ms flush 只在输出缓冲区非空时启动一次；Local PTY reader 关闭会通过有界命令通道唤醒 owner，空闲 child 轮询从 25ms 降为 1s，并把 25ms 退出确认限制在最多一秒；macOS 激活事件继续走快速路径，`isKeyWindow` 兜底从 100ms 降为 500ms。
- 已完成 XPLAT1-XPLAT4：`softbuffer::Surface::damage_support()` 将 native presentation 能力显式分类为矩形、bounding rectangle、tiles、driver-dependent、full-frame 和 lock-time；Win32/Wayland/X11/KMS/Web/Android/Orbital/Core Graphics 各 backend 返回运行时能力，winit software bridge 对 full-frame/lock-time 路径直接使用 `present()`，不改变既有 `present_with_damage` 兼容契约。

- 已完成 FONT1：保留自带字体族与四种 JetBrains Mono 字重的注册方式；新增字体注册代次，使异步字体加载完成后终端 `font-weight` 绑定失效并重建布局，避免普通回退字体布局缓存吞掉 Bold/Italic；主窗口和已分离窗口同步该代次，字体注册仍不进入终端/SSH 状态边界。

## 验证

- 已完成：SPR1-SPR5；`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 209、应用 200、Doc tests 0）、Retina 换算 focused test、去除未缓存 bench dev-dependencies 的隔离 vendor 单测（8 项）、435 条翻译、Rustfmt 源码差异检查和 `git diff --check` 通过。
- COLOR1 已完成：`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 209、应用 201、Doc tests 0）和 `git diff --check` 通过；`cargo fmt --all -- --check` 仍被仓库既有缺失的 `vendor/softbuffer/benches/buffer_mut.rs` 阻断，已单独用 rustfmt 校验修改后的 backend。
- COLOR3 代码验证：修改后的 backend 已通过独立 rustfmt、`cargo check --locked --offline`、严格 Clippy、隔离 vendor `--lib` 11 项测试、根工程完整 410 项测试和 release 构建；新增测试覆盖跨 tile damage 的局部裁剪、完全无交集矩形、2x Retina point 换算和 macOS tile-local Y 坐标往返。最新 release UUID 为 `3870BAB3-16BE-387A-AAA3-239B2512B595`，SHA-256 为 `7b2bf745ad26f955a90fd2bb31b9ccd67ad7fca7f4b7007a02b6d9aad9df63eb`。隔离完整 doctest 仍被 vendor 既有缺失的 `examples/utils/winit_app.rs` 阻断。
- CELL1-CELL3 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 210、应用 201、Doc tests 0）、Slint `ui/app.slint` 重编译、release 构建和 `git diff --check` 通过。ARM64 release UUID 为 `3FED0FF3-7C78-391C-B769-A04F7EFA6E97`，SHA-256 为 `b16c9387957656b7154dde6c887040ebc5df6b08b449bb6483269344d621745b`；tracker validator 仍报告本月旧历史/research 条目格式债务，目标 macOS 光标、选区、Retina 和 backing-store 视觉由用户确认。
- CELL4 已完成：定向 selection 测试 4 项及 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 210、应用 201、Doc tests 0）和 Slint `ui/app.slint` 重编译通过。
- WAKE1-WAKE4 已完成：Local PTY 定向测试 8 项、Telnet 定向测试 4 项，以及 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 211、应用 201、Doc tests 0）和 `git diff --check` 通过。
- CPU5 已完成：Software presentation 定向测试 3 项、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 212、应用 202、Doc tests 0）、440 条翻译、Markdown 相对链接和 `git diff --check` 通过。ARM64 release UUID 为 `9D2C3BF8-C51E-3E23-940E-F8DC3F49657E`，SHA-256 为 `12593fd84b5bdb361118b01238d8512f78cb3c304deac44ff439543eb189c846`。
- XPLAT1-XPLAT4 已完成：能力枚举/helper、各平台 cfg 映射、winit full-frame fallback、双语架构/开发说明和项目地图已同步；macOS 目标通过本地编译，Windows 交叉检查受本机未缓存 `atomic-waker 1.1.2` 阻断，Linux/Android/Web/KMS/Orbital 需由对应 CI 或目标设备完成编译和运行时验收。
- 已完成 SEC5-SEC11：AppState 的进程级 Tokio persistence gate 串行化 Settings、profile/group/import、认证后凭据与 host-key 等完整 `SessionStore` 写入，per-profile mutation token 保护 profile 专属事务；凭据读取保留硬超时，保存/删除/回滚在软截止后仍等待 blocking 操作完成再释放 gate；凭据引用要求当前密码 profile，revoked 确认先清 pin 再尝试删除撤销记录，known_hosts/config I/O 在 blocking task 执行；自动重连按 profile UUID 触发时读取最新配置，SSH/Telnet/Serial worker 都拒绝陈旧快照，Serial 在异步发现后再次验证。语言即时保存只写语言字段，保持其它 Settings 预览草稿不提前持久化。同步双语契约、项目地图和月度记录。
- 未完成：Windows offline check 因本机未缓存 `atomic-waker 1.1.2` 未执行。tracker validator 本轮 current/新增记录字段完整，仍报告既有 2026-08 历史与 research 条目格式问题。目标 macOS GUI/Retina 拖选视觉和同负载 CPU/footprint A/B 由用户执行。

- FONT1 已完成：Fontique 四字重注册测试、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 215、应用 212、Doc tests 0）和 `git diff --check` 通过；目标平台粗体/斜体视觉仍需用户验收。
- CRED1 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 227、应用 218、Doc tests 0）和 `git diff --check` 通过；当时用户日志对应 profile 仍显式引用 `system-keyring`，不会自动迁移。后续 schema 29 已将加密保险库和应用私有文件自动解锁设为默认，重新保存时仍可显式选择系统密钥库。

## 风险与阻塞

- 当前 worktree 已包含用户未提交的终端边距、几何和双语文档改动；本目标必须在其基础上追加并避免覆盖。
- UI 刷新批处理只能延后呈现通知，不能延后 terminal parser、PTY protocol response、错误、退出或 shutdown。
- 脏终端路由必须在 pane 转移、split、关闭和 detached/return 结构变化时回退到完整刷新，避免更新错误窗口或陈旧 UUID。
- release 为 strip-symbols 产物，系统级 DisplayLink/Metal 和线程阻塞结论可靠，但不能从该二进制的 sample 进一步精确拆分应用内部 Rust 函数。
- GPU/其它 renderer 的 focused 持续输出仍按 16/33/50 ms 与 Appearance FPS 上限节流；Software 改为单槽 latest-frame 背压，该呈现调度与应用层 tile/partition 已解耦。
- macOS software surface 现在通过 pane-aware CoreAnimation presentation layer 提交 Slint damage 相交区域；CPU framebuffer 持久复用，但每个被更新 layer 都有独立 `CGImage` 后备，不能让 compositor 与下一帧写入竞争。Software 不以窗口像素/分屏数强制限速；GPU/Skia 路径仍按普通 CAMetalLayer drawable present，需分别 A/B。
- 双呈现策略必须按当前 `WindowRouter`/`PaneTree` 动态读取焦点，不能在 worker 中缓存 pane 归属；Software 可见输出立即进入 latest-frame gate，GPU/其它 renderer 在新 deadline 内追上，隐藏 Tab 仍不应进入 Slint event queue。
- 窗口资源清理必须先停止应用 worker，再清空 detached/main 的 Slint model 并移除强引用；不能把清理延迟到 timer 或依赖函数作用域自然 drop，否则窗口 renderer surface 和 model 可能继续存活。
- rust-skia release build script 在 release cache 缺失时会绕过 Cargo offline 语义尝试下载预编译包；本轮最终构建复用了本机同名、同 SHA-256 的 debug cache，未把该缓存加入 Git 或项目依赖。
- resize-only 刷新仍必须在 pane 转移、split、关闭和 detached/return 结构变化时由 WindowRouter 回退到 full refresh；本轮不改变该结构变化语义。
- 应用层 row model 更新只负责减少 Slint item/model 工作；macOS softbuffer 将 winit 物理 damage 映射为 backend presentation tile，但不把 `TermDamage` 变成应用层 tile 或改变 UI 坐标。升级 Slint/softbuffer 后仍需重新核对上游 damage、buffer age、CoreAnimation layer 和 DPI 语义。
- Slint 1.17.1 的 `cache-rendering-hint` 只在 Skia/FemtoVG layer renderer 中保留离屏图像；software renderer 不提供等价 layer cache。缓存必须只包住静态终端行内容并默认关闭，避免光标闪烁或选区变化使所有行缓存失效，也避免未测量的 Retina 纹理占用成为默认成本。
- COLOR1 的 sRGB sample 已完成；`CA::Render::copy_image`/ICC/vImage 仍存在，只能说明色彩空间提示未消除 CA 的内部准备成本，不能据此宣称局部 backing store 无效。`setNeedsDisplayInRect` 方案还需保证 delegate 绘制期间读取不可变帧数据，禁止直接改写仍被 CA 使用的 `CGImage` provider 内存。
- `DamageSupport` 是 presentation capability hint，不是各平台性能保证：KMS 由驱动决定，Wayland/X11 能力随运行时协议/共享内存状态变化，Android 仍要求在 lock 前决定 damage。`present_with_damage` 保持安全的兼容入口；新增 winit fallback 只跳过无法消费的局部 damage 生成，不改变 framebuffer、终端模型、凭据或 SSH trust 边界。

## 下一步

- 已完成终端显式 `Follow`/`Detached`/`AlternateScreen` 视口策略和双宽光标跨度快照；Detached 输出保持历史位置，输入回底，alternate screen 清理本地滚动状态。中文续格光标归一化到首格并按两格绘制。返回底部/未读输出 UI 尚未增加，目标平台视觉验收待用户执行。
- 用相同窗口、pane 数、renderer 和持续输出对照当前单层行模型与 GPU/Skia；不再维护旧 tile/partition A/B 组合。
- 在目标 macOS 上先验收 1/4/8/16 行设置、sidebar 展开/收起与拖宽、横竖分屏、detached 窗口、Retina scale、resize/隐藏恢复的边界和内容顺序，再进行同负载 GPU/Skia 与 Software 对照。
- 构建同一 release 后，以默认 damage backing store 和 `AXSSH_EXPERIMENT_CA_BACKING_STORE=0` 的 layer-image 回退分别验收首帧、1/4/8/16 行、sidebar 状态/拖宽、分屏、Retina、resize、隐藏恢复、滚动、光标和持续输出；后续 release 仍用相同窗口/pane/负载记录 CPU meter 与 10 秒 sample，防止性能回归。
- 若普通 delegate 再次出现整块 CA copy 或视觉回归，先切换到 layer-image 基线分析 sample；`CATiledLayer` 的异步延迟和禁止直接设置 `contents` 使其只能作为后续独立实验。
- 在目标 macOS 上先复验新格宽与 row-origin：长 ASCII、中文/盒线混排、跨行选区、光标闪烁、1/4/8/16 行 block、Retina/resize 和 split 边界；确认无错位后继续 COLOR3 的 backing-store/layer-image A/B。
- 单格拖选需要在目标 macOS 上复验同格跨中线、向左/向右反向拖动、跨软换行与硬换行、双击单词、三击逻辑行，以及 Copy selection on select；视觉与复制内容都正确后再恢复 COLOR3 A/B。
- 使用同一 release 分别记录空窗口、一个空闲 Local、一个空闲 SSH/Telnet 和两个 Local pane 的 Activity Monitor/Instruments Idle Wake Ups；实际系统唤醒会受 Tokio timer 合并和 macOS power management 影响，不用源码 timer 数量直接替代测量值。

## 最后更新时间

- 2026-09-03：完成 CRED1；选择加密保险库但未填写用户口令时，认证弹窗和会话编辑器为 profile 生成随机隐藏解锁密钥并单独保存到系统密钥库，SSH 密码仍保存在加密记录中；更新双语架构说明与界面提示，随机值不进入 profile、UI 或日志。
- 2026-09-01 21:57 +0800：完成 SHORT1/INPUT1；Shortcuts 展示三个固定平台快捷键，普通文本输入和 TextEdit 使用完整原生剪贴板操作，SecretTextInput 仅新增粘贴入口并保持秘密不可复制。完整 Rust/Slint、翻译和差异门禁通过；目标平台输入法、快捷键和菜单视觉待用户验收。
- 2026-09-01 15:55 +0800：完成 MODAL1 阻塞式 dialog 统一与窗口路由锁定；共享 `ModalFrame`、`OverlayHost` 安全优先仲裁和 Rust 侧 Tab/Pane/workspace 动作复核已接通，完整离线门禁通过，目标平台焦点/菜单验收待用户执行。
- 2026-09-01 18:37 +0800：完成 KPAD1-KPAD2；Windows 在远端 `ESC =` application-keypad 模式下编码无修饰物理数字小键盘，普通 NumLock/IME/快捷键路径不变。host 离线门禁通过；Windows target 构建和实机键盘验收仍待 CI/目标机。
- 2026-09-01：修复异步自带字体注册后的终端粗体/斜体布局缓存失效；FONT1 已完成，主窗口与 detached 窗口同步字体注册代次，完整离线 Rust/Slint 门禁通过，目标平台视觉待用户验收。
- 2026-08-28 11:30 +0800：增加 File 菜单 workspace 保存/打开、非阻塞路径弹层、用户路径有界原子读写，以及打开前 worker/probe 清理、Tab UUID 重映射和 detached route 替换；WS1 已完成，目标平台视觉待用户验收。
- 2026-08-28 14:38 +0800：完成 SEC5-SEC11，收敛 session `01a040ac-d2f1-7fc2-b033-cc1c58f5b4ca` 复核出的 credential timeout、Telnet/Serial 旧快照和设置预览语义问题；完整验证结果见本轮月度记录。
- 2026-08-28 09:18 +0800：完成 XPLAT1-XPLAT4。新增 `Surface::damage_support()` 跨平台能力探针和 backend 映射；winit software bridge 对 full-frame/lock-time backend 直接走 `present()`，并同步双语架构、开发文档、项目地图和月度历史。macOS 本地门禁已通过，Windows offline check 受未缓存 `atomic-waker 1.1.2` 阻断，其他平台交由对应 CI/设备验收。
- 2026-08-27 13:49 +0800：按用户完成采样后的选择，将 macOS Software 的 damage backing store 提升为缺失/无效配置默认值；显式保存的 `layer-images` 和环境变量假值仍可回退，不改变 GPU、FPS、终端或 SSH 边界。
- 2026-08-27 13:05 +0800：完成 CPU1-CPU4。schema v27 保存 macOS Software 的稳定图层图像/实验脏区 backing store 选择；Appearance > Rendering、Settings 搜索/保存、中文目录和启动期 softbuffer 开关已贯通，环境变量保留为进程覆盖。完整 Rust/Slint/翻译门禁及 12 项独立 softbuffer 测试通过，等待目标机分别进行视觉和同负载 sample A/B。
- 2026-08-27 11:04 +0800：SSH/Telnet 空闲输出 timer 改为按需一次性 flush；Local PTY 空闲 child 检查降至 1s，reader 关闭即时唤醒并只短暂快速确认；macOS 激活兜底放宽到 500ms。完整 Rust/Slint 门禁通过，等待同 release Idle Wake Ups 复测。

- 2026-08-27 10:03 +0800：鼠标拖选改为最近插入边界和行优先半开索引，最小选区可精确为一个单元格；绘制与复制只消费同一规范化包含式范围，xterm mouse reporting 和语义/逻辑行选择不变。完整 Rust/Slint 门禁通过，等待目标机拖选视觉确认。

- 2026-08-27 09:47 +0800：修复终端光标/选区与文字的格宽漂移，并校正 macOS Software presentation 的真实首行原点。主字体 Latin advance 作为唯一逻辑 cell width，非 ASCII run 仍在固定 span 内居中；选区按 cell 绘制；presentation region 从 `grid-top-offset` 开始，只覆盖完整终端行，顶部余量由 fallback 接管。完整离线门禁和 release 构建通过，等待目标机视觉复验。

- 2026-08-26 18:06 +0800：修复 COLOR3 实验路径的 macOS tile-local Y 坐标。damage 从左上原点物理像素转为 CALayer 左下原点 point，delegate clip 再执行逆变换；完整 block、顶/底边和 Retina 往返测试通过，等待目标机视觉复验。
- 2026-08-26 16:45 +0800：完成 COLOR3 release 候选。ARM64 UUID `647E1512-E87E-38A9-B28F-3CFC98BA0A7F`、SHA-256 `696523a6a94bd8565861283c13289b7912e369a1991360a91a3a53fa5450f92b`；同一二进制可用环境变量切换默认/实验路径，等待目标机视觉和 sample A/B。
- 2026-08-26 16:21 +0800：完成 COLOR3 可回退原型和静态门禁。`AXSSH_EXPERIMENT_CA_BACKING_STORE=1` 为每个现有 presentation layer 接入 retained `CALayerDelegate`、同步像素 backing 和 tile-local `setNeedsDisplayInRect`；默认 `setContents` 不变。focused 测试发现并修复了无交集 damage 在局部坐标减法前被提前求值的下溢问题；目标机视觉/性能 A/B 尚未执行。
- 2026-08-26 15:00 +0800：完成 COLOR3 方法检索。Apple 公开 API 支持 `CALayerDelegate.drawLayer:inContext:` + `setNeedsDisplayInRect:` 的矩形失效；`CATiledLayer` 支持异步 tile 绘制但官方要求不直接设置 `contents`，且更新可能延迟。结合当前 softbuffer ownership，首选普通 delegate backing store，CATiledLayer 只做独立实验。
- 2026-08-26 14:32 +0800：COLOR2 sample 完成。10 秒目标 macOS release sample 与当前候选 UUID 一致；主线程 DisplayLink/CA 图像准备和 ICC/vImage 仍为主要活跃路径，COLOR3 保持待实施。
- 2026-08-26 14:20 +0800：COLOR2 release 候选完成。`target/release/ax_ssh` UUID 为 `17C09EF9-E362-38C9-9F09-07E3A0DB5F0F`，SHA-256 为 `d993796031b219d7a2af05870ffdd33600384691755db4ce2ffb716eb2c4897c`。
- 2026-08-26 14:00 +0800：COLOR1 完成。macOS CoreGraphics image 使用显式 sRGB 色彩空间，未改变像素排列、damage 几何或图像所有权；check、Clippy、完整 410 项测试和 diff 检查通过。
- 2026-08-26 13:35 +0800：完成 software presentation 9 项复核修复及 SPR1-SPR5。macOS Software 专属 bridge 现在覆盖 scale、pane/notice/reset 和窗口注销；backend 每个 buffer/present 周期只读一次 generation，只有布局/scale 变化才 clone snapshot；生产 damage/multi-region/registry 与 Retina 换算回归通过。Software 仍无固定 FPS 上限，目标平台视觉验收待用户执行。
- 2026-08-26：新增 macOS Software row-aligned presentation layout。每个 `TerminalPane` 把 window-relative 几何与实测行高发布到有界 Rust DTO，backend 按设置的 1-16 行（默认 4）划分横跨 pane 的 layer；sidebar/tab/空白区域保留 fallback grid。设置即时同步主窗口和 detached 窗口，不增加 FPS 上限；Cargo check/Clippy/409 tests、翻译和 diff 检查通过，目标平台视觉/性能 A/B 待用户执行。
- 2026-08-25 16:30 +0800：Software renderer 的可见终端输出取消固定 FPS timer 和物理像素/分屏负载档位，改为 `AppState` 单槽 latest-frame refresh gate；UI 消费 pending snapshot 时合并随后发生的 `TermDamage`，且只有 snapshot 构造后的新输出才补排一次。控制序列等没有可见变化的初始 snapshot 仍不进入 Slint event queue；macOS CoreGraphics 的安全图像所有权边界保持不变。
- 2026-08-25：macOS softbuffer `present_with_damage` 改为固定 256×128 物理像素 CoreAnimation tile。持久 CPU framebuffer 仍由 Slint 重用；仅把 damage 相交 tile 的像素行复制到独立 `CGImage` 并替换对应 layer 内容，旧 tile image 留给 compositor 安全持有。首帧、resize、surface invalidate 和 Retina scale 变化仍完整更新；不增加 FPS 限制。
- 2026-08-24：删除应用层终端 tile/partition 链路及配置接线；旧 JSON 字段作为未知字段忽略，当前 UI 使用单层 `TerminalRenderLine` model，保留 `TermDamage`、行 revision 复用和 Slint 内部 dirty region。
- 2026-08-24：撤销 macOS softbuffer 的 512x64 CoreAnimation tile/slicing；保留持久 framebuffer、`Surface::invalidate()`、`age() == 1` 和 age 0 首帧/恢复后的完整 layer present。生产代码通过 vendor/backend Cargo check；目标机仍需按首帧、resize、Retina、隐藏/恢复、滚动、光标和动画清单进行视觉验收。
- 2026-08-24：为 macOS 偶发键盘/IME abort 增加独立 `ax_ssh-crash.log` 同步 panic 报告，覆盖 panic 消息、源码位置、线程/平台元数据和 backtrace；不改变 renderer、终端、SSH 或凭据边界。
- 2026-08-25：`TerminalSnapshot` 新增 `dirty_rows/full_refresh`；普通输出在应用层只渲染并通知受损行，首帧、视口/尺寸变化、full damage 和渲染 key 变化保留整行回退。Slint item-tree 遍历与 backend 完整 present 语义不变。
- 2026-08-23 22:50 +0800
- 计划状态变更：BACKEND1: pending -> completed; BACKEND2: pending -> completed; BACKEND3: pending -> completed; BACKEND4: in_progress -> completed
- 验证结果：本地 backend patch 已清理死代码并通过 vendor rustfmt、locked/offline Cargo 全量门禁、翻译检查和 `git diff --check`；实际 macOS GUI/A-B 仍待用户执行。
- 计划切换：ROWMODEL1-ROWMODEL4 已完成；ROWMODEL3/4 在本轮完成文档同步、增量模型更新和门禁。
- 影响文件：`src/config/{settings,tests}.rs`、`src/app/{settings_bridge,view/settings,view/terminal,view/tests}.rs`、`ui/{app,settings,settings/appearance,workspace-shell}.slint`、`translations/zh-CN/LC_MESSAGES/ax_ssh.po`、双语架构/使用文档和 tracker。
- 计划状态变更：PARTITION1: pending -> completed; PARTITION2: pending -> completed; PARTITION3: pending -> completed; PARTITION4: in_progress -> completed
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 202、应用 197、Doc tests 0）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py`、tracker validator 和 `git diff --check` 通过。
- 对 plan 的更新：设置默认 `tile-8`；逐行/8 行/16 行只改变 UI tile 分组，保留 dirty-row revision、动态 `start_row` 几何以及终端 parser/worker/输入边界。
