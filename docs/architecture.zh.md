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
       │                 Tokio task + russh handle/channel + X11 relay + 私钥/agent 签名
       ├──────────────► SFTP 领域（src/sftp.rs + src/sftp/）
       │                 有界浏览 + worker 所有的下载、上传和编辑操作
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
| `src/app.rs` 与 `src/app/window_router.rs` | 生成 Slint 类型的声明、进程级 UI 启动和 callback 编排；私有多窗口路由、detached transfer 与 pane tree 所有权 | 功能实现、SSH 协议细节或 JSON schema 细节 |
| `src/app/macos_window.rs` | 主线程 AppKit 标题栏、运行中应用图标和标准应用菜单 action 绑定 | 生成的 Slint 类型、持久化设置、SSH 或 worker 状态 |
| `src/app/workspace.rs` 与 `src/app/workspace/` | 私有 workspace facade，以及按职责拆分的 Tab 生命周期、Session Editor 事务和 profile/group 管理接线 | 生成类型声明、传输实现、持久化 schema 或更宽的公共 API |
| `src/app/{connection,connection_monitor,terminal_bridge,settings_bridge,view,serial_bridge,sftp_bridge}.rs`、`src/app/{connection,view}/` | 私有 application bridge 功能接线与内聚的 snapshot/Slint 映射模块，包括协议分发、SSH 信任/认证、直连 worker、串口发现、SFTP 意图、detached opener 调度、pane model 和 settings/options 映射 | 生成类型声明、传输实现或持久化 schema |
| `src/app/file_icons.rs` 与 `src/app/file_icons/platform/` | 有界的进程内文件图标 key/cache 和自有 RGBA fallback；受 cfg 限定的平台 resolver | Slint model、SFTP session、任意路径检查或持久化缓存状态 |
| `src/app/local_files.rs` | SFTP 本地栏的有界目录元数据发现和 regular file 重验 | Slint 类型、文件修改、持久化或 SSH handle |
| `src/app/state.rs` 与 `src/app/state/` | 与 UI 无关的工作区 Tab、逐 Tab 终端/worker 状态、attempt 转换及测试 | Slint component/model 类型或 russh 协议细节 |
| `src/app/{input,session_groups,terminal_render,credential_tasks}.rs` | 可测试的输入/分组/渲染映射、主题化终端默认色和阻塞式凭据 task 边界 | 窗口所有权、传输 handle 或可变 UI 状态 |
| `src/app/diagnostics.rs` | 脱敏键盘分类、固定 diagnostics route/action 字段和专用 tracing target | 原始终端/剪贴板文本、路径、profile 标签、主机、凭据或传输状态 |
| `src/config.rs` 与 `src/config/` | 稳定的 config 入口和显式导出；session/profile 领域、设置、主题规范化、旧配置迁移、私有 JSON 持久化和原子替换 | Slint 类型、网络连接、明文密码存储 |
| `src/credentials.rs` | 按 profile 访问系统凭据库和加密保险库记录 | UI 状态、明文配置、SSH 传输 handle |
| `src/terminal.rs`、`src/terminal_dimensions.rs` 与 `src/terminal/input.rs` | 有界终端网格、共享尺寸契约、字符格样式、光标/scrollback 状态、选区提取和终端按键编码 | Slint 类型、网络 handle、凭据 |
| `src/local_shell.rs` | 跨平台 shell 发现，以及每个 Tab 一个由有界 worker 独占的本地 PTY 子进程 | Slint 状态、SSH 信任、持久化终端内容 |
| `src/x_server.rs` | 平台 X server provider 选项、系统应用发现与标准路径兜底、本机 display 候选和有界进程启动 | SSH channel、UI 状态、cookie、profile 修改或远端服务器配置 |
| `src/ssh.rs` | russh handler、主机密钥决策、密码/私钥/运行时 agent 认证、shell 与服务端发起的 X11 channel 边界 | 窗口更新、持久化会话修改、UI 格式化、agent identity 管理 |
| `src/ssh/private_keys.rs` | 本机 `.ssh` 私钥发现和阻塞式密钥加载 | passphrase 持久化、UI 状态、主机信任决策 |
| `src/ssh/x11.rs` | 本机 DISPLAY 解析、精确 xauth cookie 查询、X11 setup 校验/替换、本机端点连接和 relay | UI 状态、profile 修改、cookie 持久化、启动 X server 或修改访问控制 |
| `src/ssh/worker.rs` 与 `src/ssh/worker/` | 有界 session 启动/命令，以及私有 shell/X11 和 SFTP-only 生命周期模块；合并式 resize、批量事件、取消和关闭 | UI 状态或 profile 持久化 |
| `src/sftp.rs`、`src/sftp/transfer.rs` 与 `src/sftp/transfer/cache.rs` | 有界 SFTP v3 packet 适配、目录浏览、worker 所有的分块下载/上传、文本编辑、重命名/删除和私有临时文件发布/清理 | Slint 类型、凭据、profile 持久化、detached opener 调用或 russh 信任决策 |
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
PNG 安装到 hicolor 目录。所有路径都不读取参考工程，也不绕过字体资源加载契约。

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
激活、关闭、连接、保存和取消等用户意图。`TerminalPaneGroup` 渲染有界、按窗口保存的标准化
`TerminalPane` placement 与内部 split divider 列表。每个 `TerminalPane` 接收只读 `TerminalViewState`，只拥有
终端局部焦点、IME proxy、选区、光标闪烁和尺寸测量；它不拥有 worker、终端缓冲区或连接状态。
Terminal pane 不绘制自身框线；`AppWindow` 只在整个应用窗口客户区绘制唯一的一条框线。
只有新建的 `TerminalPane` 会把一次 IME 焦点重试排到首次布局完成后，并在聚焦原生 proxy 前重新核验其仍可见、focused 且已连接。组件身份不变时，terminal identity、分屏聚焦、连接、可见性及 divider release 请求会同步聚焦已有原生 proxy。终端输入、resize、滚动和选区 callback 都携带终端 Tab UUID，应用只在该 UUID 属于当前窗口
pane tree 时才处理。
鼠标输入遵循同一所有权边界。`TerminalModel` 只暴露当前私有 mouse mode，并生成有界的 SGR、UTF-8
或传统 X10 事件。启用 reporting 时，`TerminalGrid` 通过带 pane UUID 的 callback 转发按下/释放、滚轮、
拖动和 motion 坐标；bridge 重验 pane 后才把字节发送给对应 worker。关闭 reporting 时，既有本地选区和
滚动 fallback 继续生效。备用屏的 alternate-scroll 只在终端确实处于备用屏时视为 reporting。
Terminal Edit 菜单意图以经过校验的 command + 有界 revision 留在 Slint。所有 pane 都观察该信号，
但只有 focused pane 调用既有局部复制、粘贴或全选操作；菜单路由不会把选区坐标或文字提升到
应用状态。
divider 手势只携带稳定的前序 divider ID 和标准化比例；`WindowRouter` 会对当前活动 `PaneTree`
重验，比例及重新发布的叶节点/divider 几何都由该树拥有。当 pane UUID、divider identity 和行数
仍匹配时，bridge 会原地更新既有 Slint model 行，保留 divider 的 repeater 实例和 pointer capture；
不匹配时才回退到常规全量 snapshot 刷新。
pane 焦点 callback 也使用同一套 identity 重验的原地布局更新，因此点击分屏 pane 后不会在下一次
按键前替换其 repeater 或透明 IME proxy；模型过期或结构变化时才回退到全量 snapshot 刷新。
worker 驱动的多窗口刷新共用有界 `AppState` pending gate：一次 UI 事件读取最新路由视图，原地更新
identity 匹配的 pane/divider model 行；若应用期间仍有请求，最多补排一次刷新。终端输出因此不会
堆积无界 Slint event queue，也不会反复替换 focused IME proxy。
对于 identity 匹配的 pane，bridge 还会保留已有 render-line 与 run `VecModel` 的身份：先通过
这些已被订阅的 model 原地写入新行，行数变化时也只 reset 同一 model，最后再更新外层 pane 行。
因此可见 `TerminalGrid` 会在远端输出到达时立即收到 model 通知，不需要等待下一次焦点变化。
光标基于相同原因使用一个保留 identity、有界单行的 model。每份 snapshot 先通过该 model 更新
行、列、可见性和显示字符，再发布终端行，因此光标移动不依赖焦点变化触发外层 DTO 刷新。
只有 `PaneTree` 的非根叶节点可以单独关闭。关闭意图会按所属窗口路由重新校验，随后折叠该叶节点、
只移除对应运行时 Tab、取消 pending probe，并异步 shutdown 仍存在的 worker。子 pane 中 local shell
正常退出或 SSH/Telnet 断开会复用同一路径。workspace 根节点以及连接、认证或 transport 失败状态
继续可见；关闭可见 Terminal Tab 仍负责关闭整棵树。
其内部 `TerminalGrid` 接收更小的 `TerminalGridView` 和 `TerminalSelectionView` DTO：它绘制
有界 snapshot，并把指针、滚动和上下文菜单手势转换成 callback；焦点、IME 输入、选区草稿和
resize 生命周期仍由 `TerminalPane` 保留。
终端目标激活遵循相同边界。按住平台主修饰键时（macOS 为 `Cmd`，其它平台为 `Ctrl`），指针移动或
按下主修饰键都会让 application bridge 检查当前可见行和单元格。私有 parser 返回完整、有界目标的
字符区间，`TerminalModel` 再将其映射回半开 cell 区间；`TerminalPane` 只短暂保存
`TerminalTargetHighlight`，`TerminalGrid` 在完整目标下绘制 accent 下划线并切换鼠标指针。松开
修饰键、指针离开、开始文本选择或滚动时会清除提示。主修饰键点击仍会再次校验 pane UUID，并从
终端模型读取这一行后解析。私有 parser 只接受 `http://`/`https://` URL，以及以 `/`、`./` 或
`../` 开头的 Unix 风格远端路径；会移除终端常见尾随标点和 `:line[:column]` 诊断后缀，并拒绝
控制字符和过长文本。URL 只在 blocking worker 中交给本机默认程序，AxSSH 不会请求该 URL。
远端路径始终复用现有 SSH/SFTP companion 路由：可用 companion 会被激活并导航；仍在主机信任、认证或
SFTP browser 启动阶段的 companion 会把有界路径保留在该运行时 Tab，待正常流程就绪后处理；没有
companion 时，新 SFTP Tab 只在运行期保存初始路径，随后仍执行独立且正常的 SSH 认证。目标文本和该
初始路径都不会持久化、写日志，或作为完整终端缓冲传给 Slint。
私有终端渲染映射还只对可见、默认样式的普通 cell 添加有界语义色：URL 和可操作的 Unix 路径使用链接色；
HTTP `2xx`/`3xx`/`4xx`/`5xx` 及常见的成功、信息、警告和错误状态词使用对应语义色。默认颜色从当前 Terminal
色表派生；Settings 中的规范化 `#RRGGBB` 可按类别覆盖。最终会针对解析后的终端背景校正到至少 4.5:1。显式 ANSI 16/256/真彩色前景、非默认背景、反色、
dim 文本和非 ASCII cell run 都保留远端程序提供的原始渲染。
它的 `key-pressed` 只把特殊键和终端控制组合键发送给 Rust；可打印字符、Shift 文字和已提交的
IME 文本继续通过原生 `TextInput.edited` 路径。
`AppWindow.log-keyboard-event` 在排除快捷键录制和安全提示后，只上报已处理的临时控件按键。
原生菜单命令改走独立的固定 ID menu-action 路由，因为 Slint 不提供鼠标或 accelerator 来源。
diagnostics 边界把所有文字键或粘贴统一转换为固定 `Text` 标签，只接受白名单 route/action；
原始文本和文本长度都不会成为 tracing 字段。

