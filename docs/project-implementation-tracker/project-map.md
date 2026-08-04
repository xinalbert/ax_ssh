# 项目地图

## 项目概览

- 用途：基于 Slint/Tokio 的跨平台桌面终端工作区，支持 SSH、Telnet、Serial、本地 shell 与独立 SFTP Tab 的远端目录浏览。
- 主要入口：`src/main.rs`、`src/app.rs`、`ui/app.slint`、`build.rs`。

## 索引范围

- 根目录：`<repo-root>`
- 覆盖：`AGENTS.md`、`.agents/`、`LICENSE`、`THIRD_PARTY_NOTICES.md`、`Cargo.toml`、`Cargo.lock`、`vendor/`、`build.rs`、`src/`、`ui/`、`assets/`、`packaging/`、`docs/`、`.github/workflows/ci.yml`、根 README 和 `.gitmodules`。
- 排除：`.git/`、`target/`、`third_package/axshell` 内部文件和本机 Cargo 缓存。

## 目录地图

| Path | Purpose | Open When | Notes |
| --- | --- | --- | --- |
| `LICENSE`、`THIRD_PARTY_NOTICES.md` | AxSSH GPL 主许可证与独立第三方许可边界 | 修改授权、分发或打包声明时 | 原创软件使用 GPL-3.0-only；Slint/OFL 字体/MIT vendor 保留各自声明 |
| `src/` | Rust 库边界、进程、UI bridge、配置、系统凭据、终端、日志和 transport | 修改行为、状态、存储、凭据、终端、日志或连接时 | `lib.rs` 导出 config/credentials/logging/ssh/sftp/telnet/serial/terminal；生成的 Slint 类型只在 `app.rs` 使用 |
| `ui/` | Slint 页面、共享 Settings 控件和集中式设计 token | 修改布局、视觉状态、主题或 UI callback 时 | 页面不持有静态颜色/字号/间距字面量，不执行文件系统或网络操作 |
| `assets/fonts/` | 项目自带应用/Terminal 字体、许可证和作者声明 | 修改字体选择或打包资源时 | TTF 只由 AxSSH 在运行时从资源路径读取；不经 Slint import 嵌入，也不读取参考子模块 |
| `assets/ion/` | 用户提供 Terminal 图标的跨平台资源集与说明 | 接入应用图标、打包或替换品牌图标时 | `terminal_icon.svg` 是唯一源；Slint/winit 使用 256px PNG，Windows 嵌入 ICO，macOS Dock/Bundle 使用 PNG/ICNS，Linux package 安装 hicolor PNG 集 |
| `vendor/vt100/` | 历史终端网格补丁的保留副本 | 审计旧差异或移除遗留依赖时 | 当前迁移不修改其源码；不得再作为新的终端功能实现点 |
| `.agents/` | 项目级 Codex skills 和按需加载的工程规范 | 修改 Rust、Slint、应用边界或 SSH 安全契约时 | 根 `AGENTS.md` 保留硬约束，细则放入 references |
| `docs/` | 架构、开发、审计和实施记录 | 修改边界、命令或计划时 | 双语页面保持结构对齐 |
| `.github/workflows/` | 三平台 CI | 修改工具链、依赖或验证门禁时 | 不 checkout 参考子模块 |
| `third_package/axshell` | 仅供产品/行为参考的 Git 子模块 | 需要核对参考行为时 | 不进入 Cargo workspace 或 build graph |

## 关键文件

