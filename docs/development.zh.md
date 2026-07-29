[English](development.md)

# 开发说明

## 环境要求

- Rust `1.92.0` 或更高版本（本机验证环境为 `1.96.1`）
- Cargo
- Slint winit 后端支持的桌面环境

根 Cargo 隐式 workspace 只包含 `ax_ssh` package。`third_package/axshell` 是
参考子模块，不是 workspace member 或构建依赖。

## 常用命令

```bash
cargo run
cargo fmt --all -- --check
cargo check --locked --offline
cargo clippy --all-targets --locked --offline -- -D warnings
cargo test --locked --offline
git diff --check
```

`cargo check` / `cargo test` 不带 `--offline` 时可能需要访问 registry。2026-07-29
解析 `keyring 4.1.5` 时本机可以访问 crates.io；离线命令仍要求 Cargo 缓存已准备好。

## 修改规则

- Slint 生成类型集中在 `src/app.rs`；领域模块和传输模块不得依赖 UI。
- 不把密码写入 JSON；`src/credentials.rs` 只能按 profile 通过平台系统凭据库读写一份
  密码，并且只向 SSH worker 返回临时 secret。
- 私钥 profile 只能持久化文件路径；私钥内容和 passphrase 必须在 UI 线程外加载，
  且不得记录或持久化。
- 不为了方便而接受未知 SSH 主机密钥；测试应注入确定性的 trust policy。
- 进程持有的日志 guard 必须存活到应用退出，以刷新有界非阻塞队列；不得记录凭据
  或终端内容。
- UI 边界上的 payload 必须是有上限的自有数据；不得把 russh channel 或 Tokio
  receiver 暴露给 Slint。
- 运行实例必须使用终端 Tab UUID，而不是已保存的 profile UUID；输入、resize、输出、
  重试、关闭和迟到事件都按 `tab_id + attempt_id` 路由。
- 终端输入、输出批次、事件队列和 scrollback 都必须有上限。
- `src/terminal/input.rs` 不得依赖 Slint 键值；在 `src/app.rs` 完成映射，并在不构造
  窗口的条件下测试普通/application-cursor 终端字节序列；平台可打印键后备转换归
  Slint bridge 所有。
- 所有平台都要把 Ctrl 组合留给获得焦点的 PTY，包括 `Ctrl+C` 和 tmux 前缀；终端
  剪贴板默认键在 macOS 使用 `Cmd`，其他平台使用 `Ctrl+Shift`。全局 UI 命令在
  macOS 使用 `Cmd`，其他平台使用 `Ctrl`，不得遮蔽终端控制字节。Slint 1.17 会在
  Apple 平台交换 Command/Control 修饰键字段，快捷键匹配或终端编码前必须在
  `src/app.rs` 还原。
- 可见终端保持为渲染网格。隐藏的 Slint `TextInput` 只充当 IME 代理：跟随终端光标
  定位，把未修饰的预编辑按键留给输入法，并确保提交文本只发送一次。
- 本地 PTY 的 child、reader、writer、取消和 join 所有权保留在 `src/local_shell.rs`，
  不得把阻塞式 PTY 操作移到 UI 线程。
- 自带字体必须放在 `assets/fonts/`，并保留独立许可证和声明；构建或运行时不得从
  `third_package/axshell` 加载静态资源。
- 修改面向用户的文档时同步维护中英文页面。

## 运行日志

`src/main.rs` 通过 `src/logging.rs` 初始化唯一的全局 tracing subscriber。文件
writer 按 UTC 日期滚动，最多保留 15 个文件，并在进程持有的 guard 释放时刷新。
日志位于平台本地 AxSSH 应用数据目录的 `logs` 子目录。默认过滤规则为
`ax_ssh=info,russh=warn`，可由 `RUST_LOG` 覆盖。

## 验证边界

自动检查覆盖 profile 校验、JSON round-trip、Slint 编译、日志退出刷新，以及
loopback russh 测试服务器上的拒绝式主机密钥探测、受信密码/私钥认证、PTY shell
输入输出、resize、worker 断开与 join；单元测试还覆盖 ANSI 解析、有界 scrollback、
终端控制/导航键编码、旧版外观到版本化设置的迁移、同 profile 多 Tab 隔离、本机密钥
发现、加密密钥 passphrase、本地 PTY 生命周期、vt100 字符格渲染、application-cursor
方向键、Shift 可打印键后备转换、原始 C0 控制字节事件和 Apple 修饰键还原。忽略测试
`platform_credential_store_round_trips_and_deletes` 会执行真实平台凭据
写入、读取和删除，并可能触发系统授权提示；该测试已在 macOS Keychain 通过，Unix
Secret Service 和 Windows Credential Manager 仍需对应平台验证。窗口渲染、键盘/
焦点、可见的分组/主机密钥/认证弹窗、全屏终端程序，以及真实 SSH 服务器登录仍需
GUI/联机手工验收；其中还包括横向 Tab 滚动、多个真实 SSH 并发连接和切换后的终端
焦点保持。
