[English](architecture.md) · [文档导航](README.zh.md)

# AxSSH 架构说明

## 边界

AxSSH 是一个独立的 Rust 二进制项目。`third_package/axshell` 仅用于参考，
故意排除在构建图之外；可以参考它的产品行为和评审问题，但不得导入其中的
源码、类型或依赖。

当前实现将 UI、应用、持久化、传输和进程服务拆成独立所有权边界：

```text
Slint UI（.slint）
       │ 生成的 callback / property
       ▼
应用控制器（src/app.rs）
       │ Tab ID + 领域值 + UI event loop 调度
       ├──────────────► 配置存储（src/config.rs + src/config/）
       │                 版本化设置/profile JSON + 原子替换
       ├──────────────► 系统凭据（src/credentials.rs）
       │                 阻塞式系统 keyring 与加密保险库 API
       ├──────────────► 终端模型（src/terminal.rs）
       │                 有界 vt100 网格 + scrollback
       ├──────────────► 本地 PTY（src/local_shell.rs）
       │                 有界线程 + portable-pty 子进程
       ├──────────────► SSH 边界（src/ssh.rs）
       │                 Tokio task + russh handle/channel + X11 relay + 私钥加载
       ├──────────────► SFTP 浏览（src/sftp.rs）
       │                 有界 SFTP v3 目录游标 + 分页 DTO
       ├──────────────► Telnet 边界（src/telnet.rs）
       │                 有界 TCP worker + RFC 854 parser + NAWS
       └──────────────► Serial 边界（src/serial.rs）
                         元数据发现 + 有界设备 worker

进程启动（src/main.rs）
       └──────────────► 日志生命周期（src/logging.rs）
                         滚动 writer + flush guard
```

## 模块职责

| 区域 | 负责 | 不得负责 |
| --- | --- | --- |
| `ui/` | 主窗口组合、功能组件、Settings 分类页面、视觉状态、用户手势和生成的 callback 契约 | 文件系统、Tokio task、russh handle |
| `src/app.rs` | 生成 Slint 类型的声明、进程级 UI 启动和顶层 callback 编排 | 功能实现、SSH 协议细节或 JSON schema 细节 |
| `src/app/macos_window.rs` | 主线程 AppKit 标题栏、运行中应用图标和标准应用菜单 action 绑定 | 生成的 Slint 类型、持久化设置、SSH 或 worker 状态 |
| `src/app/{workspace,connection,connection_monitor,terminal_bridge,settings_bridge,view,serial_bridge,sftp_bridge}.rs` 与 `src/app/connection/` | 私有 application bridge 功能接线，包括协议分发、SSH 信任/认证、直连 worker、串口发现和 SFTP 浏览意图 | 生成类型声明、传输实现或持久化 schema |
| `src/app/local_files.rs` | SFTP 本地栏的有界、只读本机目录元数据发现 | Slint 类型、文件传输/修改、持久化或 SSH handle |
| `src/app/state.rs` 与 `src/app/state/` | 与 UI 无关的工作区 Tab、逐 Tab 终端/worker 状态、attempt 转换及测试 | Slint component/model 类型或 russh 协议细节 |
| `src/app/{input,session_groups,terminal_render,credential_tasks}.rs` | 可测试的输入/分组/渲染映射、主题化终端默认色和阻塞式凭据 task 边界 | 窗口所有权、传输 handle 或可变 UI 状态 |
| `src/app/diagnostics.rs` | 脱敏键盘分类、固定 diagnostics route/action 字段和专用 tracing target | 原始终端/剪贴板文本、路径、profile 标签、主机、凭据或传输状态 |
| `src/config.rs` 与 `src/config/` | 稳定的 config 入口和显式导出；session/profile 领域、设置、主题规范化、旧配置迁移、私有 JSON 持久化和原子替换 | Slint 类型、网络连接、明文密码存储 |
| `src/credentials.rs` | 按 profile 访问系统凭据库和加密保险库记录 | UI 状态、明文配置、SSH 传输 handle |
| `src/terminal.rs` 与 `src/terminal/input.rs` | 有界 vt100 网格、字符格样式、光标/scrollback 状态、选区提取和终端按键编码 | Slint 类型、网络 handle、凭据 |
| `src/local_shell.rs` | 跨平台 shell 发现，以及每个 Tab 一个由有界 worker 独占的本地 PTY 子进程 | Slint 状态、SSH 信任、持久化终端内容 |
| `src/x_server.rs` | 平台 X server provider 选项、系统应用发现与标准路径兜底、本机 display 候选和有界进程启动 | SSH channel、UI 状态、cookie、profile 修改或远端服务器配置 |
| `src/ssh.rs` | russh handler、主机密钥决策、认证、shell 与服务端发起的 X11 channel 边界 | 窗口更新、持久化会话修改、UI 格式化 |
| `src/ssh/private_keys.rs` | 本机 `.ssh` 私钥发现和阻塞式密钥加载 | passphrase 持久化、UI 状态、主机信任决策 |
| `src/ssh/x11.rs` | 本机 DISPLAY 解析、精确 xauth cookie 查询、X11 setup 校验/替换、本机端点连接和 relay | UI 状态、profile 修改、cookie 持久化、启动 X server 或修改访问控制 |
| `src/ssh/worker.rs` | 有界 shell/X11 命令、合并式 resize 状态、批量输出事件、relay 取消和关闭 | UI 状态或 profile 持久化 |
| `src/sftp.rs` | 有界 SFTP v3 packet 适配、远端路径校验、目录游标、分页 DTO 和浏览 task 生命周期 | Slint 类型、凭据、profile 持久化或 russh 信任决策 |
| `src/telnet.rs` | 明文 TCP 生命周期、RFC 854 选项过滤、NAWS、有界输入输出、取消和关闭 | 凭据、SSH 信任、UI 状态或终端渲染 |
| `src/serial.rs` | 不打开设备的端口发现、稳定 USB 身份匹配、串口参数映射和单设备有界 worker | 自动打开/探测设备、UI 状态或持久化 profile 修改 |
| `src/logging.rs` | 全局 tracing subscriber、日志目录、按日滚动、保留和 flush guard | 凭据、功能状态、UI 或 SSH handle |
| `src/main.rs` | 进程启动和日志 guard 生命周期 | 功能逻辑 |