| Path | Role | Key Symbols / Sections | Read For |
| --- | --- | --- | --- |
| `AGENTS.md` | 全仓库持久指令 | 项目边界、所有权、安全和验证 | 开始任何 Rust/Slint/SSH 变更 |
| `.agents/skills/ax-ssh-rust-slint/SKILL.md` | AxSSH 实施与评审工作流 | reference 路由、架构决策、安全门禁 | 修改或评审工程代码与边界 |
| `LICENSE` | AxSSH 主许可证正文 | GNU GPL version 3 | 发布源码或二进制、核对 GPL 条款时 |
| `THIRD_PARTY_NOTICES.md` | 第三方许可入口 | Slint、OFL 字体、MIT vt100、Cargo 依赖 | 修改依赖、字体、vendor 或发行声明时 |
| `Cargo.toml` | 根包、许可证、依赖和 Linux package 定义 | `[package]`、`license`、Slint/russh/终端模拟器、`package.metadata.deb` | 工具链、版本、授权和构建/打包范围 |
| `assets/ion/terminal_icon.svg` | AxSSH Terminal 图标的 canonical 矢量源副本 | 更换或重新生成各平台位图/容器时 | `terminal_icon_all_formats/terminal_icon.svg` 保留同一源副本；所有 PNG、ICO、ICNS 从此 SVG 生成，保持 RGBA 透明背景 |
| `build.rs` | Slint 编译入口 | `slint_build::compile` | UI build 失败或新增入口 |
| `src/main.rs` | 进程入口 | `LoggingGuard`、`app::run` | 启动、退出和进程级生命周期 |
| `src/lib.rs` | 可测试库入口 | `config`、`credentials`、`logging`、`ssh`、`sftp`、`telnet`、`serial` | 领域、系统服务、进程服务和传输公共边界 |
| `src/app.rs` | Slint/Rust bridge 入口 | `slint::include_modules!`、`run`、`wire_callbacks`、About support actions | 生成类型声明、UI 启停、功能 callback 和脱敏诊断复制/外部路径打开 |
| `src/app/font_bridge.rs` | 运行时字体资源与系统等宽字体 bridge | `FontRegistry`、`font_options`、`load_bundled_fonts` | UI 线程注册按需读取的应用/Terminal 自带 TTF；两个下拉固定自带字体在前，Tokio blocking worker 只返回有界系统字体族名称 |
| `src/app/workspace.rs` | 工作区与 profile/group bridge | `wire_workspace_tabs`、`wire_session_editor`、`wire_session_management`、`SessionTransferEnvelope`、`import_session_transfer_into_store`、`close_workspace_tab` | Tab 激活/内存排序、会话 CRUD、编辑器一次性密码/显式保存分流、Group/Server Duplicate、256 KiB/128 profile 脱敏 JSON 复制导入、凭据回滚与关闭时资源回收 |
| `src/app/connection.rs` 与 `src/app/connection/` | 多协议连接控制器组装入口与单一流程模块 | `wire_connection_request`、`request_profile_connection`、`begin_authentication`、`start_telnet_connection`、`start_serial_connection`、`start_session_worker` | `request` 解析 SSH/SFTP companion 导航、分发协议并把编辑器一次性密码绑定到对应 Tab；`direct` 负责 Telnet/Serial attempt 与 monitor；其余模块保持 SSH probe、信任、认证和 worker phase；未知或变化密钥仍须明确确认 |
| `src/app/connection_monitor.rs` | SSH worker 事件消费 | `spawn_session_monitor`、`persist_authenticated_credential` | attempt 路由、带接收时间的输出/失败事件、认证成功后的凭据保存，以及迟到 worker/凭据结果隔离 |
| `src/app/terminal_bridge.rs` | 终端与本地 shell bridge | `wire_terminal`、`start_local_shell`、`spawn_local_shell_monitor` | 终端输入/selection、本地 worker 事件和仅视觉主题刷新；resize 意图只进入 `AppState` 单一入口 |
| `src/app/settings_bridge.rs` | Settings 预览/保存 bridge | `wire_settings`、`apply_preview_settings` | 合并草稿即时更新内存/Theme/终端/布局；新自带字体异步注册；关闭请求才原子保存并关闭指定 Settings Tab |
| `src/app/view.rs` | Slint model/snapshot/主题映射 | `session_group_rows`、`dispatch_terminal_output_snapshot`、`apply_active_snapshot`、`apply_security_prompt`、`apply_theme_to_component`、`load_x11_server_installations` | 嵌套 Group/profile model、终端渲染 DTO、font options、output-to-UI latency、Light/Dark 双侧主题、活动 Tab 安全覆盖层，以及 X server 安装位置的 blocking 快照和 event-loop 更新 |
| `src/app/input.rs` | Slint 输入边界映射 | `terminal_key_from_slint`、`format_shortcut_event`、`menu_shortcut_from_setting` | 特殊键（含 F1-F12）、快捷键、原生菜单 `slint::Keys` 和 Apple 物理修饰键还原 |
| `src/app/diagnostics.rs` | 脱敏键盘、UI action 和输入耗时日志边界 | `log_keyboard_event`、`log_terminal_input`、`log_terminal_input_latency`、`log_ui_action` | 调试快捷键/功能调用/输入请求耗时；文字只记 `Text`，不得增加内容长度、路径、主机、名称、Clipboard 或秘密字段 |
| `src/app/macos_window.rs` | macOS 原生窗口/菜单 bridge | `configure`、`current_modifier_state`、`configure_application_menu`、`NativeMenuTarget` | 标准标题栏、当前物理修饰键，以及带实时 key equivalent 的 Settings/About 幂等绑定与主线程生命周期 |
| `src/app/state.rs` | 工作区 Tab、终端、编辑器 draft、transport route 与 SSH phase 所有权 | `AppState`、`switch_ssh_sftp_tab`、`SshSftpNavigation`、`resize_active_terminal`、`TerminalBackend`、`TerminalWorker`、`TerminalTabState`、`SshConnectionPhase` | Tab 创建/切换/关闭、运行时 companion UUID 配对与邻接插入、同 profile 多实例、逐 Tab attempt、活动 worker 与本地模型 resize 编排、SSH 安全 phase、对应 Tab 的短期一次性认证秘密和编辑草稿身份；配对与秘密都不持久化，也不共享 live transport |
| `src/app/state/transitions.rs` | SSH attempt 与 credential phase 状态转换 | retry/retire/credential storage helpers | 迟到 worker/凭据结果隔离、认证/host-key 重试、仅仍在 loading phase 的后端引用清理 |
| `src/app/state/tests.rs` | 应用状态回归 | singleton、duplicate tab、SSH/SFTP companion、active prompt、attempt/credential isolation tests | 修改 Tab、配对导航、认证 phase 或 attempt 生命周期后 |
| `src/app/session_groups.rs` | 会话分组/展示/编辑辅助逻辑 | `session_groups`、`group_options`、`compact_label`、`profile_endpoint`、`profile_sidebar_endpoint`、`profile_sidebar_details` | 持久化空 Group 与 profile 聚合、编辑器组选项、文字徽标、行内脱敏 sidebar endpoint 与完整非秘密悬停详情 |
| `src/app/credential_tasks.rs` | 凭据异步边界 | Tokio `spawn_blocking` + timeout、rollback | 在 UI 线程外调用系统凭据库和加密保险库；短期秘密用 `Zeroizing` 跨任务并在 profile 事务失败时恢复记录 |
| `src/app/serial_bridge.rs` | Serial 自动发现 bridge | `wire_serial_port_discovery`、`refresh_serial_ports` | 启动/手动刷新时在 blocking task 枚举 descriptor 并更新有界 Slint 端口 model；不得打开设备 |
| `src/app/sftp_bridge.rs` | SFTP Tab 浏览与本地目录 intent bridge | `wire_sftp` | 校验活动 SSH/SFTP Tab，转发远端目录/分页、前进后退和远端/本地选中意图；不持有协议 session |
| `src/app/local_files.rs` | 本机目录只读 snapshot 边界 | `read_local_directory`、`LocalDirectoryListing` | 修改 SFTP 本地栏目录读取、路径或条目/名称预算时 |
| `src/config.rs` 与 `src/config/` | 稳定 config 入口及 session、settings、theme、persistence、回归测试模块 | `ConnectionProfile`、`SshConfig`、`TelnetConfig`、`SerialConfig`、`SessionStore`、`AppSettings`、`ShortcutSettings`、`ConfigStore` | schema v17 终端最小对比度替换旧亮度字段；schema v16 收起组名字符数；Import/Export 快捷键和 `SshConfig.x11_forwarding` 用字段级默认值兼容旧配置；SSH-only 安全字段由 variant 隔离 |
| `src/credentials.rs` | 系统凭据和加密保险库边界 | `CredentialStore` | Keychain/Credential Manager/Secret Service，以及 Argon2id + XChaCha20-Poly1305 profile 记录；回滚期间清零系统密码副本 |
| `src/terminal.rs` | 有界终端文本模型 | `TerminalModel`、Alacritty `Term`/`Processor` 映射 | shell 输出解析、主屏 reflow、光标和 scrollback |
| `src/terminal/input.rs` | 与 UI 无关的终端按键编码 | `TerminalKey`、`TerminalModifiers`、`encode_key` | 控制字节、application-cursor Home/End、导航/Function 键和 xterm 修饰序列 |
| `src/logging.rs` | 进程级 tracing 生命周期 | `LoggingGuard`、滚动 writer | 日志目录、过滤、保留、退出 flush，以及向 About 提供已创建目录 |
| `src/x_server.rs` | 跨平台本机 X server 选择、位置快照与启动边界 | `provider_options`、`provider_index`、`discovered_provider_locations`、`XServerPlan` | macOS bundle/Windows PATH 与 Program Files 系统发现、只读已知位置快照、Custom executable、display 候选和首个 X11 channel 时的有界启动；不持有 SSH channel、cookie 或 UI 状态 |
| `src/ssh.rs` | russh 传输边界 | `client_config`、`ClientHandler`、`SshConnection`、`SshSessionHandle` | `TCP_NODELAY`、主机密钥、认证、默认拒绝/有界转交服务端 X11 channel、取消和连接 worker；不把静默 shell 当作断连 |
| `src/ssh/private_keys.rs` | 本机私钥边界 | `discover_private_keys`、`load_private_key` | `.ssh` 发现、blocking 加载和 `Zeroizing` passphrase |
| `src/ssh/worker.rs` | SSH/SFTP-only worker 生命周期 | `SshSessionHandle`、`SshSessionMode`、`SshSessionEvent`、`flush_output` | 有界 shell/SFTP-only 连接、仅在服务端打开 X11 channel 时开始本机准备和 relay、即时输入、输入后首个输出刷新、脱敏 latency、取消、退出 join 和短期秘密 |
| `src/ssh/x11.rs` | SSH X11 forwarding 协议与本机 X server relay 边界 | `X11Forwarding`、DISPLAY/xauth 解析、`X11Session`、setup cookie rewrite、local endpoint connect、relay | shell 创建只生成单次 fake cookie；首个服务端 X11 channel 才解析本机 DISPLAY/xauth/端点并按需启动，默认要求精确 MIT-MAGIC-COOKIE-1，显式兼容时仍先验证 fake cookie；所有读取、连接、队列与 relay 有界且可取消 |
| `src/ssh/tests.rs` | 确定性 SSH 回归 | loopback russh server | 主机密钥、密码/私钥认证、PTY shell、静默 shell 和 worker 生命周期测试 |
| `src/sftp.rs` | SFTP v3 只读目录浏览 domain | `SftpBrowserHandle`、`SftpBrowserEvent`、`PacketLimitedStream` | 在该 SFTP worker 独占的已认证 transport 上打开子 channel、路径/名称校验、raw 目录游标、分页/目录预算、packet/请求/关闭上限和定向测试 |
| `src/telnet.rs` | Telnet transport 与 worker | `TelnetFrameBuffer`、`TelnetSessionHandle`、`TelnetSessionEvent` | 明文 TCP、64 KiB 完整帧组装、IAC 过滤、选项拒绝、NAWS、有界队列、取消和分片/loopback 回归 |
| `src/serial.rs` | Serial 发现、身份解析与 worker | `discover_serial_ports`、`resolve_serial_port`、`SerialSessionHandle` | 只读枚举、稳定 USB 匹配、歧义拒绝、串口参数、设备 I/O 和有界关闭；发现不得自动打开 |
| `ui/app.slint` | Rust-facing 主窗口、菜单栏和顶层 Slint 转发 | `AppWindow`、`WorkspaceViewState`、`SecurityOverlayViewState`、`MenuBar` | Rust property/callback 接口、File 菜单 New/Import/Export、统一 Switch SSH/SFTP Tab 菜单语义与可配置快捷键、DTO 组装、主题刷新和安全 phase 输入；不在此保存局部草稿或普通弹层状态 |
| `ui/workspace-shell.slint` | 工作区局部组合和短暂 UI 状态 | `WorkspaceShell`、`WorkspaceViewState` | 侧栏收起、连接选择器、SFTP 运行时分栏比例/Transfers 折叠、Tab/内容组合和工作区 callback 转发；接收只读 Profile/Tab/终端/设置快照 |
| `ui/components/workspace-titlebar.slint` | 工作区标题栏组件 | `WorkspaceTitlebar`、`WorkspaceTabContent`、`WorkspaceTabRow`、`ConnectableSessionRow`、`ConnectionPicker` | 右侧工作区列的左起 Tab strip、跟随指针的拖拽副本/源槽/目标槽、位置序号、最右侧保存连接选择器、滚动和关闭 |
| `ui/components/flat-action-menu.slint` | 通用扁平动作菜单 | `ActionMenuItem`、`FlatActionMenu`、`show-at` | 用同一 action model 承载原生右键菜单与按钮主动触发的下拉菜单 |
| `ui/components/session-navigation.slint` | 会话导航组件 | `SessionNavigation`、`SessionNavigationGroup`、`CompactSessionNavigationGroup`、`SessionProfileRow`、`SessionGroupRow`、`SessionActionMenu` | 全高/紧凑侧栏共享本地选择身份和 Group 展开；对象菜单保留 Copy/Duplicate/Open SFTP，File 菜单调用其 Import/Export 函数 |
| `ui/components/session-management-dialog.slint` | Group/profile 管理覆盖层 | `SessionManagementDialog` | 新建/重命名 Group 输入，以及删除 Group/profile 的明确确认语义 |
| `ui/components/overlay-host.slint` | 覆盖层组合与普通会话管理弹层状态 | `OverlayHost`、`SecurityOverlayViewState` | Group/Profile 管理 action、标题/文案/草稿；HostKey/Authentication 只读消费 Rust 安全 phase 并转发确认/取消意图 |
| `ui/components/themed-combo-box.slint` | AxSSH 主题化共享下拉 | `ThemedComboBox`、`ComboBoxChevron` | 需要控件本体、弹层、选中/hover、焦点和滚动指示完全消费语义主题色时 |
| `ui/components/flat-text-input.slint` | AxSSH 主题化共享非秘密文本输入 | `FlatTextInput` | Settings、会话编辑器和管理弹窗的单行编辑、原生文本选择和编辑菜单 |
| `ui/components/secret-text-input.slint` | AxSSH 专用秘密输入 | `SecretTextInput` | 密码遮蔽、IME/focus、不可读取的可访问性语义、禁止复制/剪切/鼠标选择泄漏 |
| `ui/components/security-dialogs.slint` | 安全覆盖层组件 | `HostKeyDialog`、`AuthenticationDialog`、`prompt-id`、`selected-credential-storage` | host-key 确认、系统凭据/加密保险库密码、私钥 passphrase UI；普通密码弹窗从 General 初始化后端选择并允许本次覆盖；提交接收、取消或切换 prompt 时清空秘密 |
| `ui/components/terminal-grid.slint` | 有界终端网格渲染与指针/菜单意图组件 | `TerminalGrid`、`TerminalGridView`、`TerminalSelectionView`、`TerminalRenderLine` | 绘制底部对齐的 Rust 可见字符格、选区/光标/preedit 覆盖，并转换指针、滚动、复制/粘贴/全选意图；不持有终端数据、worker 状态、焦点或 IME 输入 |
| `ui/terminal-pane.slint` | 当前终端 Tab 视图 | `TerminalPane`、`TerminalViewState`、`grid-top-offset` | 只读终端 snapshot、局部焦点/选择/光标/尺寸、特殊键 callback 与原生文本/IME 分流、底部对齐的网格余量、复制粘贴和 PTY resize callback |
| `ui/sftp-pane.slint` | 独立 SFTP 双栏文件工作区 | `SftpPane`、`SftpDirectoryPane`、`SftpTransferQueue`、私有 `SftpSplitHandle` | 受最小尺寸约束的远端/本地与文件区/Transfers splitter、固定文件表列、远端前进后退、行选中/全选、隐藏文件过滤、远端分页和只读队列状态；不执行文件系统、网络或传输 |
| `ui/theme.slint` | 运行时视觉 token 解析器 | `Theme` Light/Dark 双侧 palette、`application-font-family`、`resolved-dark`、状态/type/spacing/geometry tokens | 修改应用字体、系统色响应、语义色、边框/焦点/hover/selected 状态或标准界面尺寸 |
| `ui/components/sidebar-controls.slint` | 会话导航基础图标/窄栏项 | `SidebarTerminalGlyph`、`SidebarToggleGlyph`、`SidebarRailToggle`、`SidebarRailItem` | 修改独立侧栏开关的 rail/行内尺寸、Local Shell 图标及带可访问展开语义的收起态 Group/服务器项 |
| `ui/components/settings-controls.slint` | Settings 基础组件集 | `SettingsNavIcon`、`SettingsNavGlyph`、`SettingsNavItem`、`SettingsHeader`、`SettingsField`、`SettingsRow` | 统一 Settings 矢量图标、导航、标题、紧凑字段和行布局；标题不再承载 Save/Close 状态操作 |
| `ui/settings.slint` | Settings 工作台编排 | `SettingsPane`、`SettingsViewState`、统一草稿、`commit-settings`、`request-close` | 只读设置源、组件私有分类选择/跨页草稿；标签页关闭请求提交保存并由 Rust 在成功后关闭 |
| `ui/settings/` | Settings 分类页面 | `AppearanceSettingsPage`、`TerminalSettingsPage`、`X11SettingsPage`、`ThemePaletteEditor`、`AboutSettingsPage`、General/Workspace/Shortcuts/About | Appearance 拥有应用字体/主题；Terminal 拥有终端设置；X11 拥有本机 provider/path/启动/兼容选择；About 展示 GPL、标准 `AboutSlint` 和问题/日志/诊断操作 |
| `ui/session-editor.slint` | 新建/编辑 Session Tab | `SessionEditorPane`、`SessionEditorViewState`、`draft-id`、`submit`、`x11_forwarding` | 只读传入 draft identity、组件私有 profile 草稿、SSH-only X11 toggle、内嵌遮蔽密码/保险库口令（空值保留、非空更新后端）、预选 Group 和私钥路径；新建 SSH 可 Save & connect |
| `docs/architecture.zh.md` | 当前架构契约 | 模块职责、事件流、安全契约 | 跨模块设计和扩展 |