`SettingsPane` 接收只读 `SettingsViewState`，复制到私有可编辑草稿，并把每轮修改合并为
即时内存预览。预览只更新当前应用、Theme、终端和布局，不读取字体文件或写入配置；关闭意图才
携带稳定的 Settings Tab ID，Rust 在异步资源注册和持久化成功后关闭该 Tab，失败时保留草稿与当前预览。
每个分类详情区都可独立滚动，Settings 导航和搜索标题保持固定。未持久化的全局搜索对分类名、
设置标题和说明执行大小写无关匹配；选择结果会清空查询、打开对应分类并回到该分类顶部。查询和
结果模型只属于 UI，不会进入 `AppSettings`、持久化、diagnostics、worker 或 transport。
菜单或原生平台只提供只读的目标 section；用户在设置页导航时的当前 section 由组件自身持有。
`SessionEditorPane` 对 `SessionEditorViewState` 使用同样模式：只有传入的
draft identity 改变时才重置私有字段，用户输入不会反向修改 Rust 快照。其滚动视图根据编辑内容的
preferred height 显式设置 viewport height，因此内容高于当前窗口时，所有字段仍可通过滚动访问。`in-out` 仅保留给
同一局部草稿的嵌套编辑控件；显示文案、dialog 文本和视觉状态都用绑定计算，不重复保存。
密码和保险库口令是编辑器私有的秘密草稿：每次打开都为空，提交后立即清空，绝不进入只读
source snapshot。保存 profile 时密码可以留空；保存密码开关和凭据后端只是明确的保存意图，
未启用保存时后端选择不参与处理。
编辑器还包含 SSH-only、非敏感的 `sftp_remote_path` 和 `sftp_local_path` 字段。
它们只是本地草稿：修改时不会打开任一目录，保存时也不会改变已经运行的 Tab。

`OverlayHost` 拥有 Group/Profile 管理弹层的开关和草稿，并从单个 action 派生标题、消息和
按钮表现；只有确认后才向上发送管理命令。它也组合 SSH 主机密钥与认证弹层，但二者是刻意
保留的例外：可见性和 prompt identity 仍是 Rust 安全 phase 的只读输入。UI 只能提交
confirm/reject/authenticate/cancel 意图，不能在 Rust 接受状态转换前自行隐藏弹层。

## 事件流

1. Slint callback 只产生已保存 profile ID、唯一 Tab ID、终端按键/修饰键、
   草稿字段、保存并连接意图、信任决策或一次性临时秘密等小值。认证秘密经专用
   `SecretTextInput` 传递，应用接收后或用户取消提示时都会清空 UI 中的值。
2. 每次打开 profile 或本地 shell 都会创建新的终端 Tab UUID，即使另一个 Tab 使用
   相同目标。已保存连接的输入、resize、输出、重试和关闭按 `tab_id + profile_id +
   attempt_id` 路由；本地操作按 `tab_id` 路由。未知 SSH 主机会启动绑定该 Tab 的可取消
   探测，但传输仍拒绝。
   工作区 Tab 顺序是仅在内存中的展示状态：拖拽释放会把 Tab UUID 和受限目标位置交给
   `AppState`，它只重排现有 Tab 列表。按住期间 Slint 保留半透明的源槽、高亮目标槽，
   并在指针位置绘制不可交互的 Tab 副本；不会创建第二个运行时 Tab。前置 UI 序号从当前
   列表位置派生，而 `#1` 这类实例后缀仍是稳定标题的一部分。Previous/Next Tab 意图会让
   `AppState` 在同一列表中激活相邻 UUID 并首尾循环；零个或一个 Tab 时状态不变。每个
   SSH Tab 还独占当前
   连接阶段：idle、可取消的主机密钥探测、等待主机密钥确认、等待认证或读取已存凭据；不再
   存在全局的 probe、信任或认证等待槽位。