## 应用图标归属

`assets/ion/terminal_icon.svg` 是唯一的 canonical 图标源。由它生成的 PNG、ICO 和 ICNS
是构建/打包输入，不属于 UI 或 transport 状态。`ui/app.slint` 选择 256px PNG 作为
Slint/winit 窗口图标；Windows 从 `packaging/windows/axssh.rc` 把多尺寸 ICO 编译进
可执行文件；macOS application bridge 在 UI 线程设置运行中的 Dock 图标，`.app` bundle
则使用 `Info.plist` 指定的 ICNS；Linux package metadata 把 desktop entry 和对应尺寸的
PNG 安装到 hicolor 目录。所有路径都不读取参考工程，也不改变字体作为运行时资源加载的契约。

## Slint 组件状态归属

`ui/app.slint` 导出的 `AppWindow` 是 Rust 唯一直接访问的 Slint 契约。它只负责顶层组合、
跨平台菜单树以及生成的 property/callback；根组件会声明式地把既有 Rust 扁平快照组装成小型
UI DTO，而不是让 Rust 直接访问任意内部 component 实例：

```text
Rust
  <-> AppWindow property / callback
  <-> WorkspaceShell / OverlayHost
  <-> TerminalPane / SettingsPane / SessionEditorPane
```

`WorkspaceShell` 拥有侧栏收起、已保存连接选择器开关、选择器关闭以及侧栏/Tab/内容编排。
它通过只读 `WorkspaceViewState` 接收 Tab、Profile、终端和设置数据，并用 callback 向上发送
激活、关闭、连接、保存和取消等用户意图。`TerminalPane` 接收只读
`TerminalViewState`，只拥有终端局部焦点、IME proxy、选区、光标闪烁和尺寸测量；它不拥有
worker、终端缓冲区或连接状态。
其内部 `TerminalGrid` 接收更小的 `TerminalGridView` 和 `TerminalSelectionView` DTO：它绘制
有界 snapshot，并把指针、滚动和上下文菜单手势转换成 callback；焦点、IME 输入、选区草稿和
resize 生命周期仍由 `TerminalPane` 保留。
它的 `key-pressed` 只把特殊键和终端控制组合键发送给 Rust；可打印字符、Shift 文字和已提交的
IME 文本继续通过原生 `TextInput.edited` 路径。
`AppWindow.log-keyboard-event` 在排除快捷键录制和安全提示后，只上报已处理的临时控件按键。
原生菜单命令改走独立的固定 ID menu-action 路由，因为 Slint 不提供鼠标或 accelerator 来源。
diagnostics 边界把所有文字键或粘贴统一转换为固定 `Text` 标签，只接受白名单 route/action；
原始文本和文本长度都不会成为 tracing 字段。

`SettingsPane` 接收只读 `SettingsViewState`，复制到私有可编辑草稿，并把每轮修改合并为
即时内存预览。预览只更新当前应用、Theme、终端和布局，不读取字体文件或写入配置；关闭意图才
携带稳定的 Settings Tab ID，Rust 在异步资源注册和持久化成功后关闭该 Tab，失败时保留草稿与当前预览。
菜单或原生平台只提供只读的目标 section；用户在设置页导航时的当前 section 由组件自身持有。
`SessionEditorPane` 对 `SessionEditorViewState` 使用同样模式：只有传入的
draft identity 改变时才重置私有字段，用户输入不会反向修改 Rust 快照。`in-out` 仅保留给
同一局部草稿的嵌套编辑控件；显示文案、dialog 文本和视觉状态都用绑定计算，不重复保存。

`OverlayHost` 拥有 Group/Profile 管理弹层的开关和草稿，并从单个 action 派生标题、消息和
按钮表现；只有确认后才向上发送管理命令。它也组合 SSH 主机密钥与认证弹层，但二者是刻意
保留的例外：可见性和 prompt identity 仍是 Rust 安全 phase 的只读输入。UI 只能提交
confirm/reject/authenticate/cancel 意图，不能在 Rust 接受状态转换前自行隐藏弹层。

## 事件流

1. Slint callback 只产生已保存 profile ID、唯一 Tab ID、终端按键/修饰键、
   草稿字段、信任决策或一次性临时秘密等小值。认证秘密经专用
   `SecretTextInput` 传递，应用接收后或用户取消提示时都会清空 UI 中的值。
2. 每次打开 profile 或本地 shell 都会创建新的终端 Tab UUID，即使另一个 Tab 使用
   相同目标。已保存连接的输入、resize、输出、重试和关闭按 `tab_id + profile_id +
   attempt_id` 路由；本地操作按 `tab_id` 路由。未知 SSH 主机会启动绑定该 Tab 的可取消
   探测，但传输仍拒绝。
   工作区 Tab 顺序是仅在内存中的展示状态：拖拽释放会把 Tab UUID 和受限目标位置交给
   `AppState`，它只重排现有 Tab 列表。按住期间 Slint 保留半透明的源槽、高亮目标槽，
   并在指针位置绘制不可交互的 Tab 副本；不会创建第二个运行时 Tab。前置 UI 序号从当前
   列表位置派生，而 `#1` 这类实例后缀仍是稳定标题的一部分。每个 SSH Tab 还独占当前
   连接阶段：idle、可取消的主机密钥探测、等待主机密钥确认、等待认证或读取已存凭据；不再
   存在全局的 probe、信任或认证等待槽位。
