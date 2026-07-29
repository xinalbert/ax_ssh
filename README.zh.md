[English](README.md)

# AxSSH

AxSSH 是一个基于 Rust、Slint、Tokio 和 russh 的跨平台 SSH 工作区。本仓库当前
的会话流程会校验服务器 SHA-256 主机密钥指纹、接收临时密码，并由可取消的 worker
持续持有认证后的连接。终端模拟和 SFTP 仍留在后续阶段实现。

## 快速开始

```bash
cargo run
```

AxSSH 按日写入平台本地 AxSSH 应用数据目录下的 `logs` 子目录，最多保留 15 个日志
文件。可通过 `RUST_LOG` 覆盖默认的 `ax_ssh=info,russh=warn` 过滤规则。

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