3. 用户明确确认后，控制器才原子持久化精确指纹。密码 profile 通过 Tokio blocking
   边界读取已记住的凭据或打开密码弹窗；会话编辑器也可以直接提交新的内嵌密码，空值
   保留既有后端引用；非空值可作为 **Save & connect** 对应 Tab 的一次性秘密，只有勾选
   **Save password (optional)** 时才会在 profile 保存前更新所选后端。若请求加密保险库但未提供
   保险库口令，会降级为系统凭据库并记录实际后端；既有保险库记录仍是保险库记录，解锁时仍需要
其保险库口令。私钥 profile 在 UI 线程外加载
   所选路径，只有加密密钥无法空口令打开时才请求一次性 passphrase。SSH agent profile 不读取
   凭据存储，也不打开秘密输入弹窗；主机信任建立后由 worker 连接当前运行时 agent。安全覆盖层只渲染活动
   Tab 的等待阶段；非活动 Tab 保留自己的提示直到被激活，认证提示切换时会先清空其中的秘密输入。
   SSH transport 还会读取平台用户的有界 OpenSSH `known_hosts` 文件。未撤销的精确匹配属于共享信任；
   profile 冲突、变更密钥、坏记录和文件不可读都不会放宽信任。精确匹配 `@revoked` 时在认证前拒绝，
   普通确认不能绕过。未知确认追加观察到的公钥；变更确认原子替换匹配 host 的非撤销记录，同时保留
   注释、无关主机和撤销记录。移除撤销记录是独立的显式动作。
4. Settings > General 持有新记住 SSH 密码的默认后端：平台系统凭据库或应用加密保险库。
   普通密码弹窗会以该设置初始化后端选择，也可以只为本次提示覆盖选择；未勾选 **Save password (optional)**
   时不会使用该选择。会话编辑器只用既有后端或 Settings 默认后端初始化选择器；未勾选
   **Save password (optional)** 时，内嵌密码只供 **Save & connect** 使用一次，单独保存会丢弃该密码。
   勾选后才把密码与 profile 事务性写入所选后端；缺少保险库口令时会有意改用系统凭据库，
   而不会创建无法解锁的保险库记录；
   秘密不会返回 source snapshot 或写入 profile。修改默认值不会迁移或破坏既有凭据。删除 profile、切换为私钥或 SSH agent 认证，或拒绝已
   保存密码时，会事务性删除该引用的凭据，但不会停止已经打开的终端 worker。profile 保存和删除
   共享一个异步凭据闸门，并为每个 profile 分配最新 mutation token；在修改凭据前和替换
   `SessionStore` 前都会重新核验原 profile。已被后续操作取代的事务会在释放闸门前恢复自己的
   凭据备份；保存完成后也只关闭发起该操作且 Tab/draft identity 仍匹配的编辑器。
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
   有界 shell 输入发送；默认的可选右键行为根据是否存在选区选择复制或粘贴。启用
   `copy_selection_on_select` 后，完成鼠标选区和 Select All 都在本地复制，直接右击始终粘贴；
   此模式覆盖独立的右键偏好，选区和剪贴板文字仍不会离开 Slint。
   活动终端报告 connected 前，原生文字/IME 和应用终端按键路由都不可交互；Rust bridge 会再次
   检查连接状态，因此焦点变化或迟到 callback 也不能在建连期间排入终端输入。
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
   SSH 信任与认证仍在进行时，worker 会继续等待并丢弃已经排队的 shell/SFTP 操作；只有明确的
   `Disconnect` 或 controller 被释放才会取消该连接尝试，普通操作必须等到 `Connected` 后生效。
   共享 russh client config 明确启用 `TCP_NODELAY`，避免少量交互 channel data 等待 Nagle
   聚合。有界输入队列不设置 batching timer；worker 取出输入后立即发送。这能消除客户端附加
   等待，但不能消除远端 PTY 回显必需的网络往返。交互 SSH PTY 请求同时启用 `OPOST` 和
   `ONLCR`，使远端普通换行回到第 0 列，避免裸 line feed 在本地终端模型中逐行累积列偏移。
7. 本地终端 Tab 持有一个 `portable-pty` worker 线程；它在 Tab 生命周期内独占 child、reader、
   writer、resize 状态、有界命令/事件队列、取消标记、child-killer handle 和所有线程 join。
   shutdown 会设置取消、唤醒 worker、强制终止隔离的 Unix PTY 进程组或平台 child、关闭 PTY
   资源，并异步等待 worker 可 join。与最后已应用行列数相同的重复尺寸会在调用平台 PTY resize
   前丢弃；满事件队列的反压可响应取消，不会卡住 reader。worker shutdown 使用固定超时，不会
   无限等待；controller 会保留 child-killer 兜底，直到 worker 收尾明确清除它。
