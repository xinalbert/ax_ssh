[English](README.md)

# AxSSH

AxSSH 是一个基于 Rust、Slint、Tokio 和 russh 的跨平台 SSH 工作区。本仓库当前
支持把已保存会话放入可折叠分组。连接流程会校验服务器 SHA-256 主机密钥指纹、
接收临时密码，并可选择将密码保存到系统凭据库，供下次自动登录；认证后的连接由
可取消的 worker 持续持有。会话也可以选择用户 `.ssh` 目录中发现的私钥或手工输入
路径；加密私钥会请求一次性 passphrase。worker 认证后打开 PTY shell，显示有界的
ANSI 终端输出，并由终端区域直接接收键盘输入。Enter、Backspace、Tab、Escape、
方向键、Home/End、Insert/Delete、Page 键、Ctrl 控制字节和带修饰键的 xterm 导航
序列都会发送到远端 PTY。终端、Settings 和新建会话编辑器共用同一个顶部 Tab 条；
每个终端 Tab 都有唯一运行时 ID，并独占 worker 和有界终端模型，因此同一个已保存
服务器可以打开多次，输出和连接状态不会串线。终端与工作区参数在 Settings Tab 中
管理，并写入版本化 `sessions.json`；项目按 SIL Open Font License 自带 JetBrains
Mono。SFTP 和完整鼠标终端协议仍留在后续阶段实现。

## 快速开始

```bash
cargo run
```

AxSSH 按日写入平台本地 AxSSH 应用数据目录下的 `logs` 子目录，最多保留 15 个日志
文件。可通过 `RUST_LOG` 覆盖默认的 `ax_ssh=info,russh=warn` 过滤规则。

记住的密码通过平台后端进入 macOS Keychain、Windows Credential Manager 或 Unix
Secret Service。会话 JSON 保存 profile、非敏感设置和凭据可用性标记，绝不保存
密码、passphrase、私钥内容、终端输出或 worker 状态。

私钥 profile 只保存所选文件路径；私钥内容和 passphrase 不会持久化或写入日志。

终端支持鼠标选择文本；`Ctrl+Shift+C` 复制选区，macOS 使用 `Cmd+V`、其他桌面
平台使用 `Ctrl+Shift+V` 把剪贴板文本粘贴到远端 shell。普通 `Ctrl+C` 仍发送终端
中断字节。

在 Cargo 缓存已准备好的环境中，可使用离线检查：

```bash
cargo check --offline
cargo test --offline
```

## 文档

- [文档导航](docs/README.zh.md)
- [架构说明](docs/architecture.zh.md)
- [开发说明](docs/development.zh.md)
- [项目实施记录](docs/project-implementation-tracker/current.md)

## 仓库边界

`third_package/axshell` 是仅供参考的子模块，不属于 Cargo workspace，不会被
`src/` 导入，也不能成为运行时或构建依赖。AxSSH 的 UI 使用 Slint，SSH 传输
使用 russh。
