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

`cargo check` / `cargo test` 不带 `--offline` 时可能需要访问 registry。本环境
当前无法解析 crates.io，因此冷依赖缓存属于外部前置条件，不应误判为代码错误。

## 修改规则

- Slint 生成类型集中在 `src/app.rs`；领域模块和传输模块不得依赖 UI。
- 不把密码写入 JSON；凭据提供器应只向 SSH worker 返回临时 secret。
- 不为了方便而接受未知 SSH 主机密钥；测试应注入确定性的 trust policy。
- 进程持有的日志 guard 必须存活到应用退出，以刷新有界非阻塞队列；不得记录凭据
  或终端内容。
- UI 边界上的 payload 必须是有上限的自有数据；不得把 russh channel 或 Tokio
  receiver 暴露给 Slint。
- 修改面向用户的文档时同步维护中英文页面。

## 运行日志

`src/main.rs` 通过 `src/logging.rs` 初始化唯一的全局 tracing subscriber。文件
writer 按 UTC 日期滚动，最多保留 15 个文件，并在进程持有的 guard 释放时刷新。
日志位于平台本地 AxSSH 应用数据目录的 `logs` 子目录。默认过滤规则为
`ax_ssh=info,russh=warn`，可由 `RUST_LOG` 覆盖。

## 验证边界

自动检查覆盖 profile 校验、JSON round-trip、Slint 编译、日志退出刷新，以及
loopback russh 测试服务器上的拒绝式主机密钥探测、受信密码认证、worker 断开与
join。窗口渲染、键盘/焦点、可见的主机密钥/密码弹窗，以及真实 SSH 服务器登录
仍需 GUI/联机手工验收。