3. 用户明确确认后，控制器才原子持久化精确指纹。密码 profile 通过 Tokio blocking
   边界读取已记住的凭据或打开密码弹窗；私钥 profile 在 UI 线程外加载所选路径，
   只有加密密钥无法空口令打开时才请求一次性 passphrase。安全覆盖层只渲染活动 Tab 的
   等待阶段；非活动 Tab 保留自己的提示直到被激活，认证提示切换时会先清空其中的秘密输入。
4. Settings > General 持有新记住 SSH 密码的默认后端：平台系统凭据库或应用加密保险库。
    会话编辑器不接收密码。profile 只会在记住密码成功写入后保存可选的后端引用，因此修改
   默认值不会迁移或破坏既有凭据。应用只在 SSH 认证成功后写入选中后端；后端记录和
   profile 引用任一持久化失败都会一起回滚。删除 profile、切换为私钥认证或拒绝已保存
   密码时，会事务性删除该引用的凭据，但不会停止已经打开的终端 worker。
5. 终端表面把 Slint 特殊键（包括 F1-F12）转换成与 UI 无关的终端键值；平台对
   `Shift+-` 仍上报 `-` 时只在该映射层后备转换为 `_`。`src/terminal/input.rs`
   生成控制字节、普通 CSI 或 application-cursor SS3 方向/Home/End 序列，以及带修饰键的
   xterm 导航/功能键序列。透明、随光标定位的 `TextInput` 是原生文字和 IME 代理：
   特殊键与终端控制组合键走 `key-pressed`，可打印字符、Shift 文字和 IME 提交只通过
   `edited` 进入；预编辑保留在局部 UI 状态。物理 macOS 按键在应用边界先读取 AppKit 当前聚合的
   修饰键状态，再还原 Slint Apple 映射中交换的 Control/Command 语义，因此即使缺少某一侧修饰键
   事件，左右 Control 仍保持一致；已提交的 IME 和粘贴文本显式使用空修饰键，不能继承仍按住的快捷键。
   `TerminalSettings.option_as_meta` 默认关闭，因此 Option
   文字和死键走文本路径；开启后 Option 组合键按终端 Meta 编码。`TerminalGrid` 只会在
   已连接光标可见时显示这份局部 preedit 值；组合文本不会经由它的手势 callback 跨越组件边界。
   Windows/Linux 保持 Alt
   终端输入，同时 Ctrl+Alt 的可打印文字可保留为 AltGr 文本。普通 `Ctrl+C` 保留为 PTY
   输入；终端获得焦点时 Ctrl 组合优先。剪贴板操作在 macOS 保留 `Cmd+C/V`，其他平台
   使用 `Ctrl+Shift+C/V`。工作区命令使用平台主修饰键。选区复制留在 UI，粘贴内容作为
   有界 shell 输入发送；可选右键行为根据是否存在选区选择复制或粘贴。
   键盘路由和主要 application callback 使用专用 `ax_ssh::diagnostics` debug target。特殊键
   使用稳定名称，所有可打印、IME、密码和粘贴文字只记录为 `Text`。功能调用事件只包含固定
   action ID 与结果，不记录 callback 的路径、名称、主机或秘密值。默认 INFO 过滤规则关闭
   该 target，仅在排障时显式开启。
   独立的 `ax_ssh::latency` debug target 只记录本地序号、固定阶段、结果和单调时钟微秒耗时，
   用于测量 UI 到 worker 请求、SSH command 排队、russh 调用，以及远端输出到 UI 调度/应用；
   不记录输入/输出内容或长度。`first-output-after-input` 只是时间上的后续输出观察，不能证明
   异步输出 chunk 就是该按键的服务端回显。
6. 每个已保存连接的终端 Tab 最多持有一个 transport worker。SSH 只在完成信任和认证后
   启动；Telnet 直接启动明文 TCP worker；Serial 先枚举元数据并解析已保存的 USB 身份，
   只有用户明确要求连接后才打开选定设备。同 profile 的重复 Tab 使用彼此独立的有界命令/
   事件队列。关闭 Tab 时先移除 attempt 路由，再异步 shutdown worker，迟到事件不会更新
   其他 Tab。
   共享 russh client config 明确启用 `TCP_NODELAY`，避免少量交互 channel data 等待 Nagle
   聚合。有界输入队列不设置 batching timer；worker 取出输入后立即发送。这能消除客户端附加
   等待，但不能消除远端 PTY 回显必需的网络往返。
7. 本地终端 Tab 改为持有一个 `portable-pty` worker 线程；它在 Tab 生命周期内独占
   child、reader、writer、resize 状态、有界命令/事件队列、取消标记和超时 join。
