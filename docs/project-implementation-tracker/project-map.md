# 项目地图

## 项目概览

- 用途：基于 Slint 与 russh 的跨平台桌面 SSH 工作区。
- 主要入口：`src/main.rs`、`src/app.rs`、`ui/app.slint`、`build.rs`。

## 索引范围

- 根目录：`<repo-root>`
- 覆盖：`AGENTS.md`、`.agents/`、`Cargo.toml`、`Cargo.lock`、`vendor/`、`build.rs`、`src/`、`ui/`、`assets/`、`docs/`、`.github/workflows/ci.yml`、根 README 和 `.gitmodules`。
- 排除：`.git/`、`target/`、`third_package/axshell` 内部文件和本机 Cargo 缓存。

## 目录地图

| Path | Purpose | Open When | Notes |
| --- | --- | --- | --- |
| `src/` | Rust 库边界、进程、UI bridge、配置、系统凭据、终端、日志和 SSH 传输 | 修改行为、状态、存储、凭据、终端、日志或网络时 | `lib.rs` 导出 config/credentials/logging/ssh/terminal；生成的 Slint 类型只在 `app.rs` 使用 |
| `ui/` | Slint 页面、共享 Settings 控件和集中式设计 token | 修改布局、视觉状态、主题或 UI callback 时 | 页面不持有静态颜色/字号/间距字面量，不执行文件系统或网络操作 |
| `assets/fonts/` | 项目自带终端字体及独立许可证/作者声明 | 修改默认字体或打包资源时 | 构建和运行时不读取参考子模块 |
| `vendor/vt100/` | 锁定 `vt100 0.16.2` 的 MIT 许可最小宽字符缩窄补丁 | 修改终端网格缩放或依赖解析时 | 只修正 `Grid::set_size` 的缩窄路径；上游发布同等修复后删除 `[patch.crates-io]` override |
| `.agents/` | 项目级 Codex skills 和按需加载的工程规范 | 修改 Rust、Slint、应用边界或 SSH 安全契约时 | 根 `AGENTS.md` 保留硬约束，细则放入 references |
| `docs/` | 架构、开发、审计和实施记录 | 修改边界、命令或计划时 | 双语页面保持结构对齐 |
| `.github/workflows/` | 三平台 CI | 修改工具链、依赖或验证门禁时 | 不 checkout 参考子模块 |
| `third_package/axshell` | 仅供产品/行为参考的 Git 子模块 | 需要核对参考行为时 | 不进入 Cargo workspace 或 build graph |

## 关键文件

