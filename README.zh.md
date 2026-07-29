[English](README.md)

# AxSSH

AxSSH 是一个基于 Rust、Slint、Tokio 和 russh 的跨平台 SSH 工作区。已保存会话可放入
可折叠分组；Activity Bar 的分组图标用于展开会话侧栏，再次点击当前分组会收起。
Local Shell 图标只新建本地终端 Tab，不改变侧栏状态。未分组会话以紧凑的顶层项直接
显示在侧栏中，鼠标短暂停留后显示连接地址。每个本地或 SSH 终端 Tab 都有唯一运行时
ID，并独占 worker 和有界终端模型，因此重复打开同一个服务器或本地 shell 时不会
共享输出和进程状态。

SSH 连接流程会校验服务器 SHA-256 主机密钥指纹、接收临时密码，并可选择将密码保存
到系统凭据库。会话也可以选择用户 `.ssh` 目录中发现的私钥或手工输入路径；加密私钥
会请求一次性 passphrase。worker 认证后打开 PTY shell，并与本地 Tab 使用同一个
终端表面。

终端按与字体关联的字符网格渲染，提供有界 scrollback、ANSI 颜色、网格坐标选区和
整格光标。Enter、Backspace、Tab、Escape、方向键、Home/End、Insert/Delete、Page
键、Ctrl 控制字节和带修饰键的 xterm 导航序列都会发送到活动 PTY。未修饰方向键会
跟随终端普通或 application-cursor 模式发送 CSI 或 SS3 序列，因此 shell 历史和全屏
程序可以收到正确输入。终端、Settings 和新建会话编辑器共用同一个顶部 Tab 条。
Tab 溢出后可用触控板或鼠标滚轮横向滚动，但不响应鼠标拖拽滚动。macOS 只有零 Tab
时的空白条和最右侧专用留白可以移动窗口；Tab、Activity Bar、会话侧栏和终端内容
都只响应自身交互，不会拖动窗口。

终端、快捷键、本地 shell 和工作区参数在 Settings Tab 中管理，并写入版本化
`sessions.json`；已发现的 shell 名称会缓存，下次只合并新增项。项目按 SIL Open Font
License 自带 JetBrains Mono。SFTP 和完整鼠标终端协议仍留在后续阶段实现。

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

终端支持鼠标选择文本；macOS 保留 `Cmd+C`/`Cmd+V` 复制粘贴，其他平台使用
`Ctrl+Shift+C`/`Ctrl+Shift+V`。普通 `Ctrl+C` 保留为终端中断字节，其他 Ctrl 组合
继续交给 shell、tmux 等终端程序。
工作区命令使用平台主修饰键，例如 macOS 用 `Cmd+S`、其他平台用 `Ctrl+S` 收放侧栏；
AxSSH 会在 Slint 的 Apple 平台修饰键映射后还原物理 Control/Command 语义，因此 macOS
的 `Ctrl+B` 会进入 tmux，而 `Cmd+B` 仍是 UI 快捷键候选。终端获得焦点时优先接收
Ctrl 输入，不会被冲突的工作区快捷键抢走。可配置的右键快捷行为会在有选区时复制、
没有选区时粘贴。

可见终端是渲染网格，不是文本编辑器。一个完全透明并跟随终端光标的输入法代理负责
接入系统中文 IME 的预编辑和候选界面，只有已提交文本会进入有界 PTY 输入路径。

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