8. 每个终端 Tab 还持有一个有界 `TerminalModel`。`vt100` 负责行、字符格样式、光标、
   scrollback、宽字符和 application-cursor 模式。仓库内的 `vendor/vt100` 补丁保持锁定
   的 `0.16.2` API 不变；在缩窄列数会移除宽字符续位格时，先清除对应的宽字符首格，且
   同时覆盖普通与备用屏幕。实时主屏的光标位于底行且改变高度时，补丁会在放大时把最近的
   scrollback 行恢复到视图顶部、缩小时把顶部行送回有界 scrollback，并将光标和最新内容
   保持在新的底边；备用屏、活动滚动区域、非底行光标和用户正在查看 scrollback 时保持上游
   resize 语义。非活动 Tab 的输出留在 Rust 状态，只有活动字符格快照进入
   Slint event loop；更新统一使用
   `slint::invoke_from_event_loop` 和 `Weak<AppWindow>`，避免退出时保活窗口。
   小屏窗口下限为 `520x360`；终端布局、持久化默认尺寸和模型统一使用非零的 `10x3`
   网格下限，既允许窗口紧凑缩小，也不会向 PTY 发出非法的零尺寸 resize。窄屏时可通过
   现有侧栏收起动作优先为终端让出列数。
   `TerminalPane` 会把测得的网格、配置字体度量、活动终端 Tab 身份和连接状态变化合并到
   下一次 UI 轮转后，再请求一次最终 PTY 尺寸。因此 Settings 修改字体后返回已连接终端时，
   与窗口缩放会走同一条当前网格更新路径。完整字符行在测得网格区域内向下对齐：行数向下
   取整后剩余的零散高度放在第一行上方，而非最后一行下方。同一局部偏移同时用于字符格、
   光标/IME 预编辑和指针行映射，不会改变计算出的行数或任何 PTY 请求。
   `AppState::resize_active_terminal` 是 UI 网格变化的单一应用入口：它先请求已有的活动 worker resize，
   再立即调整该 Tab 的本地 `TerminalModel`。本地与 SSH worker 接收 PTY resize；Telnet 只在
   对端接受选项后发送 NAWS；Serial 没有远端终端尺寸契约，因此 worker 请求为 no-op，同一入口
   只调整本地模型。任何 UI resize 被接受后，
   应用都会安排活动终端刷新。该 UI 任务实际执行时才从 `AppState` 复制当前快照，而不应用先前
   worker 事件已捕获的旧快照；因此已经排队的 Output 不会在用户持续拖动窗口时把界面
   恢复为旧网格。worker 随后到达的 `Resized` 仍只作为传输确认。
   SSH 输出通常以有界的 16 ms/16 KiB 批次跨越 worker 边界；终端输入后观察到的首个输出会
   立即刷新当前批次，降低交互回显的本地绘制等待，同时不做本地预测或重复回显；持续的无关
   输出仍保留批处理。
9. macOS 应用保留标准原生标题栏，并关闭 AppKit 的整窗背景拖动。窗口移动只由该原生
   标题栏处理；Slint 工作区 Tab 条作为其下方的普通客户端内容呈现，因此原生窗口拖动
   不会再与 Tab 重排手势竞争。
10. 平台菜单的 Settings 和 About 意图分别把同一个单例 Settings 工作台 Tab 打开到
    General 或 About。它与正在运行的 SSH/本地终端 Tab 一起留在可见工作区 Tab model
    中，因此激活 Settings 不会移除返回活动终端的路径。Close 会在草稿持久化成功后移除该单例
    Tab，绝不影响任何终端 worker。页面切换时未保存草稿仍由 Slint 持有；只有 Settings Tab
    关闭会跨入应用边界。About 展示静态产品用途说明，只接收编译期 package 版本作为只读 UI
   元数据，标明应用使用 `GPL-3.0-only`，并嵌入 Slint 标准 `AboutSlint` 署名组件。该组件保持为
   声明式 UI，自行打开 Slint 网站，不新增应用 callback。会话侧边栏不再重复 Settings/About，
   并从原生标题栏下方贯穿整个客户端高度；
   工作区 Tab 条只占其右侧的工作区列。`+` 固定在最右边缘，打开由 Slint 本地持有的
    选择器，显示全部已保存连接 profile 的遮蔽只读快照，选择后只将 profile UUID 传入
    现有连接 callback。File > New Server、可配置的 `Cmd+N`/`Ctrl+N` 快捷键与侧栏列表
    空白区域的右键菜单仍是独立的新建会话编辑器动作。File 还统一持有剪贴板导入和所选对象
    导出，默认快捷键分别为可配置的 `Cmd/Ctrl+Shift+I` 与 `Cmd/Ctrl+Shift+E`。
11. 单一声明式 Slint `MenuBar` 持有跨平台业务菜单树。锁定的 winit/muda 后端把它安装
    到 macOS 屏幕顶部和 Windows 原生窗口菜单；没有 native menu 支持的 Linux 后端在
    客户区顶部渲染同一棵树。macOS 的 `src/app/macos_window.rs` 复用后端已创建的标准
    应用菜单；现有 About 项存在时把它接到内部页面，同时不依赖 About 是否存在而安装
    `Settings...`。其 key equivalent 跟随实时可配置的 Settings 快捷键，不再写死显示值。AppKit
    target 只在主线程运行且只捕获 `Weak<AppWindow>`；由于 target 为弱引用，菜单项用
    represented object 保持其生命周期。应用边界用单一解析器把配置字符串转换为
    `slint::Keys`；Apple 上持久化的 `Cmd` 映射为 Slint `Control`，物理 `Ctrl` 映射为
    Slint `Meta`，因此 Muda 负责绘制和激活原生 accelerator，不在标题后拼接文字。macOS
    的关闭 Tab 与固定 **Open SFTP Tab** 菜单项刻意不绑定动态活动 Tab 菜单属性，因此
    Tab 身份、类型、连接和 SFTP loading 变化不会重建原生菜单。快捷键或安全状态变化仍可能
    触发重建，AppKit bridge 随后会幂等重绑当前 Settings/About。Windows/Linux 仍保留
    动态关闭 Tab 和 SFTP 状态，并在 Edit/Help 提供 Settings/About；其他菜单复用已有的
    新建会话、侧栏、本地 shell、关闭 Tab、剪贴板传输和快捷键意图。Import 固定进入自动
    识别的有界传输路径；Export 从 `SessionNavigation` 读取当前选中的持久化 Group/服务器
    对象，没有有效选择时只显示固定状态提示。菜单激活只记录固定 action ID；Slint 的
    `MenuItem.activated` 不提供鼠标或 accelerator 来源。