| Path | Role | Key Symbols / Sections | Read For |
| --- | --- | --- | --- |
| `AGENTS.md` | 全仓库持久指令 | 项目边界、所有权、安全和验证 | 开始任何 Rust/Slint/SSH 变更 |
| `.agents/skills/ax-ssh-rust-slint/SKILL.md` | AxSSH 实施与评审工作流 | reference 路由、架构决策、安全门禁 | 修改或评审工程代码与边界 |
| `Cargo.toml` | 根包和依赖定义 | `[package]`、Slint/russh、`[patch.crates-io]`、profiles | 工具链、版本和构建范围 |
| `build.rs` | Slint 编译入口 | `slint_build::compile` | UI build 失败或新增入口 |
| `src/main.rs` | 进程入口 | `LoggingGuard`、`app::run` | 启动、退出和进程级生命周期 |
| `src/lib.rs` | 可测试库入口 | `config`、`credentials`、`logging`、`ssh` | 领域、系统服务、进程服务和传输公共边界 |
| `src/app.rs` | Slint/Rust bridge 入口 | `slint::include_modules!`、`run`、`wire_callbacks` | 生成类型声明、UI 启停和功能 callback 总编排 |
| `src/app/workspace.rs` | 工作区与 profile/group bridge | `wire_workspace_tabs`、`wire_session_editor`、`wire_session_management`、`close_workspace_tab` | Tab 激活/内存排序、会话新增编辑删除、Group CRUD、凭据回滚和关闭时资源回收 |
| `src/app/connection.rs` 与 `src/app/connection/` | SSH 连接控制器组装入口与单一流程模块 | `wire_connection_request`、`wire_host_key_confirmation`、`wire_authentication`、`start_session_worker` | `request` 负责逐 Tab probe 启动，`host_key` 负责显式信任/拒绝，`authentication` 负责临时凭据输入/读取，`worker_start` 在创建 worker 前校验 phase；未知或变化密钥仍须明确确认才可继续 |
| `src/app/connection_monitor.rs` | SSH worker 事件消费 | `spawn_session_monitor`、`persist_authenticated_credential` | attempt 路由、输出/失败事件、认证成功后的凭据保存，以及迟到 worker/凭据结果隔离 |
| `src/app/terminal_bridge.rs` | 终端与本地 shell bridge | `wire_terminal`、`start_local_shell`、`spawn_local_shell_monitor` | 终端输入/resize/selection、本地 worker 事件和仅视觉主题刷新 |
| `src/app/settings_bridge.rs` | 设置保存 bridge | `wire_settings` | 校验并原子保存 Settings 草稿、独立显示模式、palette 和双 Custom 色板 |
| `src/app/view.rs` | Slint model/snapshot/主题映射 | `session_group_rows`、`apply_active_snapshot`、`apply_security_prompt`、`apply_theme_to_component`、`set_theme_palette` | 嵌套 Group/profile model、终端渲染 DTO、Light/Dark 双侧主题、活动 Tab 安全覆盖层和 event-loop 更新 |
| `src/app/input.rs` | Slint 输入边界映射 | `terminal_key_from_slint`、`format_shortcut_event` | 特殊键（含 F1-F12）、快捷键和 Apple 修饰键还原 |
| `src/app/macos_window.rs` | macOS 原生窗口/菜单 bridge | `configure`、`configure_application_menu`、`NativeMenuTarget` | 标准标题栏、应用菜单 Settings/About action 与主线程生命周期 |
| `src/app/state.rs` | 工作区 Tab、终端、编辑器 draft 与 SSH phase 所有权 | `AppState`、`SessionEditorState`、`move_tab`、`TerminalTabState`、`SshConnectionPhase` | Tab 创建/切换/内存排序/关闭、同 profile 多实例、逐 Tab probe/host-key/authentication phase 和编辑草稿身份；不持有局部 Group 展开 |
| `src/app/state/transitions.rs` | SSH attempt 与 credential phase 状态转换 | retry/retire/credential storage helpers | 迟到 worker/凭据结果隔离、认证/host-key 重试、仅仍在 loading phase 的后端引用清理 |
| `src/app/state/tests.rs` | 应用状态回归 | singleton、duplicate tab、active prompt、attempt/credential isolation tests | 修改 Tab、认证 phase 或 attempt 生命周期后 |
| `src/app/session_groups.rs` | 会话分组/展示/编辑辅助逻辑 | `session_groups`、`group_options`、`compact_label`、`profile_endpoint`、`profile_sidebar_endpoint` | 持久化空 Group 与 profile 聚合、编辑器组选项、文字徽标和遮蔽侧栏 endpoint 格式 |
| `src/app/credential_tasks.rs` | 凭据异步边界 | Tokio `spawn_blocking` + timeout、rollback | 在 UI 线程外调用系统凭据库和加密保险库；短期秘密用 `Zeroizing` 跨任务并在 profile 事务失败时恢复记录 |
| `src/config.rs` 与 `src/config/` | 稳定 config 入口及 session、settings、theme、persistence、回归测试模块 | `CredentialStorage`、`ThemeMode`、`ThemePaletteKind`、`ThemePalette`、`ThemeSettings`、`SessionStore`、`AppSettings`、`ConfigStore` | schema v13 `TerminalSettings::option_as_meta`、v12 凭据后端引用/旧标记迁移、v11 主题和 Group/profile 迁移；根文件保留显式 re-export，私有磁盘替换留在 `persistence` |
| `src/credentials.rs` | 系统凭据和加密保险库边界 | `CredentialStore` | Keychain/Credential Manager/Secret Service，以及 Argon2id + XChaCha20-Poly1305 profile 记录；回滚期间清零系统密码副本 |
| `src/terminal.rs` | 有界终端文本模型 | `TerminalModel`、ANSI `Perform` | shell 输出解析、光标和 scrollback |
| `vendor/vt100/src/grid.rs` | 终端依赖的受控修复点 | `Grid::set_size` | 宽字符被列缩窄截断时保持 normal/alternate grid 有效 |
| `src/terminal/input.rs` | 与 UI 无关的终端按键编码 | `TerminalKey`、`TerminalModifiers`、`encode_key` | 控制字节、application-cursor Home/End、导航/Function 键和 xterm 修饰序列 |
| `src/logging.rs` | 进程级 tracing 生命周期 | `LoggingGuard`、滚动 writer | 日志目录、过滤、保留和退出 flush |
| `src/ssh.rs` | russh 传输边界 | `ClientHandler`、`SshConnection`、`SshSessionHandle` | 主机密钥、认证、取消和连接 worker；不把静默 shell 当作断连 |
| `src/ssh/private_keys.rs` | 本机私钥边界 | `discover_private_keys`、`load_private_key` | `.ssh` 发现、blocking 加载和 `Zeroizing` passphrase |
| `src/ssh/worker.rs` | SSH worker 生命周期 | `SshSessionHandle`、`SshSessionEvent` | 有界 shell 输入/resize/输出、取消、退出 join 和短期 `Zeroizing` 认证秘密 |
| `src/ssh/tests.rs` | 确定性 SSH 回归 | loopback russh server | 主机密钥、密码/私钥认证、PTY shell、静默 shell 和 worker 生命周期测试 |
| `ui/app.slint` | Rust-facing 主窗口、菜单栏和顶层 Slint 转发 | `AppWindow`、`WorkspaceViewState`、`SecurityOverlayViewState`、`MenuBar` | Rust property/callback 接口、根级菜单/快捷键、DTO 组装、主题刷新和安全 phase 输入；不在此保存局部草稿或普通弹层状态 |
| `ui/workspace-shell.slint` | 工作区局部组合和短暂 UI 状态 | `WorkspaceShell`、`WorkspaceViewState` | 侧栏收起、连接选择器、Tab/内容组合、工作区 callback 转发；接收只读 Profile/Tab/终端/设置快照 |
| `ui/components/workspace-titlebar.slint` | 工作区标题栏组件 | `WorkspaceTitlebar`、`WorkspaceTabContent`、`WorkspaceTabRow`、`ConnectableSessionRow`、`ConnectionPicker` | 右侧工作区列的左起 Tab strip、跟随指针的拖拽副本/源槽/目标槽、位置序号、最右侧保存连接选择器、滚动和关闭 |
| `ui/components/flat-action-menu.slint` | 通用扁平动作菜单 | `ActionMenuItem`、`FlatActionMenu`、`show-at` | 用同一 action model 承载原生右键菜单与按钮主动触发的下拉菜单 |
| `ui/components/session-navigation.slint` | 会话导航组件 | `SessionNavigation`、`SessionNavigationGroup`、`CompactSessionNavigationGroup`、`SessionProfileRow`、`SessionGroupRow`、`SessionActionMenu` | 全高/紧凑侧栏的组件本地 Group 展开、会话 action 映射、空白区域新建入口和嵌套 profile model |
| `ui/components/session-management-dialog.slint` | Group/profile 管理覆盖层 | `SessionManagementDialog` | 新建/重命名 Group 输入，以及删除 Group/profile 的明确确认语义 |
| `ui/components/overlay-host.slint` | 覆盖层组合与普通会话管理弹层状态 | `OverlayHost`、`SecurityOverlayViewState` | Group/Profile 管理 action、标题/文案/草稿；HostKey/Authentication 只读消费 Rust 安全 phase 并转发确认/取消意图 |
| `ui/components/themed-combo-box.slint` | AxSSH 主题化共享下拉 | `ThemedComboBox`、`ComboBoxChevron` | 需要控件本体、弹层、选中/hover、焦点和滚动指示完全消费语义主题色时 |
| `ui/components/flat-text-input.slint` | AxSSH 主题化共享非秘密文本输入 | `FlatTextInput` | Settings、会话编辑器和管理弹窗的单行编辑、原生文本选择和编辑菜单 |
| `ui/components/secret-text-input.slint` | AxSSH 专用秘密输入 | `SecretTextInput` | 密码遮蔽、IME/focus、不可读取的可访问性语义、禁止复制/剪切/鼠标选择泄漏 |
| `ui/components/security-dialogs.slint` | 安全覆盖层组件 | `HostKeyDialog`、`AuthenticationDialog`、`prompt-id` | host-key 确认、系统凭据/加密保险库密码、私钥 passphrase UI；提交接收、取消或切换 prompt 时清空秘密 |
| `ui/components/terminal-grid.slint` | 有界终端网格渲染与指针/菜单意图组件 | `TerminalGrid`、`TerminalGridView`、`TerminalSelectionView`、`TerminalRenderLine` | 绘制 Rust 所有的可见字符格、选区/光标/preedit 覆盖，并转换指针、滚动、复制/粘贴/全选意图；不持有终端数据、worker 状态、焦点或 IME 输入 |
| `ui/terminal-pane.slint` | 当前终端 Tab 视图 | `TerminalPane`、`TerminalViewState` | 只读终端 snapshot、局部焦点/选择/光标/尺寸、特殊键 callback 与原生文本/IME 分流、复制粘贴和 PTY resize callback |
| `ui/theme.slint` | 运行时视觉 token 解析器 | `Theme` Light/Dark 双侧 palette、`resolved-dark`、状态/type/spacing/geometry tokens | 修改系统色响应、语义色、边框/焦点/hover/selected 状态或标准界面尺寸 |
| `ui/components/sidebar-controls.slint` | 会话导航基础图标/窄栏项 | `SidebarTerminalGlyph`、`SidebarToggleGlyph`、`SidebarRailToggle`、`SidebarRailItem` | 修改独立侧栏开关的 rail/行内尺寸、Local Shell 图标及带可访问展开语义的收起态 Group/服务器项 |
| `ui/components/settings-controls.slint` | Settings 基础组件集 | `SettingsNavIcon`、`SettingsNavGlyph`、`SettingsNavItem`、`SettingsHeader`、`SettingsField`、`SettingsRow` | 统一 Settings 矢量图标、导航、标题操作、紧凑字段和行布局 |
| `ui/settings.slint` | Settings 工作台编排 | `SettingsPane`、`SettingsViewState`、统一草稿、`commit-settings` | 只读设置源、组件私有分类选择/跨页草稿和一次 Save 事务 |
| `ui/settings/` | Settings 分类页面 | `AppearanceSettingsPage`、`ThemePaletteEditor`、General/Terminal/Workspace/Shortcuts/About | 修改显示模式、palette、Custom 双侧字段或其它单一设置分类布局 |
| `ui/session-editor.slint` | 新建/编辑 Session Tab | `SessionEditorPane`、`SessionEditorViewState`、`draft-id` | 只读传入 draft identity、组件私有 profile 草稿、预选 Group 和私钥路径；密码只在连接弹窗输入 |
| `docs/architecture.zh.md` | 当前架构契约 | 模块职责、事件流、安全契约 | 跨模块设计和扩展 |