8. 每个真正渲染终端的 Tab 持有一个有界 `TerminalModel`。纯 SFTP Tab 从不渲染终端字符格，
   因此不创建该模型，只保留独立的浏览状态。`vt100` 负责行、字符格样式、光标、
   scrollback、宽字符和 application-cursor 模式。终端生成的 `PtyWrite` 协议应答（包括 Windows
   ConPTY 启动依赖的光标位置报告）进入私有有界队列，并且只通过产生该输出的 Tab 当前 transport
   worker 写回；这些应答不进入 Slint、持久化或日志。仓库内的 `vendor/vt100` 补丁保持锁定
   的 `0.16.2` API 不变；在缩窄列数会移除宽字符续位格时，先清除对应的宽字符首格，且
   同时覆盖普通与备用屏幕。`TerminalModel` 将高度变化交给锁定的
   `alacritty_terminal::Term::resize`：放大时只能把真实 scrollback 行恢复到视图顶部。
   历史不足时，已有主屏内容保持顶部对齐，新增空行留在底部；模型不得向下滚动内容或伪造
   空白历史来强制将光标置于新底边。缩小时、备用屏、活动滚动区域、非底行光标和用户正在查看
   scrollback 时保持上游 resize 语义。非活动 Tab 的输出留在 Rust 状态；每个可见 pane 只把自己的有界字符格
   snapshot 送入 Slint event loop；更新统一使用
   `slint::invoke_from_event_loop` 和 `Weak<AppWindow>`，避免退出时保活窗口。
   小屏窗口下限为 `520x360`；终端布局、持久化默认尺寸和模型统一使用非零的 `10x3`
   网格下限。Rust 的 `terminal_dimensions` 模块是模型、设置和各后端最大值的共享来源；由于
   Slint 不能导入 Rust 常量，Theme 保留编译期镜像。PTY 和 worker 入口继续保留独立的非零
   `1x1` 最小值，但共享 `300x100` 最大值，既允许窗口紧凑缩小，也不会向 PTY 发出非法的零尺寸 resize。窄屏时可通过
   现有侧栏收起动作优先为终端让出列数。
   `TerminalPane` 会把测得的网格、配置字体度量、终端 Tab 身份和连接状态变化合并到
   下一次 UI 轮转后，再请求一次最终 PTY 尺寸。初始化也会安排同一合并同步，因此已连接
   pane 在首次稳定布局时无需等待后续窗口或分隔线 resize 就会使用最终网格。Settings 修改
   字体后返回已连接终端时，与窗口缩放仍走同一条当前网格更新路径。完整字符行在测得网格
   区域内从顶部开始绘制：行数向下取整后剩余的零散高度放在最后一行下方，而非第一行上方。
   同一内容区原点同时用于字符格、光标/IME 预编辑和指针行映射，不会改变计算出的行数或任何 PTY 请求。
   `AppState::resize_terminal(tab_id, ...)` 是 UI 网格变化的单一应用入口：它先请求指定可见 pane
   对应 worker 的 resize，再立即调整该 Tab 的本地 `TerminalModel`。本地与 SSH worker 接收 PTY resize；Telnet 只在
   对端接受选项后发送 NAWS；Serial 没有远端终端尺寸契约，因此 worker 请求为 no-op，同一入口
   只调整本地模型。任何 UI resize 被接受后，应用都会安排可见 pane 刷新。
   该 UI 任务实际执行时才从 `AppState` 复制当前快照，而不应用先前
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
    中，因此激活 Settings 不会移除返回活动终端的路径。Settings Tab 已存在时再次按其快捷键，
    只激活既有 Tab，不会创建第二个 Settings 实例。Close 会在草稿持久化成功后移除该单例
    Tab，绝不影响任何终端 worker。页面切换时未保存草稿仍由 Slint 持有；只有 Settings Tab
    关闭会跨入应用边界。About 展示产品用途、package 版本和构建 revision 只读元数据，标明
    应用使用 `GPL-3.0-only`，并嵌入 Slint 标准 `AboutSlint` 署名组件。其支持操作只通过现有
    AppWindow callback：Report a bug 打开 AxSSH issue tracker，Open log folder 打开进程持有的
    滚动日志目录，Copy diagnostics 只把版本、revision、系统、架构和构建类型写入剪贴板。
    不上传数据，也不把配置、主机、路径或凭据字段暴露给 Slint。会话侧边栏不再重复 Settings/About，
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
    Slint `Meta`，因此 Muda 负责绘制和激活原生 accelerator，不在标题后拼接文字。Edit 菜单
    只为 Terminal 提供 **Copy**、**Paste** 和 **Select All**，并移除永久 disabled 的 Undo
    占位。Copy/Paste 复用可配置的终端快捷键；Select All 固定为 macOS `Cmd+A`、
    Windows/Linux `Ctrl+Shift+A`，从而保留这些平台终端内普通 `Ctrl+A`、`Ctrl+C`、`Ctrl+V`
    的输入语义。非 Terminal Tab 中这些命令保持 disabled，因此普通非秘密文本字段继续使用
    原生编辑快捷键和右键菜单，秘密字段仍不可复制；本轮不增加通用文本焦点 bridge、Cut 或 Undo。
    macOS
    的 **Previous Tab** / **Next Tab** 通过同一解析器使用固定 `Cmd+Shift+[` /
    `Cmd+Shift+]`，Windows/Linux 使用 `Ctrl+Shift+[` / `Ctrl+Shift+]`。它们只在多于一个
    Tab 时启用，并共用快捷键录制/安全提示禁用闸门。macOS
    的关闭 Tab 菜单项与跨平台固定的 **Switch SSH/SFTP Tab** 菜单项刻意不绑定动态活动 Tab
    属性，其应用 callback 只在命令触发时解析当前运行时 Tab。只有 Terminal Edit 的 enabled
    状态跟随活动 Tab 类型；工作区刷新、快捷键/安全状态变化以及替换工作区 Tab model 都可能触发
    原生菜单重建，既有 AppKit bridge 随后会幂等重绑当前 Settings/About；重绑会扫描当前 native menu
    tree 的应用 submenu 和 About 标题，兼容平台使用的省略号写法，并在 AppKit 尚未发布重建菜单时
    短暂重试。有界重试预算内的瞬时查找失败保持静默；只有预算耗尽后才输出一条带总尝试次数的
    warning。Windows/Linux 仍保留动态关闭 Tab，并在 Edit/Help
    提供 Settings/About；其他菜单复用已有的
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
    将收起栏限制为 180px，并切换为高密度列表：标题行把侧栏按钮放在末端，Local Shell 使用
    图标加文字的单行项，Group 使用带展开尖角和数量的单行名称，服务器则缩进显示单行全名。
    长名称在稳定行高中省略，并可通过 tooltip 查看全文。
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

## 工作区快照恢复

运行时工作区与 `sessions.json` 分离，保存在私有目录中的
`workspace.json`，并通过原子替换写入。版本化快照只保存有界的 Tab 顺序和
身份、窗口与分屏结构、活动/焦点 Tab、纯文本终端内容以及 SFTP 远程/本地
路径。不会保存 Tokio/russh/PTY worker、活动连接句柄、密码、保险库解锁
材料、私钥口令或临时 host key 决定。启动时，已保存的 profile Tab 会通过
正常 host key 和认证流程创建新 worker；未知 host key 仍必须由用户确认。
终端恢复只是有界文本回放，不会恢复远端进程或 alternate screen 状态。
已删除的 profile 会跳过，其余工作区继续恢复。

## 多窗口工作区转移

SSH Terminal/SFTP Tab 上的内联按钮和 Window 菜单都可以把对应工作区转移到第二个原生
Slint 窗口。detached 窗口把活动连接名显示为原生窗口标题；macOS 将原生标题栏设为透明并使用当前
客户区表面色：Terminal 使用终端背景，SFTP 使用应用背景，使两者保持连续。其同一行的仅图标返回按钮使用系统重叠窗口符号，系统符号不可用时
回退到对应的 AppKit 多文档模板图标。按钮通过 Tooltip 和无障碍描述说明返回主窗口的用途。其客户区使用专门的精简组合，只含当前 `TerminalPaneGroup`
或 `SftpPane`，不包含 Tab 条、
会话 sidebar、已保存连接选择器、Settings、会话编辑器或客户区菜单。`AppState` 仍是 Tab 运行对象、终端模型、待处理的
信任/认证阶段和 transport worker 的唯一 owner。`WorkspaceTransfer` 只携带源窗口 ID、
终端 pane UUID、其 SSH/SFTP companion 与活动 Tab UUID；不会携带 Slint component、russh handle、
Tokio receiver、终端缓冲区或秘密。
主题预览和保存会把既有 `AppSettings` 值同步到每个仍存活的 detached UI，再从该 UI 已解析的
客户区表面色更新其 AppKit 标题栏背景。此纯外观路径保持各窗口本地 Slint theme 一致，
不会把 AppKit 状态写入 `AppState`。
独立 Terminal 虽然不增加客户区菜单，仍保留相同的 Copy/Paste/Select All 直接键盘处理。

`WindowRouter` 按转移后的 Tab UUID 映射当前窗口的 weak UI handle。刷新时每个路由
得到过滤后的 Tab model 和对应 snapshot，因此 worker 的迟到事件仍会更新当前拥有该
工作区的窗口。内联和菜单动作直接把选中的 Tab UUID 传给 Rust 路由 handler；它先验证
UUID 属于发起窗口并设为活动 Tab，再创建或返回原生窗口。转移和返回只修改路由表；关闭
detached 窗口会把 transfer 返回主路由并隐藏原生窗口，不会断开或重新认证 SSH/SFTP。
配对的 Terminal/SFTP UUID 总是一起移动，但两端原本独立的 russh worker 仍保持独立。

`WindowRouter` 在每个窗口路由中为每个可见 Terminal Tab 保存一棵 volatile `PaneTree`。树保存稳定的
工作区 Tab UUID，以及最多 8 个 terminal 叶节点 UUID、有界 split 比例、布局和焦点，不保存 Slint handle、worker、
终端 buffer 或秘密。顶部 Tab model 只发布稳定的工作区 UUID；子 pane 会话仍由 `AppState` 独立管理，
但不会进入 Tab 条。关闭这个可见 Terminal Tab 会关闭树中的全部 terminal 会话，SFTP companion 则
继续作为独立可见 Tab。detached 窗口 Return 或关闭时，会把同一份 pane tree 和子 pane 焦点恢复到
主窗口，不会重连或停止 worker。