12. 会话导航持有 Slint 本地的侧边栏展开/收起状态、每个 Group 自己的展开状态，以及当前
    选中 kind/ID/Group 名称。展开与收起行共享这份本地选择身份，因此切换侧栏模式后仍保持
    选中高亮，hover 和焦点则是独立的瞬时状态。选择身份不序列化、不进入 `AppState`、不把目标值
    写入日志，也不发送到 transport。Rust
    只提供完整、只读的 `SessionGroupRow` 快照及其嵌套的有界 profile model，不再保存
    展开 Group 集合，也不接收 Group 切换 callback。`SessionNavigationGroup` 与
    `CompactSessionNavigationGroup` 分别管理各自的 Group 展开/收起，因此点击或
    Enter/Space 只改变当前组件的呈现状态。持久化 Group 名称仍由 `SessionStore` 持有，
    空 Group 也能跨重启保留。展开态先渲染 Local Shell 卡片，再渲染可折叠的 Group 父行
    及其单行服务器子项；进入 Slint 的 endpoint 仍是遮蔽值。展开父行显示名称、数量和
    居中的绘制下尖角；收起父行显示对应的上尖角。只有紧凑栏以可配置的 1-4 个 Group 名称
    字符生成文字徽标，或在 Full name 模式显示完整组名，而不是文件夹图标。Full name 模式
    将收起栏限制为 180px，并在稳定的四行预算内换行，避免完整名称落回方形卡片后被截断。
    自定义 Group 行可通过键盘获得焦点，Enter/Space 与点击执行
    相同的本地展开动作；只有独立的紧凑面板按钮负责展开或收起侧栏。原生行右键菜单可在 Group 内
    新增服务器、复制或 Duplicate Group、重命名或删除 Group，以及连接、复制地址、复制配置、
    Duplicate、编辑或删除服务器；Ungrouped 只提供新增服务器。右击列表空白区域可
    新建空 Group 或 Ungrouped 服务器；剪贴板导入/导出只属于 File 菜单。`SessionActionMenu` 把四种菜单形态
    映射为扁平 `ActionMenuItem` 列表；`FlatActionMenu` 只组合一个 `ContextMenuArea`，只发出
    action ID，并暴露 `show-at(Point)`，使同一动作列表也能由按钮触发为下拉菜单。Group/Server
    复制与 File 菜单导出使用版本化 JSON envelope，限制为 256 KiB 和 128 个 profile；导出会移除 identity、凭据
    引用和主机密钥指纹。导入总是生成新 UUID、处理名称/Group 冲突、校验有界字段，并在保存候选
    `SessionStore` 前再次清除凭据/信任字段。Group Duplicate 与现有 Server Duplicate 保持一致：
    profile ID 全部更新、已记住密码引用被清除，但同一 endpoint 的信任指纹保留。删除 Group
    会把 profile 移入 Ungrouped；删除 profile 只移除持久化定义和凭据。收起态用更大的
    Group 徽标和更小、连续排列的服务器徽标保留层级，Local Shell 保持专用入口。应用层
    formatter 会在数据进入 Slint model
    前遮蔽用户名和 IPv4 的中间段。静态尺寸进入 `ui/theme.slint`，持久化的单字符遮蔽设置
    由 `WorkspaceSettings` 持有；收起组名字符数同样由该设置持有，`0` 表示完整名称。

## SSH 安全契约

`russh::client::Handler::check_server_key` 是信任边界。未知和不匹配的主机密钥都在
认证前拒绝。首次拒绝握手可以把 SHA-256 指纹交给确认 UI，但只有用户明确决定后，
该精确指纹才进入 profile；密钥变化需要再次明确确认。密码只作为 callback 的临时
输入，不进入 `SessionStore`。密码 profile 只包含以稳定 UUID 为键的可选
`credential_storage` 后端引用，绝不包含密码或保险库口令。Settings > General 选择以后
勾选 **Remember password** 时使用的后端：系统后端使用 macOS Keychain、Windows Credential
Manager 或 Unix Secret Service；保险库后端使用按 profile 分隔的应用记录。保险库对每条记录
用 Argon2id 派生密钥、用 XChaCha20-Poly1305 加密并将 profile UUID 绑定为附加认证数据，
保险库口令始终是短期输入。私钥 profile 只持久化路径；私钥内容和可选 passphrase 只在一次
blocking 加载/认证任务中短暂存在，不进入配置、tracing 字段或 UI model。

X11 forwarding 是逐 SSH profile 的设置；新 profile 和旧配置缺失该字段时默认开启，已经明确
保存为 `false` 的 profile 仍保持关闭。它只适用于 Terminal mode，SFTP-only、Telnet 和 Serial
worker 永远不会申请 X11。全局 `X11Settings` 只保存非秘密的 provider、仅供 Custom 使用的
应用路径、连接时启动偏好和显式 no-auth 兼容选择。`src/x_server.rs` 负责按平台解析 Auto：
macOS 通过 `NSWorkspace` 和 bundle identifier 发现应用，Windows 先搜索进程 `PATH` 再检查
Program Files；标准安装路径仍作为存在性检查后的兜底。该模块生成有上限的本机 display 候选，
并在 UI 线程外有界启动选定程序。macOS Auto 依次选择 XQuartz、MacXServer；Windows Auto
依次选择 VcXsrv、Xming；Linux 提供系统 `DISPLAY` 和 Custom。所有已知 provider 都忽略保存的
Custom 路径，AxSSH 不下载或安装任何 provider。Custom 不经命令 shell 启动，且必须是普通文件；
Unix 上还必须具有 executable 权限。

