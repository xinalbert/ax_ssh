[English](usage.md) · [项目首页](../README.zh.md)

# 使用 AxSSH

## 启动应用

AxSSH 需要 Rust `1.92.0` 或更高版本，以及 Slint winit 后端支持的桌面环境。
在仓库根目录运行：

```bash
cargo run --locked
```

## 创建并连接会话

1. 选择 **File > New Session**，或使用侧栏的新建会话控件。
2. 填写名称、可选分组、主机、端口和用户名，然后选择密码或私钥认证。可以选择从
   用户 `.ssh` 目录发现的私钥，也可以手工输入路径。
3. 保存会话并在会话导航中选择它。重复打开同一个已保存会话会创建彼此独立的终端
   Tab，每个 Tab 都有自己的连接与输出。
4. 首次连接时，先通过可信来源核对界面显示的 SHA-256 主机密钥指纹，再确认信任。
   主机密钥发生变化时必须再次明确确认，并应在接受前调查原因。
5. 根据提示输入临时密码或私钥 passphrase。密码可以选择保存到系统凭据库；私钥
   passphrase 永远不会持久化。

选择 **Pane > New Local Shell** 或 Local Shell 控件可打开独立的本地终端。通过 Tab
关闭控件或 **Window > Close Current Tab** 关闭终端。

## 工作区与终端操作

展开的会话导航以可折叠的 Group 行组织服务器。展开后的 Group 行只显示名称、数量和
居中的绘制下尖角；收起后显示对应的绘制上尖角，避免名称与文字徽标重复。点击 Group
行，或将焦点移到该行后按 Enter/Space，可切换状态。每台可见服务器仍只占一行：左侧为
名称，右侧为遮蔽后的 endpoint。**View > Toggle Session Sidebar** 可在此视图和紧凑
Activity Bar 之间切换；只有紧凑栏使用 Group 名称前两个字符作为文字徽标，打开 Group
时会同时展开侧边栏和该 Group。

侧边栏默认遮蔽用户名与 IPv4 地址：用户名尽可能保留前后各两个字符，
`192.168.1.202` 显示为 `192.*.202`。可在 **Settings > Workspace** 中修改单个遮蔽
字符。主机名保持可见，便于快速区分目标服务器。新建会话编辑器与终端共用工作区
Tab 条，Settings 则打开为独立的工作台页面。Tab 条最右侧的 `+` 会列出全部已保存 SSH
会话，选择后连接对应 profile；侧栏 `+` 和 **File > New Session** 仍只打开会话编辑器。
可拖拽工作区 Tab 调整顺序。前置数字会随当前位置变化，而 `#1` 这类实例后缀保持不变。

终端支持有界回滚、ANSI 颜色、文本选择、原生输入法和常见 xterm 风格控制/导航序列。
普通 `Ctrl+C` 会作为中断信号发送给活动终端。默认剪贴板快捷键在 macOS 上为
`Cmd+C` / `Cmd+V`，在 Windows 和 Linux 上为 `Ctrl+Shift+C` / `Ctrl+Shift+V`；
这些快捷键可以在 Settings 中修改。

macOS 的 Settings 与 About 位于标准 AxSSH 应用菜单；Windows 和 Linux 分别在 Edit
和 Help 菜单中提供 Settings 与 About。Settings 包含 General、Appearance、Terminal、
Workspace、Shortcuts 和 About 页面；只有选择 **Save** 后修改才会持久化。

## 本地数据与凭据

AxSSH 把 profile 和非敏感设置写入平台本地应用数据目录中的版本化
`sessions.json`。profile 可以包含已确认的主机密钥指纹、私钥路径和密码可用性标记，
但不包含密码、私钥 passphrase、私钥内容、终端输出或运行中的进程状态。

记住的密码通过 macOS Keychain、Windows Credential Manager 或 Unix Secret Service
保存。每日日志写入同一应用数据目录的 `logs` 子目录，最多保留 15 个文件。
`RUST_LOG` 可以覆盖默认的 `ax_ssh=info,russh=warn` 过滤规则。凭据和终端内容不得
写入日志。

## 当前限制

共享的 OpenSSH 兼容 known-hosts 存储、主机密钥撤销、SFTP、SSH agent、自动重连、
持久化工作区恢复和完整的全屏终端鼠标上报仍属于后续工作。