每个内部 split 只发布一个 divider overlay。普通态使用语义 divider 色的 hairline，hover、拖拽和
键盘焦点使用 accent 色与较粗线条，但不改变命中区域尺寸。鼠标拖动、对应方向键、Home/End、
无障碍 slider 操作，以及双击或 Enter/Space 复位都会映射到 0.1-0.9 的比例。比例变化复用每个 pane
按 UUID 定向的 terminal resize 路径，因此 PTY/NAWS/本地模型尺寸会跟随新几何。比例在 Tab 切换和
detached/return 转移中保留，但应用重启后恢复默认，也不会进入设置、worker 或 transport 状态。
divider 会把局部 drag 状态保持到 pointer release 或 cancel，随后只请求当前 focused、connected terminal pane
的 IME proxy 取得焦点。键盘和无障碍 divider 操作继续保留自身焦点。

主窗口 Tab 顶部管理栏、保存连接按钮旁放置一组固定尺寸且可键盘聚焦的纵向/横向分屏控件。
它们通过既有 `pane-command` callback 携带当前活动 pane UUID 并发出 `split-right` 或 `split-down`；
Slint 不创建 worker、不直接修改布局，也不会新增顶部 Tab。macOS 的独立 Terminal 将同一组仅图标控件
放在原生标题栏、紧邻返回图标左侧，客户区保持为全高 pane 表面。每个原生动作只捕获 weak `AppWindow`
并调用同一 callback，仍由 `WindowRouter` 校验当前 focused pane。
终端 pane 还可用 `Alt+H/J/K/L` 聚焦左/下/上/右相邻 pane，用
`Alt+Shift+H/J/K/L` 在对应方向创建新的独立终端会话。Local Shell 会创建新的 PTY；SSH、Telnet 和
Serial 会重新走对应 profile 的常规连接流程。SSH child 会重新执行 host-key/认证，绝不继承一次性密码或
私钥 passphrase。SFTP 保持独立表面，不能作为 terminal pane 分屏。只有路由成功受理时 UI 才消费这些
Alt 组合，因此不支持的方向或已达 pane 上限时，普通终端 Meta 输入不会被吞掉。

## SSH 安全契约

`russh::client::Handler::check_server_key` 是信任边界。未知和不匹配的主机密钥都在
认证前拒绝。首次拒绝握手可以把 SHA-256 指纹交给确认 UI，但只有用户明确决定后，
该精确指纹才进入 profile；密钥变化需要再次明确确认。密码只作为 callback 的临时
输入，不进入 `SessionStore`。密码 profile 只包含以稳定 UUID 为键的可选
`credential_storage` 后端引用，绝不包含密码或保险库口令。编辑器中的密码和保险库口令每次
打开都为空，提交后立即清空；非空密码默认只由对应 Tab 短期持有，主机密钥确认完成并由 SSH worker
接管后即清除。只有勾选 **Save password (optional)** 才会额外通过所选后端写入，profile 持久化失败时回滚。
若选择加密保险库但未提供保险库口令，会有意改用系统凭据库并持久化实际引用，不会创建空口令
保险库记录；既有保险库记录仍需要其保险库口令才能解锁。Settings > General 初始化以后保存密码时使用的
后端：系统后端使用 macOS Keychain、Windows Credential Manager 或 Unix Secret Service；保险库
后端使用按 profile 分隔的应用记录。保险库对每条记录用 Argon2id 派生密钥、用
XChaCha20-Poly1305 加密并将 profile UUID 绑定为附加认证数据，保险库口令始终是短期输入。私钥
profile 只持久化路径；私钥内容和可选 passphrase 只在一次 blocking 加载/认证任务中短暂存在，
不进入配置、tracing 字段或 UI model。独立的非秘密 `.ssh` 候选路径扫描只有在 Session Editor
进入 Private key 模式时才启动；离开该模式或关闭编辑器会清空选项 model 并推进代次，在途扫描
不能重新写回已经释放的 UI 状态。

SSH agent profile 只持久化 `AuthMethod::SshAgent`，不能包含密码凭据引用，也不保存 agent
socket 路径、identity 注释、公钥、私钥或 passphrase。真实 SSH 握手通过上述同一精确主机密钥
校验后，`src/ssh.rs` 才为本次连接访问运行时 agent。Unix/macOS 使用当前
`SSH_AUTH_SOCK`；Windows 使用该变量或 OpenSSH agent 默认 named pipe。agent 负责列出
identity 和签署认证请求，client 始终由 russh worker 独占。AxSSH 最多尝试 5 个 identity，并用
一个 30 秒总上限覆盖 agent 连接、identity 列举、算法协商、签名和认证；成功、失败、取消或超时
都会释放 client。应用自身只返回固定错误类别，不包含 socket 路径、identity 注释或密钥数据；
解锁或确认界面仍由系统 agent 自己拥有。这只是客户端认证，不包含 agent forwarding 或 agent
密钥管理。

X11 forwarding 是逐 SSH profile 的设置；新 profile 和旧配置缺失该字段时默认开启，已经明确
保存为 `false` 的 profile 仍保持关闭。它只适用于 Terminal mode，SFTP-only、Telnet 和 Serial
worker 永远不会申请 X11。全局 `X11Settings` 只保存非秘密的 provider、仅供 Custom 使用的
应用路径、启动偏好和显式 no-auth 兼容选择。`src/x_server.rs` 负责按平台解析 Auto：macOS 通过
`NSWorkspace` 和 bundle identifier 发现应用，Windows 先搜索进程 `PATH` 再检查 Program Files；
同时向 Settings 返回有上限、只读的已知安装位置快照。标准安装路径仍作为存在性检查后的兜底。
macOS Auto 依次选择 XQuartz、MacXServer；Windows Auto 依次选择 VcXsrv、Xming；Linux 提供系统
`DISPLAY` 和 Custom。所有已知 provider 都忽略保存的 Custom 路径，AxSSH 不下载或安装任何
provider。Custom 不经命令 shell 启动，且必须是普通文件；Unix 上还必须具有 executable 权限。

创建 shell 时只会带着随机 128-bit fake cookie 发送 X11 forwarding request，不读取本机
`DISPLAY`、不运行 `xauth`、不探测本机端点，也不启动 X server。只有远端 server 打开 X11
channel 时，relay 才解析本机 display 候选、在超时和输出上限内执行 `xauth list <DISPLAY>`，并在
需要且启用时启动选定 provider、轮询其就绪状态。MacXServer 只有在显式开启 no-auth 兼容时才会以
`127.0.0.1:6000` 启动；VcXsrv/Xming 也只有在该选择下才接收 `-multiwindow -clipboard -ac`。
relay 仍只连接本机端点，并先验证 SSH fake cookie，之后才为兼容 server 去除 X authority。
本机准备、channel 或服务端请求失败时，只会拒绝对应 X11 channel，SSH shell 仍保持连接并显示
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
- 认证完成前，已排队的 shell/SFTP 操作会被忽略且不会结束连接尝试；`Disconnect` 仍立即生效；
- 取消既能中断连接/认证，也能断开已建立会话；
- 20 秒 keepalive 和三次未响应上限、以及 90 秒传输 inactivity 边界共同判定连接
  存活；安静的 shell 数据通道是有效状态，绝不单独按无输出超时；
- 关闭 Tab 先使 Tab/attempt 路由失效，再请求 worker shutdown；
- 窗口退出对所有剩余 worker 请求断开，在超时边界内逐个等待 join，最后再关闭 Tokio。

## SFTP 浏览与写操作契约

每个 SFTP Tab 拥有一条 SSH transport，认证完成后 worker 只打开独立的 `sftp` subsystem
channel，绝不申请 PTY 或终端 shell。SSH worker 仍是该 russh connection 的唯一所有者；应用
状态和 Slint 只接收自有的目录 DTO 与短小浏览意图。配对的 Terminal 与 SFTP Tab 也绝不共享
russh handle 或 worker。