申请 forwarding 前，worker 会探测本机 Unix socket 或 loopback TCP 端点，默认以超时和输出
上限执行 `xauth list <DISPLAY>`，要求唯一、明确的 `MIT-MAGIC-COOKIE-1`。若本机 server 未
就绪且允许连接时启动，启动与就绪轮询会串行并受超时限制。MacXServer 只有在显式开启 no-auth
兼容时才会以 `127.0.0.1:6000` 启动；VcXsrv/Xming 也只有在该选择下才接收
`-multiwindow -clipboard -ac`。relay 仍只连接本机端点，并先验证 SSH fake cookie，之后才为
兼容 server 去除 X authority。本机准备失败或服务端拒绝请求时，SSH shell 仍保持连接并显示
X11 不可用。AxSSH 不修改远端 `sshd`；远端必须独立允许 X11 forwarding 并接受请求，才会设置
远端 `DISPLAY`。

启用后，每个 Terminal 为 SSH 请求生成随机 128-bit fake cookie。`ClientHandler` 默认拒绝
服务端发起的 X11 channel，只有 forwarding 请求成功后才允许进入有界分发。等待队列和活动
relay 都最多 8 个；禁用/已关闭时按 administratively prohibited 拒绝，资源超限时明确返回
resource shortage。relay 连接预验证的本机端点，在超时和长度上限内读取 X11 setup，只接受
预期字节序、协议和 fake cookie，将其替换为真实 cookie 后再执行使用固定内部缓冲的双向复制。
fake/real cookie 只存在于 worker 拥有的可清零内存，不持久化、不记录日志；SSH worker 关闭前
会取消并 join 全部 relay task。

认证秘密使用 `ui/components/secret-text-input.slint`，而不是通用文本输入。它保留
原生密码遮蔽、IME、焦点和密码输入可访问性语义，但不发布 `accessible-value`、不提供
编辑右键菜单、不允许复制/剪切快捷键，也不允许鼠标选择进入平台 selection clipboard。
其可访问性契约允许设置值，不允许读取值。Slint 到应用边界会立即把已接收的
`SharedString` 复制到 `Zeroizing<String>`；SSH worker、私钥加载、保险库任务和凭据
回滚会在 drop 时清零 AxSSH 自己拥有的秘密缓冲区。这只能缩短 AxSSH 拥有的秘密寿命，
不宣称能清除 Slint、输入法、russh 或平台凭据后端内部的临时副本。

认证后连接遵循以下生命周期：

- 每个终端 Tab 有唯一运行时 UUID，并由一个 worker 在完整生命周期内独占 russh handle；
- 有界命令 channel 传递 shell 输入、断开、取消和 SFTP 浏览意图；watched terminal size 合并
  高频 resize 更新；
- 启用 X11 的 Terminal 拥有一个有界服务端 channel receiver 和最多 8 个 relay task；
  SFTP-only mode 不创建 X11 dispatcher；
- 终端输出按批次限制大小，并通过有界事件 channel 反压后进入有界终端模型；
- worker 事件报告 connected、resize、output、disconnected、host-key rejection、
  凭据失败或截断后的错误；
- 每个 SSH Tab 独立拥有 probe 取消和认证阶段；每个 UI callback 以及迟到的 probe、凭据
  或 worker 结果都必须重新核验 Tab、profile、attempt 和预期 phase 后才能转换状态；
- 取消既能中断连接/认证，也能断开已建立会话；
- 20 秒 keepalive 和三次未响应上限、以及 90 秒传输 inactivity 边界共同判定连接
  存活；安静的 shell 数据通道是有效状态，绝不单独按无输出超时；
- 关闭 Tab 先使 Tab/attempt 路由失效，再请求 worker shutdown；
- 窗口退出对所有剩余 worker 请求断开，在超时边界内逐个等待 join，最后再关闭 Tokio。

## SFTP 浏览契约

每个 SFTP Tab 拥有一条 SSH transport，认证完成后 worker 只打开独立的 `sftp` subsystem
channel，绝不申请 PTY 或终端 shell。SSH worker 仍是 russh connection 的唯一所有者；应用状态
和 Slint 只接收自有的目录 DTO 与短小浏览意图。服务器右键操作和可配置的打开 SFTP 快捷键都
创建这种独立 Tab，并复用默认拒绝的主机密钥与凭据流程。关闭 SFTP Tab 会关闭浏览器、subsystem
和 transport。

第一阶段提供双栏目录浏览。远端仍使用有界 SFTP 浏览器，`src/app/local_files.rs` 仅在 Tokio
blocking 边界读取本机目录元数据。本地结果带 Tab 内请求 identity，迟到读取不会覆盖较新的路径；进入
Slint 前限制为 250 条、每个名称 256 字符、名称总预算 64 KiB 和路径 4 KiB。传输队列只显示视觉状态，
不提供传输命令。命令/事件 channel 有界，请求串行执行并带超时；入站 SFTP
frame 在进入 `russh-sftp` parser 前拒绝超过 256 KiB 的 packet；raw 目录游标每页最多输出
250 条。单目录在接受 2,000 条或名称/路径累计 2 MiB 时停止，单条路径和名称进入应用快照前
也会校验并限制。`russh-sftp` 内部仍使用 unbounded packet sender，因此 AxSSH 把暴露范围
限制为一个浏览 session 和一个在途请求。上传、下载、删除、重命名和受管编辑同步需要独立的
确认、进度、取消与冲突契约，不属于本阶段。