## 常用定位

- 修改会话或设置字段：先从 `src/config.rs` 定位至 `src/config/{session,settings,theme,persistence}.rs`，再同步 `src/app/settings_bridge.rs`、`src/app/view.rs` 和对应 `ui/settings/*.slint` 映射；收起组名字符数由 `WorkspaceSettings` 校验并通过 `SessionNavigation` 消费；默认凭据后端位于 Settings > General，既有 profile 使用自身的非敏感后端引用。
- 修改连接或认证：先从 `src/app/connection.rs` 定位至 `src/app/connection/{request,direct,host_key,authentication,worker_start}.rs`；保存并连接必须在 profile 持久化成功后复用 request 路由，认证弹窗的后端选择只在成功且勾选记住后进入 `PendingCredentialStore`；SSH 再检查 `src/ssh.rs`、`src/ssh/worker.rs`、`src/ssh/x11.rs` 和 `src/x_server.rs`，保持未知密钥默认拒绝、X11 profile 默认开启但普通 SSH 建连不读 DISPLAY/xauth/启动 provider、首个远端 X11 channel 准备失败不阻断 shell，以及本机 cookie/relay/启动上限；Telnet/Serial 分别检查 `src/{telnet,serial}.rs`，保持明文提示和只发现不自动打开。
- 修改 SFTP 浏览、分栏或 SSH/SFTP Tab 切换：先检查 `src/sftp.rs` 的 packet/path/page/directory 上限和 `src/app/local_files.rs` 的本地目录预算，再检查 `src/ssh/worker.rs` 的 SFTP-only worker 生命周期、`src/app/state.rs` 的运行时 companion UUID、`src/app/{connection,sftp_bridge,connection_monitor,view}.rs` 的逐 Tab 路由/snapshot，最后修改 `ui/{app,workspace-shell,sftp-pane,components/session-navigation}.slint`；分栏比例和 Transfers 折叠只属于 `WorkspaceShell` 运行时 UI 状态，不进入配置或 Rust；配对不能共享 russh handle，上传/下载/修改必须另建确认、进度、取消和冲突契约。
- 修改终端解析、scrollback 或 resize：`src/terminal.rs`；主屏使用 `alacritty_terminal::Term::resize` 的重排语义，备用屏不重排。UI resize 只从 `src/app/terminal_bridge.rs` 进入 `AppState::resize_active_terminal`，由状态层依次请求 worker 并更新本地模型；worker/Slint 只消费既有 `TerminalSnapshot`。修改按键序列：`src/terminal/input.rs`，并同步 `src/app/input.rs`、`ui/terminal-pane.slint` 与 `TerminalSettings::option_as_meta`；修改终端网格绘制、指针或菜单意图时还需检查 `ui/components/terminal-grid.slint`，其中完整行向下对齐且顶部余量必须同步 IME/预编辑和指针坐标；修改 shell I/O：`src/ssh/worker.rs`。
- 修改终端/工作区设置：`src/config/{settings,theme}.rs`、`src/app.rs`、`ui/settings.slint` 和 `ui/workspace-shell.slint`；跨组件新增字段优先扩展相应 `*ViewState`，不要重新逐条透传；字体资源同时检查 `assets/fonts/` 的许可证/声明和 `src/app/font_bridge.rs` 的运行时注册/有界发现。
- 修改 Tab 生命周期或排序：`src/app/state.rs`、`src/app/state/transitions.rs` 和 `src/app/workspace.rs`；运行实例键及 SSH/SFTP companion 配对保持为 Tab UUID，不能退回 profile UUID，位置序号只由 Slint 列表索引派生。关闭一端必须解除另一端 companion 引用但不得联动关闭；SSH probe、host-key 确认、认证与 stored-credential loading 也必须随 Tab 迁移，并让迟到 completion 重验 Tab/profile/attempt/phase。
- 修改本机私钥发现或加载：`src/ssh/private_keys.rs`，不得持久化私钥内容或 passphrase。
- 修改日志初始化、滚动或刷新：`src/logging.rs`，由 `src/main.rs` 持有唯一 guard；修改键盘/UI action diagnostics：`src/app/diagnostics.rs`、`src/app/input.rs` 与对应 callback bridge，保持 `Text` 脱敏和固定 action 字段；修改 SSH latency 时还需检查 `src/ssh/worker.rs`、`src/app/connection_monitor.rs` 和 `src/app/view.rs`，不得把时间关联写成回显确认。
- 修改 About 支持动作或构建诊断：先检查 `ui/settings/about.slint`、`ui/settings.slint`、`ui/workspace-shell.slint`、`ui/app.slint` 的 callback 链，再检查 `src/app.rs`、`src/main.rs`、`build.rs` 和日志目录生命周期；诊断剪贴板只能包含版本、revision、系统、架构和构建类型，不得自动上传日志。
- 修改 UI callback：先改对应 `ui/*.slint` 契约，再改 `src/app.rs::wire_callbacks` 调用的功能 bridge 模块。
- 修改平台顶部菜单：业务菜单和 `MenuItem.shortcut` 在 `ui/app.slint`，配置字符串到 `slint::Keys` 的唯一转换在 `src/app/input.rs`，macOS 标准应用菜单 action/key equivalent 在 `src/app/macos_window.rs`；File 的 Import/Export 通过 `WorkspaceShell` 调用 `SessionNavigation` 的公开函数，菜单不能越过根组件访问内部实例。macOS 菜单不得依赖活动 Tab/SFTP 状态；快捷键或安全状态重建后必须幂等重绑 Settings/About。
- 修改会话管理：`src/config.rs` 持久化 Group/profile，`src/app/workspace.rs` 执行 CRUD、Duplicate、脱敏 clipboard JSON 复制/导入与凭据事务，`src/app/state.rs` 只持有编辑 draft identity，`src/app/view.rs::session_group_rows` 生成嵌套的 Group/遮蔽服务器 model；Group 展开与当前 Group/Server 选择身份留在 `ui/components/session-navigation.slint`，不持久化或发送 transport。Slint 入口在 `ui/workspace-shell.slint`、`ui/components/{flat-action-menu,flat-text-input,session-navigation,session-management-dialog,overlay-host}.slint` 和 `ui/session-editor.slint`。导入上限、UUID 重分配和凭据/主机指纹清除不得下放到 Slint。
- 修改主题：先改 `src/config.rs` 的领域/迁移/对比度保护，再同步 `src/app/{settings_bridge,view,terminal_bridge}.rs`、`ui/{app,settings,theme}.slint` 和 Appearance 的共享 `ThemePaletteEditor`；系统跟随只能留在 Slint，不能把平台配色写进配置。需要精确弹层配色的选择控件统一使用 `ui/components/themed-combo-box.slint`，边框/分隔线/焦点/hover/selected 使用 `Theme` 状态 token。
- 修改应用图标资源：先检查 `assets/ion/terminal_icon.svg`，再从该源生成 `assets/ion/terminal_icon_all_formats/` 中的 PNG、ICO 和 ICNS；窗口入口在 `ui/app.slint`，Windows resource 在 `packaging/windows/axssh.rc`，macOS Dock/Bundle 在 `src/app/macos_window.rs` 与 `packaging/macos/`，Linux desktop/deb 安装表在 `packaging/linux/axssh.desktop` 与 `Cargo.toml`。
- 修改许可证或发行声明：先检查 `LICENSE`、`THIRD_PARTY_NOTICES.md` 和 `Cargo.toml` 的 SPDX/安装表，再同步 `ui/settings/about.slint`、根 README、`docs/{architecture,development}*.md` 与平台打包脚本；不得用根 GPL 覆盖 OFL 字体或 MIT vendor 的独立声明。
- 修改 Rust/Slint 工程规则：先读根 `AGENTS.md`，再由项目 skill 选择 Rust 或 Slint reference。
- 修改工程边界：同步根 README、`docs/architecture*.md` 和本项目地图。

