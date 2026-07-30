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
| `src/app/workspace.rs` | 工作区与 profile bridge | `wire_workspace_tabs`、`wire_session_editor`、`close_workspace_tab` | Tab 激活/内存排序命令、可见单例 Settings、会话保存和关闭时资源回收 |
| `src/app/connection.rs` | SSH 连接控制器 | `wire_connection_request`、`begin_authentication`、`start_session_worker` | 主机探测、信任确认、认证和 worker 启动 |
| `src/app/connection_monitor.rs` | SSH worker 事件消费 | `spawn_session_monitor`、`persist_authenticated_credential` | attempt 路由、输出/失败事件和认证成功后的凭据保存 |
| `src/app/terminal_bridge.rs` | 终端与本地 shell bridge | `wire_terminal`、`start_local_shell`、`spawn_local_shell_monitor` | 终端输入/resize/selection 和本地 worker 事件 |
| `src/app/settings_bridge.rs` | 设置保存 bridge | `wire_settings` | 校验并原子保存 Settings 草稿 |
| `src/app/view.rs` | Slint model/snapshot 映射 | `refresh_session_models`、`connection_option_rows`、`refresh_workspace`、`apply_active_snapshot`、`dispatch_ui` | 侧栏/连接选择器 UI model、终端渲染 DTO、弹窗和 event-loop 更新 |
| `src/app/input.rs` | Slint 输入边界映射 | `terminal_key_from_slint`、`format_shortcut_event` | 特殊键、快捷键和 Apple 修饰键还原 |
| `src/app/macos_window.rs` | macOS 原生窗口/菜单 bridge | `configure`、`configure_application_menu`、`NativeMenuTarget` | 标准标题栏、应用菜单 Settings/About action 与主线程生命周期 |
| `src/app/state.rs` | 工作区 Tab、终端与临时 Group 展开状态所有权 | `AppState`、`move_tab`、`expanded_groups`、`WorkspaceTab`、`TerminalTabState` | Tab 创建/切换/内存排序/关闭、同 profile 多实例和 Group 展开状态 |
| `src/app/state/transitions.rs` | SSH attempt 状态转换 | retry/retire/credential marker helpers | 迟到 worker 隔离、认证/host-key 重试和非敏感 marker 保存 |
| `src/app/state/tests.rs` | 应用状态回归 | singleton、duplicate tab、attempt isolation tests | 修改 Tab 或 attempt 生命周期后 |
| `src/app/session_groups.rs` | 会话分组/展示/编辑辅助逻辑 | `session_groups`、`group_options`、`compact_label`、`profile_endpoint`、`profile_sidebar_endpoint` | Group 聚合、文字徽标、编辑器已有组选项、精确连接 endpoint 与遮蔽侧栏 endpoint 格式 |
| `src/app/credential_tasks.rs` | 凭据异步边界 | Tokio `spawn_blocking` + timeout | 从 UI 线程外调用系统凭据库 |
| `src/config.rs` | 会话/设置 schema 和持久化 | `SessionProfile`、`AppSettings`、`ConfigStore` | profile、终端/工作区参数、旧配置迁移、校验和版本化 JSON 写入 |
| `src/credentials.rs` | 平台系统凭据边界 | `CredentialStore` | Keychain/Credential Manager/Secret Service 密码读写 |
| `src/terminal.rs` | 有界终端文本模型 | `TerminalModel`、ANSI `Perform` | shell 输出解析、光标和 scrollback |
| `vendor/vt100/src/grid.rs` | 终端依赖的受控修复点 | `Grid::set_size` | 宽字符被列缩窄截断时保持 normal/alternate grid 有效 |
| `src/terminal/input.rs` | 与 UI 无关的终端按键编码 | `TerminalKey`、`TerminalModifiers`、`encode_key` | 控制字节、导航键和 xterm 修饰序列 |
| `src/logging.rs` | 进程级 tracing 生命周期 | `LoggingGuard`、滚动 writer | 日志目录、过滤、保留和退出 flush |
| `src/ssh.rs` | russh 传输边界 | `ClientHandler`、`SshConnection`、`SshSessionHandle` | 主机密钥、认证、取消和连接 worker |
| `src/ssh/private_keys.rs` | 本机私钥边界 | `discover_private_keys`、`load_private_key` | `.ssh` 发现、blocking 加载和 passphrase |
| `src/ssh/worker.rs` | SSH worker 生命周期 | `SshSessionHandle`、`SshSessionEvent` | 有界 shell 输入/resize/输出、取消和退出 join |
| `src/ssh/tests.rs` | 确定性 SSH 回归 | loopback russh server | 主机密钥、密码/私钥认证、PTY shell 和 worker 生命周期测试 |
| `ui/app.slint` | 主窗口、菜单栏和 Tab 交互契约 | `AppWindow`、`MenuBar`、`ConnectionPicker`、安全提示层 | 全高会话侧栏、右侧 Tab 激活/排序 callback、已保存连接选择器和活动页面 |
| `ui/components/workspace-titlebar.slint` | 工作区标题栏组件 | `WorkspaceTitlebar`、`WorkspaceTabContent`、`WorkspaceTabRow`、`ConnectableSessionRow`、`ConnectionPicker` | 右侧工作区列的左起 Tab strip、跟随指针的拖拽副本/源槽/目标槽、位置序号、最右侧保存连接选择器、滚动和关闭 |
| `ui/components/session-navigation.slint` | 会话导航组件 | `SessionNavigation`、`SessionNavigationRow`、`GroupDisclosureChevron`、`SessionRow` | 原生标题栏下方贯穿全高的 Local Shell 行内独立侧栏开关、Group 折叠、绘制上下尖角、紧凑栏文字徽标、单行服务器与 Local Shell 交互 |
| `ui/components/security-dialogs.slint` | 安全覆盖层组件 | `HostKeyDialog`、`AuthenticationDialog` | host-key 确认、密码和私钥 passphrase UI |
| `ui/terminal-pane.slint` | 当前终端 Tab 视图 | `TerminalPane` | 终端焦点、复制粘贴、输出跟随和 PTY resize |
| `ui/theme.slint` | 集中式视觉配置 | `Theme` semantic palette/type/spacing/geometry tokens | 修改颜色、字号、间距、圆角或标准界面尺寸 |
| `ui/components/sidebar-controls.slint` | 会话导航基础图标/窄栏项 | `SidebarTerminalGlyph`、`SidebarToggleGlyph`、`SidebarRailToggle`、`SidebarRailItem` | 修改独立侧栏开关的 rail/行内尺寸、Local Shell 图标及可键盘操作的收起态 Group/服务器项 |
| `ui/components/settings-controls.slint` | Settings 基础组件集 | `SettingsNavIcon`、`SettingsNavGlyph`、`SettingsNavItem`、`SettingsHeader`、`SettingsField`、`SettingsRow` | 统一 Settings 矢量图标、导航、标题操作、紧凑字段和行布局 |
| `ui/settings.slint` | Settings 工作台编排 | `SettingsPane`、统一草稿、`commit-settings` | 分类导航、跨页草稿和一次 Save 事务 |
| `ui/settings/` | Settings 分类页面 | General/Appearance/Terminal/Workspace/Shortcuts/About components | 修改单一设置分类布局或字段组合 |
| `ui/session-editor.slint` | New Session Tab | `SessionEditorPane` | profile、分组、密码/私钥表单 |
| `docs/architecture.zh.md` | 当前架构契约 | 模块职责、事件流、安全契约 | 跨模块设计和扩展 |

