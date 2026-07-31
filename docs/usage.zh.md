[English](usage.md) · [项目首页](../README.zh.md)

# 使用 AxSSH

## 启动应用

AxSSH 需要 Rust `1.92.0` 或更高版本，以及 Slint winit 后端支持的桌面环境。
在仓库根目录运行：

```bash
cargo run --locked
```

## 创建并连接会话

1. 选择 **File > New Session**，或右击侧栏列表空白区域并选择 **New Server**。
2. 填写名称、可选分组、主机、端口和用户名，然后选择密码或私钥认证。可以选择从
   用户 `.ssh` 目录发现的私钥，也可以手工输入路径。
3. 保存会话并在会话导航中选择它。重复打开同一个已保存会话会创建彼此独立的终端
   Tab，每个 Tab 都有自己的连接与输出。每个 SSH Tab 都可以独立等待主机密钥确认或认证；
   安全提示始终对应活动 Tab，切换 Tab 后其它等待提示仍会保留。
4. 首次连接时，先通过可信来源核对界面显示的 SHA-256 主机密钥指纹，再确认信任。
   主机密钥发生变化时必须再次明确确认，并应在接受前调查原因。
5. 根据提示输入临时密码或私钥 passphrase；私钥 passphrase 永远不会持久化。若要记住
   SSH 密码，先在 **Settings > General** 选择 **System credential store** 或
   **Encrypted application vault**，再在密码弹窗勾选 **Remember password**。选择加密
   保险库时还要输入保险库口令；之后连接只需输入该口令来解锁已保存的 SSH 密码。
   密码、保险库口令和私钥 passphrase 字段不能复制、剪切或选择；应用接收已提交的秘密后
   或用户取消提示时会清空它们。
   关闭正在探测或等待认证的 Tab，只会取消或丢弃该 Tab 自己的连接流程。

右击 Group 可新增服务器、重命名或删除 Group；右击 Ungrouped 只提供新增服务器。右击
服务器可连接、编辑或删除，编辑操作复用同一会话编辑器。修改 host 或 port 会清除已确认
的主机密钥指纹，下次连接必须重新明确确认。会话编辑器不会显示或修改已保存密码；要记住
新密码，请在连接弹窗操作。修改 **Settings > General** 的默认后端只影响之后新记住的
密码，不会迁移或破坏既有 profile 所引用的后端。

选择 **Pane > New Local Shell** 或 Local Shell 控件可打开独立的本地终端。通过 Tab
关闭控件或 **Window > Close Current Tab** 关闭终端。

## 工作区与终端操作

展开的会话导航以可折叠的 Group 行组织服务器；右击列表空白区域可在没有 profile 时创建
空 Group 或 Ungrouped 服务器。展开后的 Group 行显示名称、数量和
居中的绘制下尖角；收起后显示对应的绘制上尖角，避免名称与文字徽标重复。点击 Group
行，或将焦点移到该行后按 Enter/Space，可切换状态。每台可见服务器仍只占一行：左侧为
名称，右侧为遮蔽后的 endpoint。**View > Toggle Session Sidebar** 可在此视图和紧凑
Activity Bar 之间切换；只有紧凑栏使用 Group 名称前两个字符作为文字徽标，打开 Group
时会同时展开侧边栏和该 Group，紧凑栏行也提供相同的右键菜单。删除 Group 会把其中
服务器移入 Ungrouped；删除 profile 也会删除记住的密码，但不会关闭已经打开的终端 Tab。

侧边栏默认遮蔽用户名与 IPv4 地址：用户名尽可能保留前后各两个字符，
`192.168.1.202` 显示为 `192.*.202`。可在 **Settings > Workspace** 中修改单个遮蔽
字符。主机名保持可见，便于快速区分目标服务器。新建会话编辑器与终端共用工作区
Tab 条，Settings 则打开为独立的工作台页面。Tab 条最右侧的 `+` 会列出全部已保存 SSH
会话，选择后连接对应 profile；**File > New Session** 和侧栏列表空白区域的右键菜单
仍只打开会话编辑器。
可拖拽工作区 Tab 调整顺序。前置数字会随当前位置变化，而 `#1` 这类实例后缀保持不变。

终端支持有界回滚、ANSI 颜色、文本选择、原生输入法、F1-F12 和常见 xterm 风格
控制/导航序列。全屏程序的 application-cursor 模式会正确影响 Home 与 End。普通
`Ctrl+C` 会作为中断信号发送给活动终端。默认剪贴板快捷键在 macOS 上为
`Cmd+C` / `Cmd+V`，在 Windows 和 Linux 上为 `Ctrl+Shift+C` / `Ctrl+Shift+V`；
这些快捷键可以在 Settings 中修改。

macOS 默认让 Option 输入原生字符、死键和 IME 文本。只有需要将 Option 组合键作为带
Escape 前缀的终端 Meta 输入时，才在 **Settings > Terminal** 开启 **Option acts as Meta**。
Windows/Linux 继续保持 Alt 作为终端 Meta 输入；本地键盘布局的 AltGr 字符则通过文本输入
路径提交。

macOS 的 Settings 与 About 位于标准 AxSSH 应用菜单；Windows 和 Linux 分别在 Edit
和 Help 菜单中提供 Settings 与 About。Settings 包含 General、Appearance、Terminal、
Workspace、Shortcuts 和 About 页面；只有选择 **Save** 后修改才会持久化。
**Settings > General** 还负责选择以后连接时主动记住密码的默认后端。

在 **Settings > Appearance** 中，Display mode 单独选择 **Follow system**、**Light** 或
**Dark**；Color palette 单独选择 **AxSSH**、**Solarized** 或 **Custom**，因此固定配色都能
同时用于浅色和深色。Custom 会展开 Light/Dark 两套语义色。保存时，无效十六进制值或会让
文字、必要边框、焦点/状态及终端文字看不清的颜色，会回退到对应明暗侧的可读默认。

## 本地数据与凭据

AxSSH 把 profile、非敏感 Group 名称和设置写入平台本地应用数据目录中的版本化
`sessions.json`。profile 可以包含已确认的主机密钥指纹、私钥路径以及指向已记住密码
后端的可选非敏感引用，但不包含密码、保险库口令、私钥 passphrase、私钥内容、终端输出
或运行中的进程状态。

选择 **System credential store** 后，记住的密码通过 macOS Keychain、Windows Credential
Manager 或 Unix Secret Service 保存。选择 **Encrypted application vault** 后，应用会在
配置目录中为每个 profile 保存一个私有加密记录，保险库口令不会保存。每日日志写入同一
应用数据目录的 `logs` 子目录，最多保留 15 个文件。`RUST_LOG` 可以覆盖默认的
`ax_ssh=info,russh=warn` 过滤规则。凭据和终端内容不得写入日志。

已连接的终端可以合法地长时间没有 shell 输出。AxSSH 使用 SSH 传输层的 keepalive 和
inactivity 策略，而不是 shell 输出计时器，来判断连接何时不再存活。

## 当前限制

共享的 OpenSSH 兼容 known-hosts 存储、主机密钥撤销、SFTP、SSH agent、自动重连、
持久化工作区恢复和完整的全屏终端鼠标上报仍属于后续工作。