## Telnet 与 Serial 传输契约

Telnet 被明确标记为明文，且绝不共享 SSH 凭据或信任字段。RFC 854 事件、选项状态、协商
响应、IAC 转义和 subnegotiation 编码由 `libmudtelnet-rs` 负责。本地 64 KiB 有界分帧
适配器先组装完整命令、协商与 subnegotiation 再调用 parser，并把成对的 `IAC IAC`
还原为终端数据；它只隔离已确认的跨调用分片边界，不重新实现选项语义。协商命令不会进入
`TerminalModel`；Echo、Suppress-Go-Ahead、Binary 和 NAWS 等受支持选项得到明确响应，
未知选项被拒绝，且只有对端接受后才发送 NAWS。TCP connect、协议帧、输入输出批次、错误、
队列和 shutdown 等待都有上限。

Serial 发现通过 Tokio blocking 边界调用操作系统枚举 API，只返回 descriptor；不会打开
候选设备、切换 modem line、写入探测字节或推断 baud/parity。应用启动时执行一次扫描，
同时提供明确的手动刷新。用户发起连接后再次扫描；存在 USB 身份时必须解析到唯一设备，
然后才启动由 worker 独占的串口 handle。找不到或出现歧义时默认拒绝；仍支持手工端口名。
Serial 参数和可选的非敏感 USB 身份元数据可以持久化，设备 handle 和通信内容不能持久化。

## 日志生命周期

`src/main.rs` 在创建 UI 前建立唯一的 `LoggingGuard`，并保持到 Slint 与 Tokio 生命周期
结束之后。`src/logging.rs` 通过有界无损队列写入按 UTC 日期滚动的文件，最多保留
15 个，同时把 `INFO` 及以上事件镜像到 stderr。guard 释放时先写退出事件，再排空
队列、刷新当前文件并 join writer 线程。运行字段可以包含 session ID、host、port 和
主机指纹；禁止记录凭据和终端内容。

## 持久化设置与字体资源

`assets/fonts/` 保存 AxSSH 自有的 Maple Mono NF CN、Iosevka Term、JetBrains Mono
和 Monaspace Neon 文件及各自许可证/声明。它们不是 Slint import，不会嵌入可执行文件。
启动时 Tokio blocking task 会从 AxSSH 资源路径分别读取已选中的自带应用字体与 Terminal
字体，相同字体族只读取一次，再由 Slint UI 线程把字节注册到共享 collection；之后在 Settings
即时预览中首次选到的自带字体也会按需读取。候选设置先立即应用，注册完成后再读取当前内存设置
重新应用，避免迟到字体读取恢复旧选择。Appearance 只拥有应用字体、显示模式与配色；Terminal
拥有自己的字体、字号、行高、
亮度、粗体亮色和终端交互设置。两个字体列表都固定先显示自带字体，随后显示由 `fontdb` 在
Tokio blocking task 中发现、按大小写无关去重并按字母排序且有数量上限的系统等宽字体。
`Theme.application-font-family` 统一驱动窗口默认字体和非终端等宽标签，
`TerminalViewState.font_family` 仍是终端字符格度量与绘制的唯一字体来源。构建和运行时都不会从
`third_package/axshell` 加载字体；发行包必须把
`assets/fonts/` 保留在可执行文件旁或平台资源路径中。Slint 测量配置字体，并用测得的字符格
宽度和配置的行高百分比统一计算渲染、选区、光标和向下取整的 PTY 尺寸；不能组成完整字符格的
剩余高度绘制在向下对齐网格的上方，IME 和指针坐标使用同一原点；终端只会在这些度量和布局稳定
后合并发送 resize。

`SessionStore` 在现有私有 `sessions.json` 中写入版本化 profile、非敏感 Group 名称和
`settings` 对象，包括分别经过约束的应用字体与 Terminal 字体、终端字号、行高、亮度、粗体亮色和右键行为、
scrollback、默认 PTY 尺寸、本地 shell 选择和有上限的发现缓存、macOS 的
    Option-as-Meta 偏好、侧栏/Tab 宽度、会话遮蔽字符、收起组名字符数、快捷键、`ThemeSettings`、
    非秘密的 X11 provider/path/启动/兼容设置，以及记住密码的默认后端。schema 版本 16 新增
    收起 Group 徽标字符数设置，`0` 表示完整组名，
    旧文件缺失时保持默认的两个字符。schema 版本 15 新增独立的应用字体；旧文件默认使用 JetBrains Mono，
