# 项目地图

## 项目概览

- 用途：基于 Slint 与 russh 的跨平台桌面 SSH 工作区。
- 主要入口：`src/main.rs`、`src/app.rs`、`ui/app.slint`、`build.rs`。

## 索引范围

- 根目录：`<repo-root>`
- 覆盖：`AGENTS.md`、`.agents/`、`Cargo.toml`、`Cargo.lock`、`build.rs`、`src/`、`ui/`、`assets/`、`docs/`、`.github/workflows/ci.yml`、根 README 和 `.gitmodules`。
- 排除：`.git/`、`target/`、`third_package/axshell` 内部文件和本机 Cargo 缓存。

## 目录地图

| Path | Purpose | Open When | Notes |
| --- | --- | --- | --- |
| `src/` | Rust 库边界、进程、UI bridge、配置、系统凭据、终端、日志和 SSH 传输 | 修改行为、状态、存储、凭据、终端、日志或网络时 | `lib.rs` 导出 config/credentials/logging/ssh/terminal；生成的 Slint 类型只在 `app.rs` 使用 |
| `ui/` | Slint 页面和设计 token | 修改布局、视觉状态或 UI callback 时 | 不执行文件系统或网络操作 |
| `assets/fonts/` | 项目自带终端字体及独立许可证/作者声明 | 修改默认字体或打包资源时 | 构建和运行时不读取参考子模块 |
| `.agents/` | 项目级 Codex skills 和按需加载的工程规范 | 修改 Rust、Slint、应用边界或 SSH 安全契约时 | 根 `AGENTS.md` 保留硬约束，细则放入 references |
| `docs/` | 架构、开发、审计和实施记录 | 修改边界、命令或计划时 | 双语页面保持结构对齐 |
| `.github/workflows/` | 三平台 CI | 修改工具链、依赖或验证门禁时 | 不 checkout 参考子模块 |
| `third_package/axshell` | 仅供产品/行为参考的 Git 子模块 | 需要核对参考行为时 | 不进入 Cargo workspace 或 build graph |

## 关键文件

