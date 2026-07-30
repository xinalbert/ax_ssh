[English](README.md)

# AxSSH

AxSSH 是一个由 Rust、Slint、Tokio 和 russh 构建的跨平台桌面 SSH 工作区，
在同一个原生应用中提供已保存会话、彼此独立的本地/远程终端 Tab 和持久化工作区设置。

当前功能包括密码与私钥认证、明确的 SHA-256 主机密钥确认、可选的系统凭据库存储、
有界终端回滚、ANSI 渲染、文本选择、剪贴板快捷键和原生输入法支持。SFTP、SSH agent、
自动重连、工作区恢复和完整的终端鼠标上报尚未实现。

## 快速开始

安装 Rust `1.92.0` 或更高版本，并使用 Slint winit 后端支持的桌面环境，然后运行：

```bash
cargo run --locked
```

首次连接某台主机时，AxSSH 会先拒绝连接；核对并明确确认服务器的 SHA-256 主机密钥
指纹后才能继续。会话配置、终端操作、设置和数据存储说明见[使用指南](docs/usage.zh.md)。

## 文档

- [使用指南](docs/usage.zh.md)
- [架构说明](docs/architecture.zh.md)
- [开发与验证](docs/development.zh.md)
- [文档导航](docs/README.zh.md)
- [项目实施记录](docs/project-implementation-tracker/current.md)

## 安全与仓库边界

未知或发生变化的主机密钥默认拒绝。记住的密码由 macOS Keychain、Windows
Credential Manager 或 Unix Secret Service 保存；会话 JSON 永远不会写入密码、
私钥 passphrase、私钥内容、终端输出或运行中的 worker 状态。

`third_package/axshell` 只作为参考资料，不是 Cargo workspace member、源码导入、
运行时依赖、构建输入或文档依赖。AxSSH 使用 Slint 构建 UI，使用 russh 提供 SSH
传输。