服务器右键 **Open SFTP** 会创建无配对来源的独立 SFTP Tab。可配置的
**Switch SSH/SFTP Tab** 命令则使用仅由 `AppState` 持有、不会持久化的运行时 Tab UUID 配对。
从未配对的 SSH Terminal 触发时，会在其后创建并独立认证 SFTP Tab；从未配对的独立 SFTP Tab
触发时，会在其前创建并独立认证 Terminal。两条路径都复用默认拒绝的主机密钥与凭据流程。
配对建立后，命令只激活对应 Tab，不会再次连接或认证。关闭任一端只解除配对，并只关闭该 Tab
自己的浏览器、subsystem、worker 和 transport；另一端继续保留，之后可重新创建配对 Tab。

只有 SFTP Tab 报告 connected 后，远端导航和选择控件才可交互。此前 `AppState` 不发布
available 的远端 snapshot，application bridge 也会独立拒绝来自未连接或非 SFTP Tab 的操作。

创建新的 SFTP Tab 时，SSH profile 会把初始远端目录交给 worker 所有的浏览器，把初始本地目录
交给 application-owned 的本地 snapshot。旧 profile 缺少远端值时使用 `~`，本地值为空时解析为
平台 home 目录。这些默认值只在 Tab 初始化时使用，之后的导航仍属于各自 Tab。

第一阶段提供双栏目录浏览。Slint 拥有两个受约束的 splitter：一个调整远端/本地宽度，另一个调整
文件区/Transfers 高度。`WorkspaceShell` 只在当前进程生命周期内保留两个比例和 Transfers 折叠状态，
`SftpPane` 则按响应式最小尺寸限制两侧。splitter 提供 resize 光标、键盘焦点与方向键调整，以及 slider
可访问操作；双击目录 splitter 恢复等宽，双击 Transfers splitter 折叠或展开队列。分栏状态不进入
Rust、配置 schema 或 SFTP transport，Name/Size/Modified 列在本阶段仍是固定的响应式列。
两个目录标题栏还只会通过既有 clipboard callback 发出当前已受限路径；复制按钮不读取目录，也不会接触
SFTP worker。

远端仍使用有界 SFTP 浏览器，`src/app/local_files.rs` 仅在 Tokio blocking 边界读取本机目录元数据。
本地结果带 Tab 内请求 identity，迟到读取不会覆盖较新的路径；进入 Slint 前限制为 250 条、每个名称
256 字符、名称总预算 64 KiB 和路径 4 KiB。远端浏览器在应用状态中为每个 Tab 保留有界的
前进/后退路径历史；只有目录页成功返回后才提交历史，因此失败请求不会消耗导航步骤，加载期间
导航按钮会禁用。远端和本地行都拥有真实的 Tab 内选中状态，表头可以全选或清空；目录刷新后
只保留仍存在于当前快照的条目，选中本身不会启动传输。命令/事件 channel 有界，请求串行执行
并带超时；入站 SFTP frame 在进入 `russh-sftp` parser 前拒绝超过 256 KiB 的 packet；raw
目录游标每页最多输出 250 条。单目录在接受 2,000 条或名称/路径累计 2 MiB 时停止，单条路径和
名称进入应用快照前也会校验并限制。`russh-sftp` 内部仍使用 unbounded packet sender，因此
AxSSH 把浏览器暴露范围限制为一个 session 和一个在途请求。

每行从 `src/app/file_icons.rs` 接收 24x24 的自有 RGBA 图标。UI 只读取内存结果或内建的目录、
链接、通用文件 fallback；平台查询与图片解码都在 blocking worker 中运行，每批最多预热 64 个
唯一 key，进程内最多保留 128 项。macOS 通过 NSWorkspace 查询 UTType 图标，Windows 使用
合成文件属性调用 Shell API，Linux 将扩展名映射为 MIME 与 freedesktop 图标主题。远端名称不会
被当成本机路径解析图标，Slint 也不调用平台或文件系统图标 API。provider 由 SFTP 图标预热首次
创建，而不是在进程启动时创建。关闭最后一个 SFTP Tab 会清除已解析的扩展名图标并让待处理预热
代次失效；固定 fallback 图标继续保留。

双击本地 regular file 行只产生只读打开意图。bridge 先要求该精确路径仍位于活动 SFTP Tab 的
当前本地快照；blocking worker 再使用不跟随链接的元数据重验目录和条目，拒绝目录与符号链接，
打开只读 handle，并要求其平台文件 identity 和列目录时记录的长度、修改时间、创建时间 fingerprint
完全一致。该 metadata fingerprint 只能发现当前平台可观察到的变化，不能作为内容完整性保证。AxSSH
从这个已验证 handle 复制到有界私有 open cache，原子发布快照后才通过
`open::that_detached` 调用平台默认程序；不会再次打开已验证的源路径。过期 Tab、目录 request、路径、
identity 或 fingerprint 不匹配的条目都会在调度前被拒绝，验证后的路径替换也无法把 opener 重定向到另一个
文件 identity。

选择远端文件或目录会向 worker 发送小型下载根 intent。worker 自己打开 SFTP subsystem 进行递归发现，
拒绝链接及不安全/非 regular 条目，并生成以当前 Local files 目录为根的自有文件请求。目录会保留相对
目录树。发现过程最多扫描 4,096 个条目，并最多接受 512 个文件、256 个目录、16 层、512 KiB 路径文本、1 GiB 总字节，
每个文件最多 512 MiB。每个 SFTP Tab 最多允许两个活动或正在打开的 transfer，每个 transfer 独占单独的 SFTP
subsystem stream。

每个请求都会重验远端路径和 handle 元数据，每次最多读取 64 KiB，writer queue 只容纳两个 chunk，
每个操作 15 秒超时、总时长 30 分钟，并报告自有的队列、状态、进度和终态事件。应用状态拥有有界行，
分为活动、失败（包括已取消）和成功快照；Slint 只渲染这些 DTO，并发送勾选/批量暂停、继续或取消 intent。
暂停/继续是 worker 生命周期内的契约：writer 保留部分文件，流只在该 worker 存活时从当前 offset 继续。

本地 writer 会校验每个路径组件，拒绝符号链接穿越和已有目标；Unix 上创建该任务专属的 `0600` `.part`
文件，随后 flush、fsync 并以不替换并发本地文件的方式原子发布最终名称。取消和失败会删除部分数据；若发布后才观察到取消，
会在报告成功前删除最终目标。成功的本地下载会保留。关闭 Tab 会取消并 join 待发现、待打开 subsystem
和活动 transfer。远端工具栏负责有界删除、重命名、UTF-8 编辑和 Save As；本地 regular file 通过同一 transfer queue 上传。
编辑器打开期间按远端 size/mtime fingerprint 轮询监控；自动上传必须显式开启、默认关闭并经过防抖与 fingerprint 校验。
拖放只接受有界路径 intent，随后复用 bridge 校验与 transfer queue。

## Telnet 与 Serial 传输契约

Telnet 被明确标记为明文，且绝不共享 SSH 凭据或信任字段。RFC 854 事件、选项状态、协商
响应、IAC 转义和 subnegotiation 编码由 `libmudtelnet-rs` 负责。本地 64 KiB 有界分帧
适配器先组装完整命令、协商与 subnegotiation 再调用 parser，并把成对的 `IAC IAC`
还原为终端数据；它只隔离已确认的跨调用分片边界，不重新实现选项语义。协商命令不会进入
`TerminalModel`；Echo、Suppress-Go-Ahead、Binary 和 NAWS 等受支持选项得到明确响应，
未知选项被拒绝，且只有对端接受后才发送 NAWS。TCP connect、协议帧、输入输出批次、错误、
队列和 shutdown 等待都有上限。

