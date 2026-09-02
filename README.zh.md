[English](README.md)

# AxSSH

AxSSH 是一个由 Rust、Slint 和 Tokio 构建的跨平台桌面终端工作区，在同一个原生应用中
提供已保存的 SSH、Telnet、Serial 和本地 shell 会话，以及彼此独立的 Terminal/SFTP Tab
和持久化工作区设置。

## 功能概览

- 使用密码、私钥或运行时 SSH agent 连接，并在首次信任主机前明确确认 SHA-256 主机密钥指纹。
- 在本地或远端终端中使用有界回滚、ANSI 渲染、文本选择、剪贴板和原生输入法支持；Windows 物理数字
  小键盘也能传递给终端程序，包括 DEC application-keypad 模式。
- 在独立的 SFTP Tab 中浏览本地和远端文件，提供有界目录读取、递归下载、上传、重命名、删除和远端文本编辑。
- 支持分屏和独立的原生工作区窗口；Settings 同时展示可配置快捷键和固定的平台快捷键。普通输入框支持原生编辑
  与粘贴，秘密字段仍不可复制。

## 快速开始

安装 Rust `1.92.0` 或更高版本，并使用 Slint winit 后端支持的桌面环境，然后运行：

```bash
cargo run --locked
```

未知或发生变化的 SSH 主机密钥会被拒绝；核对并明确确认服务器的 SHA-256 指纹后才能继续。
会话创建、终端操作、SFTP 和设置说明见[使用指南](docs/usage.zh.md)。

## 发布

GitHub Releases 会提供 Windows x86_64、Linux x86_64/aarch64，以及 macOS Apple Silicon、
Intel 和通用应用包。在默认分支同步并提交发行元数据后，推送有效的 annotated
`YYYY-MM-DD[-N]` tag 会直接启动发布 workflow。所需命令见[发布说明](docs/development.zh.md#github-发布)。

## 文档

- [使用指南](docs/usage.zh.md)
- [开发与验证](docs/development.zh.md)
- [架构说明](docs/architecture.zh.md)
- [文档导航](docs/README.zh.md)

## 支持与安全

可通过 [issue tracker](https://github.com/xinalbert/ax_ssh/issues/new) 或应用 About 页的
**Report a bug** 反馈问题。只有用户明确选择保存时才会写入密码，并且仅使用系统凭据库或加密
应用保险库；会话 JSON 不会写入明文密码、私钥 passphrase、终端输出或运行中的 worker 状态。
Telnet 不加密。

## 许可证

AxSSH 原创软件和原创应用资源采用
[GNU General Public License version 3 only](LICENSE)。仓库内的第三方源码和自带字体
继续适用各自的许可证，详见[第三方声明](THIRD_PARTY_NOTICES.md)。

应用 About 页面同时显示 AxSSH 的许可证标识和 Slint 标准署名组件。