## 常用定位

- 修改会话或设置字段：`src/config.rs`，再同步 `src/app/settings_bridge.rs`、`src/app/view.rs` 和对应 `ui/settings/*.slint` 映射。
- 修改连接或认证：`src/ssh.rs`，保持未知主机密钥默认拒绝。
- 修改终端解析或 scrollback：`src/terminal.rs`；宽字符缩窄补丁在 `vendor/vt100/src/grid.rs`；修改按键序列：`src/terminal/input.rs`；修改 shell I/O：`src/ssh/worker.rs`。
- 修改终端/工作区设置：`src/config.rs`、`src/app.rs`、`ui/settings.slint`；字体资源同时检查 `assets/fonts/` 的许可证/声明。
- 修改 Tab 生命周期或排序：`src/app/state.rs`、`src/app/state/transitions.rs` 和 `src/app/workspace.rs`；运行实例键保持为 Tab UUID，不能退回 profile UUID，位置序号只由 Slint 列表索引派生。
- 修改本机私钥发现或加载：`src/ssh/private_keys.rs`，不得持久化私钥内容或 passphrase。
- 修改日志初始化、滚动或刷新：`src/logging.rs`，由 `src/main.rs` 持有唯一 guard。
- 修改 UI callback：先改对应 `ui/*.slint` 契约，再改 `src/app.rs::wire_callbacks` 调用的功能 bridge 模块。
- 修改平台顶部菜单：业务菜单在 `ui/app.slint`，macOS 标准应用菜单 action 在 `src/app/macos_window.rs`；优先复用现有 callback 和 Slint 状态。
- 修改会话导航：`src/app/session_groups.rs` 按规范化名称聚合，`src/app/state.rs` 持有临时 Group 展开集合，`src/app/view.rs::session_rows` 生成 Group/遮蔽服务器模型；`ui/components/session-navigation.slint` 呈现上下尖角、文字徽标与单行子项，窄栏基础件在 `ui/components/sidebar-controls.slint`。
- 修改主题或静态界面尺寸：只改 `ui/theme.slint` token；新增 Settings 行优先在 `ui/settings/` 组合 `ui/components/settings-controls.slint`。
- 修改 Rust/Slint 工程规则：先读根 `AGENTS.md`，再由项目 skill 选择 Rust 或 Slint reference。
- 修改工程边界：同步根 README、`docs/architecture*.md` 和本项目地图。

## 忽略与未索引

- `target/` 是生成产物，不纳入源码地图。
- `third_package/axshell` 的内部地图由其自身仓库维护；这里只记录引用边界。
- 本机 Cargo registry/cache 只用于验证，不属于仓库事实。

## 刷新规则

- 刷新触发：新增/移动重要模块、改变 UI/worker/存储所有权、变更构建入口、CI 或参考子模块边界。
- 最近依据：2026-07-30 标准 macOS 原生标题栏与其下方的 Tab 手势隔离、贯穿完整客户端高度的会话侧栏、跟随指针的 Tab 拖拽副本/源槽/目标槽、动态位置序号与最右侧已保存 SSH 连接选择器、始终可见的 Settings/终端 Tab、Local Shell 行内的独立侧栏开关与紧凑 Group rail、Settings 分类页面组件，以及 `vt100 0.16.2` 宽字符缩窄补丁。

## 最后更新时间

- 2026-07-30 16:11 +0800