Serial 发现通过 Tokio blocking 边界调用操作系统枚举 API，只返回 descriptor；不会打开
候选设备、切换 modem line、写入探测字节或推断 baud/parity。Session Editor 只有在用户选择
Serial 或明确刷新列表时才请求扫描；应用启动不会枚举串口。用户发起连接后再次扫描；存在 USB 身份时必须解析到唯一设备，
然后才启动由 worker 独占的串口 handle。找不到或出现歧义时默认拒绝；仍支持手工端口名。
Serial 参数和可选的非敏感 USB 身份元数据可以持久化，设备 handle 和通信内容不能持久化。
离开 Serial 模式或关闭编辑器会同时清空 descriptor 和 Slint 选项 model；代次检查会丢弃迟到
的发现结果。

## 日志生命周期

`src/main.rs` 在创建 UI 前建立唯一的 `LoggingGuard`，并保持到 Slint 与 Tokio 生命周期
结束之后。`src/logging.rs` 通过有界无损队列写入按 UTC 日期滚动的文件，最多保留
15 个，同时把 `INFO` 及以上事件镜像到 stderr。guard 释放时先写退出事件，再排空
队列、刷新当前文件并 join writer 线程。运行字段可以包含 session ID、host、port 和
主机指纹；禁止记录凭据和终端内容。About 只接收 guard 已创建的日志目录 owned path，
通过应用 bridge 打开它，不改变日志模块的所有权。

## 持久化设置与字体资源

`assets/fonts/` 保存 AxSSH 自有的 Maple Mono NF CN、Iosevka Term、JetBrains Mono
和 Monaspace Neon 文件及各自许可证/声明。它们不是 Slint import；JetBrains Mono 四个字重会编译进
可执行文件，作为始终可用的应用和 Terminal 默认字体。Tokio blocking task 只从 AxSSH 资源路径读取
已选中的其他自带字体，再由 Slint UI 线程统一把字节注册到共享 collection。选中 Terminal 或打开
本地 shell 的第一个终端 Tab 时才加载 Terminal 字体；之后在 Settings 即时预览中首次选到的自带字体
也会按需加载。候选设置先立即应用，注册
完成后再读取当前内存设置重新应用，避免迟到字体读取恢复旧选择。Appearance 只拥有应用字体、显示模式与配色；Terminal
拥有自己的字体、字号、行高、
最小对比度、粗体亮色、五项可选语义高亮色和终端交互设置。两个字体列表都固定先显示自带字体，随后显示由 `fontdb` 在
Tokio blocking task 中发现、按大小写无关去重并按字母排序且有数量上限的系统等宽字体。
`Theme.application-font-family` 统一驱动窗口默认字体和非终端等宽标签，
`TerminalViewState.font_family` 仍是终端字符格度量与绘制的唯一字体来源。构建和运行时都不会从
`third_package/axshell` 加载字体；发行包必须把
`assets/fonts/` 保留在可执行文件旁或平台资源路径中，以提供三个可选自带字体和全部字体声明。Slint 测量配置字体，并用测得的字符格
宽度和配置的行高百分比统一计算渲染、选区、光标和向下取整的 PTY 尺寸；`TerminalPane` 只计算一个内容区
光标 cell y 坐标，网格、预编辑覆盖层和原生 IME proxy 共同使用它，pane clip 是唯一的垂直溢出边界。正常高度下不能组成完整字符格的
剩余高度保留在网格底部，避免扩大终端顶部内容边界，IME 和指针坐标使用同一原点。pane group 会把每个
终端 surface 裁剪到分配的 split 矩形，pane 自身再裁剪网格、光标、预编辑覆盖层和透明 IME proxy，确保
尺寸过小的嵌套分屏不能绘制到相邻 pane 或工作区之外。pane 高度不足终端保底三行时，网格才把当前底行贴住
pane 底边，并优先裁剪较旧的顶部行；终端只会在这些度量和布局稳定后合并发送 resize。

首次打开 Settings 时才会在 blocking worker 中发现本地 shell、系统等宽字体和已知 X server
安装位置；再次激活现有单例 Tab 不会重复扫描。关闭 Settings 会释放发现出的系统字体与 X server
选项 model，同时保留自带字体选项和有界的内存 shell 列表。已经注册到 Fontique/Slint 的字体族
没有可靠卸载 API，会保持进程级可用；平台字体/应用/图标数据库和 allocator 也可能在 AxSSH 丢弃
引用后继续持有自身缓存，因此不能预期进程 RSS 立即下降。

`UiLanguage` 是 config 拥有的策略，持久化值稳定为 `system`、`english` 和
`simplified-chinese`。schema 版本 21 将旧配置或未知值默认迁移为跟随系统；中文系统 locale
解析为内嵌的 `zh-CN` 目录，其它 locale 解析为英文。`build.rs` 内嵌经过审阅的 PO 目录；翻译
检查器要求每个静态 Slint `@tr` 文案都有非空翻译，并保持相同的编号占位符。Slint 只提交稳定的
选择索引；应用 bridge 在 blocking 事务成功持久化语言后，才在 UI 线程切换进程级内嵌 locale，
并同步所有存活的主窗口与独立窗口。普通 Settings 预览/保存始终保留最后提交的语言，避免并发预览
覆盖语言选择。远端 Terminal 内容、用户值、日志和运行时技术错误详情不作为翻译键，也不会被翻译。

发行自动化只拥有发行元数据，不拥有应用运行时状态。手动启动的 `Create Dated Release` workflow
按 `Asia/Shanghai` 计算日期：当天首发公开 tag 为 `YYYY-MM-DD`，正整数修订后缀形成
`YYYY-MM-DD-N`。基础日期映射为 Cargo/Debian 的 `YYYY.M.D` 和 macOS build 的 `YYYYMMDD`；
修订映射为 Cargo `YYYY.M.D+N`、Debian `YYYY.M.D-N` 和 macOS build `YYYYMMDD.N`，macOS short
version 保持 `YYYY.M.D`。workflow 更新锁文件和 macOS bundle 元数据，再创建 annotated release tag。
`Retry Existing Release` 既是新 tag 创建后调用的 reusable workflow，也是手动重试明确已有 tag 的入口；
它校验 annotated tag 和已提交元数据、为精确 SHA dispatch CI，并且只在 CI 成功后 dispatch Release，
不具备任何写 tag 操作。
发布 workflow 构建 Windows x86_64、Linux x86_64/aarch64，以及 arm64/x86_64 macOS 二进制；随后合并
macOS 通用 bundle，并在适用的发行包中保留 `assets/fonts/`、图标和独立许可证声明。CI 只在默认分支或
日期 tag 成功后写入按 target 隔离的共享 Cargo cache，失败 job 不会写入；发布 job 也会独立验证所选
tag 的 CI 成功后才恢复该缓存，并重新构建锁定的 release 二进制。构建前会校验
所有版本表示一致，且不会读取或打包 `third_package/axshell`。
发布前，workflow 只向 `scripts/generate_release_highlights.py` 提供已检出的 tag 历史和仓库 URL；
该脚本只返回 Markdown，不拥有应用状态或发行资产。其去重的分类提交摘要作为 Release 正文前缀，
GitHub 自动生成的 notes 继续保留完整提交列表。

