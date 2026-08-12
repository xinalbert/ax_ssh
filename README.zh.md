[English](README.md)

# AxSSH

AxSSH 是一个由 Rust、Slint 和 Tokio 构建的跨平台桌面终端工作区，在同一个原生应用中
提供已保存的 SSH、Telnet、Serial 会话、彼此独立的本地/远程终端 Tab 和持久化设置。

当前功能包括 SSH 密码/私钥/运行时 agent 认证与明确的主机密钥确认、明文 Telnet、自动发现端口但由用户
主动连接的 Serial、有界终端回滚、ANSI 渲染、文本选择、剪贴板快捷键和原生输入法支持。
还支持在独立的双栏 SFTP Tab 中有界浏览远端目录；本地栏只读取有界目录元数据，底部传输队列
支持进度和取消；本地 regular file 可以用平台默认程序打开，远端 regular file 会下载到私有缓存后
打开。SFTP 上传、删除、编辑、自动重连、工作区恢复和完整的终端鼠标上报尚未实现。
已连接的 SSH Terminal 及其 SFTP companion 可以一起移动到独立原生窗口，并在不重连的情况下合并回主窗口。

## 快速开始

安装 Rust `1.92.0` 或更高版本，并使用 Slint winit 后端支持的桌面环境，然后运行：

```bash
cargo run --locked
```

首次连接某台主机时，AxSSH 会先拒绝连接；核对并明确确认服务器的 SHA-256 主机密钥
指纹后才能继续。会话配置、终端操作、设置和数据存储说明见[使用指南](docs/usage.zh.md)。

## 发布

GitHub Releases 会提供 Windows x86_64、Linux x86_64/aarch64，以及 macOS 通用应用包。
在默认分支手动运行 **Create Dated Release** workflow，会按上海时区当天日期发布版本：
例如自动创建 `2026-08-12` tag、同步 Cargo 和 macOS 元数据，并启动 CI；CI 成功后才启动
多平台构建 workflow。
发布 tag 必须严格使用 `YYYY-MM-DD` 格式。每个已发布的 Release 会按高信号提交生成简短的
Highlights 分类和完整变更链接，并保留 GitHub 自动生成的 release notes 作为完整变更列表。

## 文档

- [使用指南](docs/usage.zh.md)
- [架构说明](docs/architecture.zh.md)
- [开发与验证](docs/development.zh.md)
- [文档导航](docs/README.zh.md)
- [项目实施记录](docs/project-implementation-tracker/current.md)

## 安全与仓库边界

未知或发生变化的 SSH 主机密钥默认拒绝。记住的密码由 macOS Keychain、Windows
Credential Manager 或 Unix Secret Service 保存；会话 JSON 永远不会写入密码、
私钥 passphrase、私钥内容、SSH agent socket 路径与 identity、终端输出或运行中的 worker 状态。
Telnet 不加密；Serial 自动发现只读取设备元数据，只有用户明确发起连接后才打开设备。

`third_package/axshell` 只作为参考资料，不是 Cargo workspace member、源码导入、
运行时依赖、构建输入或文档依赖。AxSSH 使用 Slint 构建 UI，使用 russh/russh-sftp、
libmudtelnet-rs 和 tokio-serial 分别提供 SSH/SFTP、Telnet 协议事件和 Serial 传输。

## 许可证

AxSSH 原创软件和原创应用资源采用
[GNU General Public License version 3 only](LICENSE)。仓库内的第三方源码和自带字体
继续适用各自的许可证，详见[第三方声明](THIRD_PARTY_NOTICES.md)。

应用 About 页面同时显示 AxSSH 的许可证标识和 Slint 标准署名组件。