## 常用定位

- 修改会话或设置字段：先从 `src/config.rs` 定位至 `src/config/{session,settings,theme,persistence}.rs`，再同步 `src/app/settings_bridge.rs`、`src/app/view.rs` 和对应 `ui/settings/*.slint` 映射；默认凭据后端位于 Settings > General，既有 profile 使用自身的非敏感后端引用。
- 修改连接或认证：先从 `src/app/connection.rs` 定位至 `src/app/connection/{request,host_key,authentication,worker_start}.rs`，再检查 `src/ssh.rs`；保持未知主机密钥默认拒绝，秘密输入只可使用 `ui/components/secret-text-input.slint`，不得退回 `FlatTextInput`。
- 修改终端解析或 scrollback：`src/terminal.rs`；宽字符缩窄补丁在 `vendor/vt100/src/grid.rs`；修改按键序列：`src/terminal/input.rs`，并同步 `src/app/input.rs`、`ui/terminal-pane.slint` 与 `TerminalSettings::option_as_meta`；修改终端网格绘制、指针或菜单意图时还需检查 `ui/components/terminal-grid.slint`；修改 shell I/O：`src/ssh/worker.rs`。
- 修改终端/工作区设置：`src/config/{settings,theme}.rs`、`src/app.rs`、`ui/settings.slint` 和 `ui/workspace-shell.slint`；跨组件新增字段优先扩展相应 `*ViewState`，不要重新逐条透传；字体资源同时检查 `assets/fonts/` 的许可证/声明。
- 修改 Tab 生命周期或排序：`src/app/state.rs`、`src/app/state/transitions.rs` 和 `src/app/workspace.rs`；运行实例键保持为 Tab UUID，不能退回 profile UUID，位置序号只由 Slint 列表索引派生。SSH probe、host-key 确认、认证与 stored-credential loading 也必须随 Tab 迁移，并让迟到 completion 重验 Tab/profile/attempt/phase。
- 修改本机私钥发现或加载：`src/ssh/private_keys.rs`，不得持久化私钥内容或 passphrase。
- 修改日志初始化、滚动或刷新：`src/logging.rs`，由 `src/main.rs` 持有唯一 guard。
- 修改 UI callback：先改对应 `ui/*.slint` 契约，再改 `src/app.rs::wire_callbacks` 调用的功能 bridge 模块。
- 修改平台顶部菜单：业务菜单在 `ui/app.slint`，macOS 标准应用菜单 action 在 `src/app/macos_window.rs`；菜单只能调用 `AppWindow` 的公开 property/callback/function，不能越过根组件访问内部实例。
- 修改会话管理：`src/config.rs` 持久化 Group/profile，`src/app/workspace.rs` 执行 CRUD 与凭据事务，`src/app/state.rs` 只持有编辑 draft identity，`src/app/view.rs::session_group_rows` 生成嵌套的 Group/遮蔽服务器 model；Group 展开留在 `ui/components/session-navigation.slint`。Slint 入口在 `ui/workspace-shell.slint`、`ui/components/{flat-action-menu,flat-text-input,session-navigation,session-management-dialog,overlay-host}.slint` 和 `ui/session-editor.slint`。
- 修改主题：先改 `src/config.rs` 的领域/迁移/对比度保护，再同步 `src/app/{settings_bridge,view,terminal_bridge}.rs`、`ui/{app,settings,theme}.slint` 和 Appearance 的共享 `ThemePaletteEditor`；系统跟随只能留在 Slint，不能把平台配色写进配置。需要精确弹层配色的选择控件统一使用 `ui/components/themed-combo-box.slint`，边框/分隔线/焦点/hover/selected 使用 `Theme` 状态 token。
- 修改 Rust/Slint 工程规则：先读根 `AGENTS.md`，再由项目 skill 选择 Rust 或 Slint reference。
- 修改工程边界：同步根 README、`docs/architecture*.md` 和本项目地图。

## 忽略与未索引

- `target/` 是生成产物，不纳入源码地图。
- `third_package/axshell` 的内部地图由其自身仓库维护；这里只记录引用边界。
- 本机 Cargo registry/cache 只用于验证，不属于仓库事实。

## 刷新规则

- 刷新触发：新增/移动重要模块、改变 UI/worker/存储所有权、变更构建入口、CI 或参考子模块边界。
- 最近依据：2026-07-31 的 config/connection/terminal-grid 模块边界重构，`WorkspaceShell`/`OverlayHost`、`TerminalViewState`/`SettingsViewState`/`SessionEditorViewState`、组件私有 picker/普通弹层/草稿/Group 展开与 Rust 单向安全 phase；schema v13 `TerminalSettings::option_as_meta`、特殊键 callback 与文本/IME `edited` 分流、application-cursor Home/End、F1-F12、schema v12 SystemKeyring/EncryptedVault、P1 逐 Tab `SshConnectionPhase`、主题、既有 Group/profile 管理、macOS 标题栏/Tab 拖拽和 `vt100 0.16.2` 宽字符缩窄补丁继续有效。

## 最后更新时间

- 2026-07-31 17:36 +0800