| Path | Role | Key Symbols / Sections | Read For |
| --- | --- | --- | --- |
| `AGENTS.md` | 全仓库持久指令 | 项目边界、所有权、安全和验证 | 开始任何 Rust/Slint/SSH 变更 |
| `.agents/skills/ax-ssh-rust-slint/SKILL.md` | AxSSH 实施与评审工作流 | reference 路由、架构决策、安全门禁 | 修改或评审工程代码与边界 |
| `Cargo.toml` | 根包和依赖定义 | `[package]`、Slint/russh、profiles | 工具链、版本和构建范围 |
| `build.rs` | Slint 编译入口 | `slint_build::compile` | UI build 失败或新增入口 |
| `src/main.rs` | 进程入口 | `LoggingGuard`、`app::run` | 启动、退出和进程级生命周期 |
| `src/lib.rs` | 可测试库入口 | `config`、`credentials`、`logging`、`ssh` | 领域、系统服务、进程服务和传输公共边界 |
| `src/app.rs` | Slint/Rust bridge | `run`、`wire_callbacks`、`set_status` | callback、模型映射和 event loop |
| `src/app/state.rs` | 应用状态转换和工作区 Tab 所有权 | `AppState`、`WorkspaceTab`、`TerminalTabState`、attempt 生命周期 | Tab 创建/切换/关闭、同 profile 多实例和迟到 worker 隔离 |
| `src/app/session_groups.rs` | 会话分组领域逻辑 | `session_groups`、`group_options` | 分组聚合、已有组选项和 endpoint 格式 |
| `src/app/credential_tasks.rs` | 凭据异步边界 | Tokio `spawn_blocking` + timeout | 从 UI 线程外调用系统凭据库 |
| `src/config.rs` | 会话/设置 schema 和持久化 | `SessionProfile`、`AppSettings`、`ConfigStore` | profile、终端/工作区参数、旧配置迁移、校验和版本化 JSON 写入 |
| `src/credentials.rs` | 平台系统凭据边界 | `CredentialStore` | Keychain/Credential Manager/Secret Service 密码读写 |
| `src/terminal.rs` | 有界终端文本模型 | `TerminalModel`、ANSI `Perform` | shell 输出解析、光标和 scrollback |
| `src/terminal/input.rs` | 与 UI 无关的终端按键编码 | `TerminalKey`、`TerminalModifiers`、`encode_key` | 控制字节、导航键和 xterm 修饰序列 |
| `src/logging.rs` | 进程级 tracing 生命周期 | `LoggingGuard`、滚动 writer | 日志目录、过滤、保留和退出 flush |
| `src/ssh.rs` | russh 传输边界 | `ClientHandler`、`SshConnection`、`SshSessionHandle` | 主机密钥、认证、取消和连接 worker |
| `src/ssh/private_keys.rs` | 本机私钥边界 | `discover_private_keys`、`load_private_key` | `.ssh` 发现、blocking 加载和 passphrase |
| `src/ssh/worker.rs` | SSH worker 生命周期 | `SshSessionHandle`、`SshSessionEvent` | 有界 shell 输入/resize/输出、取消和退出 join |
| `src/ssh/tests.rs` | 确定性 SSH 回归 | loopback russh server | 主机密钥、密码/私钥认证、PTY shell 和 worker 生命周期测试 |
| `ui/app.slint` | 主窗口和 Tab 交互契约 | `AppWindow`、`WorkspaceTabRow`、安全提示层 | 顶部 Tab、会话侧栏、活动页面和 callback |
| `ui/terminal-pane.slint` | 当前终端 Tab 视图 | `TerminalPane` | 终端焦点、复制粘贴、输出跟随和 PTY resize |
| `ui/settings.slint` | Settings Tab | `SettingsPane` | 终端/工作区参数表单和保存意图 |
| `ui/session-editor.slint` | New Session Tab | `SessionEditorPane` | profile、分组、密码/私钥表单 |
| `docs/architecture.zh.md` | 当前架构契约 | 模块职责、事件流、安全契约 | 跨模块设计和扩展 |

## 常用定位

- 修改会话或设置字段：`src/config.rs`，再同步 `src/app.rs` 和对应 `ui/*.slint` 映射。
- 修改连接或认证：`src/ssh.rs`，保持未知主机密钥默认拒绝。
- 修改终端解析或 scrollback：`src/terminal.rs`；修改按键序列：`src/terminal/input.rs`；修改 shell I/O：`src/ssh/worker.rs`。
- 修改终端/工作区设置：`src/config.rs`、`src/app.rs`、`ui/settings.slint`；字体资源同时检查 `assets/fonts/` 的许可证/声明。
- 修改 Tab 生命周期：`src/app/state.rs` 和 `src/app.rs`；运行实例键保持为 Tab UUID，不能退回 profile UUID。
- 修改本机私钥发现或加载：`src/ssh/private_keys.rs`，不得持久化私钥内容或 passphrase。
- 修改日志初始化、滚动或刷新：`src/logging.rs`，由 `src/main.rs` 持有唯一 guard。
- 修改 UI callback：先改 `ui/app.slint`，再改 `src/app.rs::wire_callbacks`。
- 修改 Rust/Slint 工程规则：先读根 `AGENTS.md`，再由项目 skill 选择 Rust 或 Slint reference。
- 修改工程边界：同步根 README、`docs/architecture*.md` 和本项目地图。

## 忽略与未索引

- `target/` 是生成产物，不纳入源码地图。
- `third_package/axshell` 的内部地图由其自身仓库维护；这里只记录引用边界。
- 本机 Cargo registry/cache 只用于验证，不属于仓库事实。

## 刷新规则

- 刷新触发：新增/移动重要模块、改变 UI/worker/存储所有权、变更构建入口、CI 或参考子模块边界。
- 最近依据：2026-07-29 顶部 Tab 工作区、同 profile 多终端实例和版本化参数设置。

## 最后更新时间

- 2026-07-29 12:58 +0800