不会改变已有 Terminal 字体。schema 版本 14 把原有 SSH-only 平铺 profile 字段替换为显式带 tag 的
`ConnectionProfile::{Ssh,Telnet,Serial}`；旧平铺 profile 自动迁移为 SSH，且只有该 variant
可以包含信任或凭据引用。schema 版本 13 新增 `terminal.option_as_meta`；旧文件缺失该字段时
保持 `false`，所以 Option 默认仍产生原生字符、IME 和死键输入。schema 版本 12 将旧
`credential_stored: true` profile 标记迁移为 `credential_storage: "system-keyring"`；
没有已记住密码的 profile 省略该字段。另一种加密保险库记录单独位于私有应用配置目录，
从不包含保险库口令。显示策略独立保存为 System、Light 或 Dark，配色方案独立选择 AxSSH、
Solarized、Arctic、Tokyo、Ember、Forest 或 Custom。固定方案各自提供 Light/Dark 语义色；
固定方案解析为 Dark 时，会使用匹配的 ANSI-16 Terminal 色表。Custom 分别保存 Light/Dark
两套 13 个语义 UI/终端默认色，并规范化为 `#RRGGBB` 或 `#RRGGBBAA`。schema 版本 11 会
拆分旧的组合模式：Solarized Dark 迁移为 Dark + Solarized；旧 Custom 按背景亮度进入对应的
一侧，另一侧使用安全 AxSSH 默认。主题规范化会保证 Light 表面保持浅色、Dark 表面保持深色；
正文、焦点/强调和状态色至少 4.5:1，必要边框至少 3:1，不安全的终端前景/选区组合回退到
相同明暗侧的安全默认。
schema 版本 10 会把旧 profile 中的 Group 值提升为规范化、去重后的 Group 列表，从而
持久化空 Group 和重命名结果。schema 版本 9 会把旧终端配色迁移到对应的固定主题，以保持
升级前的外观。启动时会验证已有 shell 缓存并只追加新发现项；更早的迁移继续只将 schema
版本 7 的旧默认 260px 侧栏改为紧凑 220px，并增加 schema 版本 8 默认 `*` 的遮蔽设置，
不覆盖用户自定义值。密码、passphrase、私钥内容、终端输出、Tab 运行时 ID、子进程和
worker 永远不会序列化。

即使没有已保存 profile，展开会话侧栏也会保留列表空白区域的右键菜单，用于新建空 Group
或添加第一台服务器。用户手动收起后切换为窄栏，窄栏仍保留 Local Shell 和相同的行/列表
右键菜单。Settings/About 只保留在平台菜单和快捷键中，不再进入左侧栏。

`src/app/view.rs` 将已选 palette 经过校验的 Light/Dark 两侧同时送入 `ui/theme.slint`。
System 模式把标准控件 `Palette.color-scheme` 保持为 `ColorScheme.unknown`，由 Slint 跟随
运行时平台 palette；手动 Light/Dark 则显式设置。唯一的 `resolved-dark` 同时选择对应 palette
侧、标准控件方向、AxSSH 自绘表面和终端 ANSI palette。Theme 还显式命名 divider、frame/
control border、focus、hover 和 selected 状态 token，避免共享组件各自重新解释基础色。
原生 `ContextMenuArea` 仍由平台绘制，所以它的具体色值可能不同，但明暗选择保持一致。主题
还统一字号层级、间距、圆角、标准工作区尺寸、Settings 控件尺寸、编辑器宽度和覆盖层尺寸。
`ui/components/themed-combo-box.slint` 统一拥有所有需要 AxSSH 精确配色的应用内选择控件；
控件表面、弹层、hover/选中行、焦点边框、箭头和滚动指示全部消费语义 `Theme` token，不再
使用 Slint 标准控件 palette。组件保留有界字符串 model、current-index、selected callback、
键盘导航、点击外部关闭和 combobox 可访问性契约。其它标准控件继续使用已同步的 Slint
`Palette`，原生 `ContextMenuArea` 菜单仍由平台拥有。
`ui/components/flat-text-input.slint` 统一提供与主题一致的非秘密单行扁平文本输入，供
Settings、会话编辑器和管理弹窗复用。底层原生 `TextInput` 仍独占光标定位、文本选择、
IME、键盘焦点、可访问性和标准文本编辑右键菜单。
`ui/components/secret-text-input.slint` 是 SSH 密码、保险库口令和私钥 passphrase 专用的
密码输入，不继承普通字段的选择或复制/剪切行为。数值输入继续使用标准 `SpinBox`，不重复
实现范围与增减语义。
`ui/components/settings-controls.slint` 使用这些 token 提供共享的 Settings 图标、导航、
页面、右对齐紧凑字段、设置行、开关、快捷键和操作标题栏。设置行保持稳定的标题/元数据
列，标准控件统一使用 Theme 配置的高度。`ui/settings.slint` 在只读
`SettingsViewState` 边界后持有统一草稿，并将编辑合并为即时内存预览；关闭 Tab 时才启动独立的
关闭即保存事务。各分类布局拆到 `ui/settings/*.slint`，只接收本分类需要的局部草稿属性和 callback。
`ui/settings/appearance.slint` 将 Display mode 与 Color palette 分开，并用一个共享
`ThemePaletteEditor` 组件渲染 Custom Light/Dark 字段，避免两套编辑器结构漂移。
`src/app/view.rs` 将当前内存设置的主题映射进 Slint global，并在解析色变化时只重新渲染当前终端
快照。终端渲染使用解析后的默认前景、背景和选区色，仍保留既有 ANSI 16/256 色语义。
主题刷新不会 resize PTY、发送 worker 命令或改变 SSH/本地 shell 生命周期。运行时终端
几何与用户选项仍进入版本化 `AppSettings`；Theme global 只作为视觉解析器，不拥有持久化状态。

## 分阶段范围

当前应用可校验并持久化 SSH、Telnet 与 Serial profile，确认逐 profile 的 SSH 主机指纹，
使用临时密码或本机私钥完成 SSH 认证，为已认证 SSH Tab 提供有界远端 SFTP 和本地元数据目录浏览，并持有多个逐 Tab 隔离的 transport 或本地 shell
终端，相同目标也可重复打开。新建会话编辑器和单例 Settings 工作台都属于可见工作区 Tab；只有短期信任和
secret 提示保留为覆盖层。
以下内容仍作为独立步骤：

- 共享的 OpenSSH 兼容 known_hosts 存储和主机密钥撤销；
- SFTP 上传、下载、修改和受管编辑同步；
- SSH agent、重连和工作区恢复；
- 更完整的全屏终端兼容和鼠标上报。