`SessionStore` 在现有私有 `sessions.json` 中写入版本化 profile、非敏感 Group 名称和
`settings` 对象，包括分别经过约束的应用字体与 Terminal 字体、终端字号、行高、最小对比度、粗体亮色、可选语义高亮色和鼠标复制/粘贴偏好、
scrollback、默认 PTY 尺寸、本地 shell 选择和有上限的发现缓存、macOS 的
    Option-as-Meta 偏好、侧栏/Tab 宽度、会话遮蔽字符、收起组名字符数、快捷键、`ThemeSettings`、
    非秘密的 X11 provider/path/启动/兼容设置、SSH 认证方式（可选择 agent，但不包含 agent 端点或
    identity），以及记住密码的默认后端和界面语言策略。schema 版本 21 新增跟随系统的界面语言策略；缺失或无效值默认跟随系统。schema 版本 20 增加五项可选 Terminal 语义色覆盖；空值或无效值跟随活动 ANSI 色表，非空值规范化为不透明 `#RRGGBB`。schema 版本 19 增加 SSH SFTP 默认目录字段；缺失远端值
    默认为 `~`，本地值为空表示平台 home 目录。schema 版本 18 增加默认关闭的
    `copy_selection_on_select` 偏好；旧配置保持既有右键行为。schema 版本 17 将原有
    `terminal_brightness_percent` 重映射替换为定点保存的
    `terminal_minimum_contrast_ratio_tenths`，范围为 1.0:1 至 21.0:1，默认 4.5:1。
    应用调用方通过 `AppSettingsInput` 提交原始值，并按 Appearance、Terminal、Workspace 和
    Shortcuts 所有权域分组。这些输入类型不包含 Slint 值，规范化后仍写入同一个持久化
    `AppSettings`，不会给 JSON schema 增加字段或改变其结构。
    schema 版本 16 及更早文件会丢弃旧亮度值并迁移到该默认值，因为两种设置不存在安全的一一数值映射。
    schema 版本 16 新增
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
拆分旧的组合模式：Solarized Dark 迁移为 Dark + Solarized；旧 Custom 按背景明暗进入对应的
一侧，另一侧使用安全 AxSSH 默认。主题规范化会保证 Light 表面保持浅色、Dark 表面保持深色；
正文、焦点/强调和状态色至少 4.5:1，必要边框至少 3:1，不安全的终端前景/选区组合回退到
相同明暗侧的安全默认。
schema 版本 10 会把旧 profile 中的 Group 值提升为规范化、去重后的 Group 列表，从而
持久化空 Group 和重命名结果。schema 版本 9 会把旧终端配色迁移到对应的固定主题，以保持
升级前的外观。首次打开 Settings 时才会验证并追加 shell 发现项，结果先留在内存，下一次明确
保存设置时再持久化有界列表；更早的迁移继续只将 schema
版本 7 的旧默认 260px 侧栏改为紧凑 220px，并增加 schema 版本 8 默认 `*` 的遮蔽设置，
不覆盖用户自定义值。密码、passphrase、私钥内容、终端输出、Tab 运行时 ID、子进程和
worker 永远不会序列化。

config 领域会拒绝控制字符，并对 profile 名称、host、username、私钥路径、主机密钥指纹、
串口标识和 Group 名称应用共享字符上限。单个 store 最多包含 1,024 个 profile 和 256 个 Group；
`sessions.json` 在反序列化前与编码后都不得超过 8 MiB。每个反序列化 profile 和每次 store 保存
都会经过同一套领域校验。私有配置与保险库写入使用同目录隐藏 UUID 临时文件、`create_new`、Unix
`0600` mode 和普通文件校验；写入或替换失败时由 guard 删除临时文件。文件同步、平台原子替换、
最终私有权限和父目录同步仍属于同一次提交，不再跟随固定 `.tmp` 路径。

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
`ui/components/elided-controls.slint` 统一拥有单行文本按钮的呈现契约。`ElidedLabel` 将配置
字体的自然宽度测量与可见省略标签分开，并输出 `natural-width`、`line-height` 和
`overflowed`；仅在标签真实溢出时创建有最大宽度且可换行的全文 tooltip。`ElidedButton`
继续让 Slint 标准 `Button` 拥有焦点、键盘、enabled、pressed、可访问性和点击语义，只在其上
覆盖共享标签。调用方显式传入显示 `text`、可选 `tooltip-text`、独立的
`accessible-name`、`enabled` 和 `clicked()`；纯图标按钮保留自身的用途 tooltip。这些 UI
局部值不会进入 Rust、持久化、diagnostics、worker 或 transport。
`ui/components/flat-text-input.slint` 统一提供与主题一致的非秘密单行扁平文本输入，供
Settings、会话编辑器和管理弹窗复用。底层原生 `TextInput` 仍独占光标定位、文本选择、
IME、键盘焦点、可访问性和标准文本编辑右键菜单。
`ui/components/secret-text-input.slint` 是 SSH 密码、保险库口令和私钥 passphrase 专用的
密码输入，不继承普通字段的选择或复制/剪切行为。数值输入继续使用标准 `SpinBox`，不重复
实现范围与增减语义。
`ui/components/settings-controls.slint` 使用这些 token 提供共享的 Settings 图标、导航、
页面、右对齐紧凑字段、设置行、开关、快捷键和操作标题栏。设置行保持稳定的标题/元数据
列，标准控件统一使用 Theme 配置的高度；设置搜索框和结果行也复用这里的基础控件。
`SettingsPage` 提供统一的详情滚动容器，较长的 Appearance、Terminal 和 About 页面保留等价的
页面内滚动容器。`ui/settings.slint` 在只读 `SettingsViewState` 边界后持有统一草稿、未持久化的
分类选择和搜索查询，并接收 application bridge 生成的有界结果列表；编辑继续合并为即时内存预览，
关闭 Tab 时才启动独立的关闭即保存事务。各分类布局拆到 `ui/settings/*.slint`，只接收本分类需要的
局部草稿属性和 callback。
`ui/settings/appearance.slint` 将 Display mode 与 Color palette 分开，并用一个共享
`ThemePaletteEditor` 组件渲染 Custom Light/Dark 字段，避免两套编辑器结构漂移。
`src/app/view.rs` 将当前内存设置的主题映射进 Slint global，并在解析色变化时只重新渲染当前终端
快照。终端渲染使用解析后的默认前景、背景和选区色，同时保留 ANSI 16/256/真彩色语义。
有界语义覆盖层只改变符合条件的可见默认样式 cell，并从当前终端色表派生不同的链接、成功、信息、警告和错误色；存在规范化的 Settings 覆盖时使用该颜色。
每一色均会针对终端背景校正至至少 4.5:1。最小对比度按每个单元格的实际背景计算；只有低于目标的前景会向黑或白修正，背景和已经可读的颜色保持不变。
设置为 1.0:1 可关闭修正，dim 文本使用一半目标对比度以保留层级。
主题刷新不会 resize PTY、发送 worker 命令或改变 SSH/本地 shell 生命周期。运行时终端
几何与用户选项仍进入版本化 `AppSettings`；Theme global 只作为视觉解析器，不拥有持久化状态。

## 分阶段范围

当前应用可校验并持久化 SSH、Telnet 与 Serial profile，读取用户的 OpenSSH
`~/.ssh/known_hosts` 作为有界共享信任来源，并确认逐 profile 的 SSH 主机指纹，
使用临时密码、本机私钥或有界运行时 SSH agent 完成 SSH 认证，为已认证 SSH Tab 提供有界远端 SFTP、本地元数据目录浏览和 regular file 下载后打开，并持有多个逐 Tab 隔离的 transport 或本地 shell
终端，相同目标也可重复打开。新建会话编辑器和单例 Settings 工作台都属于可见工作区 Tab；只有短期信任和
secret 提示保留为覆盖层。
以下内容仍作为独立步骤：

- 超出共享解析器的 known_hosts 管理（应用内撤销/替换 UI 与系统级策略编辑）；
- 更完整的 SFTP 冲突解决与跨进程编辑恢复；
- 跨进程重连任务持久化和工作区恢复；
- 更完整的全屏终端兼容和鼠标上报。