## 忽略与未索引

- `target/` 是生成产物，不纳入源码地图。
- `third_package/axshell` 的内部地图由其自身仓库维护；这里只记录引用边界。
- 本机 Cargo registry/cache 只用于验证，不属于仓库事实。

## 刷新规则

- 刷新触发：新增/移动重要模块、改变 UI/worker/存储所有权、变更构建入口、CI 或参考子模块边界。
- 最近依据：2026-08-04 的会话编辑器可选密码保存、逐 Tab 一次性认证秘密，以及 Terminal 最小对比度设置、schema v17 迁移和按实际单元格背景的 WCAG 前景修正；2026-08-03 的运行时 SSH/SFTP companion UUID、双向 Tab 切换、独立 transport 和 Settings 单例快捷键激活，以及 X11 首个远端 channel 按需本机准备、X server 已知安装位置快照和 Custom executable 设置入口；2026-08-02 的跨平台 X server provider/启动设置、默认开启的 SSH X11 forwarding、显式 loopback no-auth 兼容；同日的原生可配置菜单 accelerator、活动 Tab 无关的 macOS 菜单与 Settings/About 重建后幂等绑定、GPL-3.0-only 主许可证和 Slint 标准 About 署名；2026-08-01 的独立双栏 SFTP Tab、默认 `Ctrl+M`、脱敏 diagnostics、终端主屏 reflow、运行时字体资源和双字体设置分离。

## 最后更新时间

- 2026-08-04 13:52 +0800
