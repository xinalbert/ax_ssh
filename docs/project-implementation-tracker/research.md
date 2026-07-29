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
