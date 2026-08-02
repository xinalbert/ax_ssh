[English](usage.md) · [项目首页](../README.zh.md)

# 使用 AxSSH

## 启动应用

AxSSH 需要 Rust `1.92.0` 或更高版本，以及 Slint winit 后端支持的桌面环境。
在仓库根目录运行：

```bash
cargo run --locked
```

## 创建并连接会话

1. 选择 **File > New Server**，在 macOS 按 `Cmd+N`、Windows/Linux 按 `Ctrl+N`，
   或右击侧栏列表空白区域并选择 **New Server**。可在 **Settings > Shortcuts** 修改该快捷键。
2. 选择 **SSH**、**Telnet** 或 **Serial**，再填写对应字段。SSH 使用 host、port、
   username 和密码/私钥认证；**Forward X11 applications** 只适用于 SSH，新 profile
   默认开启；Telnet 使用 host、port，并显示未加密警告；Serial 使用
   端口名、baud rate、data bits、stop bits、parity 和 flow control。AxSSH 启动时会自动
   列出已检测端口；设备插入后可点 **Refresh**，也可以手工输入端口路径或名称。
3. 保存会话并在会话导航中选择它。重复打开同一个已保存会话会创建彼此独立的终端
   Tab，每个 Tab 都有自己的连接与输出。每个 SSH Tab 都可以独立等待主机密钥确认或认证；
   安全提示始终对应活动 Tab，切换 Tab 后其它等待提示仍会保留。
4. 使用 SSH 首次连接时，先通过可信来源核对界面显示的 SHA-256 主机密钥指纹，再确认信任。
   主机密钥发生变化时必须再次明确确认，并应在接受前调查原因。
5. 使用 SSH 时根据提示输入临时密码或私钥 passphrase；私钥 passphrase 永远不会持久化。若要记住
   SSH 密码，先在 **Settings > General** 选择 **System credential store** 或
   **Encrypted application vault**，再在密码弹窗勾选 **Remember password**。选择加密
   保险库时还要输入保险库口令；之后连接只需输入该口令来解锁已保存的 SSH 密码。
   密码、保险库口令和私钥 passphrase 字段不能复制、剪切或选择；应用接收已提交的秘密后
   或用户取消提示时会清空它们。
   关闭正在探测或等待认证的 Tab，只会取消或丢弃该 Tab 自己的连接流程。

在 **Settings > X11** 选择本机 server。macOS 提供 Auto、XQuartz、MacXServer、Custom；
Windows 提供 Auto、VcXsrv、Xming、Custom；Linux 提供 System DISPLAY、Custom。已知 provider
由 macOS 应用数据库或 Windows executable 搜索路径与 Program Files 自动定位，只有 Custom
显示可编辑的 executable 路径。连接时自动启动默认开启。安全默认仍要求 `xauth` 中存在精确的
`MIT-MAGIC-COOKIE-1`，XQuartz 和系统 X.Org/Xwayland 应使用该模式。
**Allow local connections without X authority** 默认关闭，仅在 MacXServer 或由 AxSSH 启动的
VcXsrv/Xming 确实需要时开启；这些兼容启动只连接 loopback，Windows server 也只有在用户明确
选择后才接收 `-ac`。

远端 SSH server 还必须允许 X11 forwarding，通常需要 `X11Forwarding yes` 和可用的服务端
`xauth`；AxSSH 不修改 `sshd_config`。远端 `DISPLAY` 为空表示 forwarding request 没有建立，
常见原因是本机准备失败或 `sshd` 拒绝。无论哪种情况，Terminal shell 都保持连接并提示 X11
不可用；关闭 Tab 会取消全部活动 X11 relay。

Telnet 流量不加密，在终端中输入的登录信息也会明文传输；AxSSH 不会自动填写 Telnet
凭据。Serial 扫描只读取操作系统提供的可用端口元数据，不会打开端口、发送探测字节、
切换 modem line 或猜测通信参数。用户明确连接后，AxSSH 会再次扫描，并用已保存的 USB
vendor/product/serial-number 元数据跟踪被操作系统改名的设备；找不到或出现歧义时会拒绝
打开，要求用户重新选择目标设备。

右击 Group 可新增服务器、复制或 Duplicate Group、重命名或删除 Group；右击 Ungrouped
可新增服务器。右击服务器可复制地址、复制 AxSSH 配置或 Duplicate。SSH 服务器还可选择
**Open SFTP**，直接新建独立的 SFTP Tab，不会创建终端 shell；Telnet 和 Serial 服务器的
该操作保持禁用。服务器菜单仍可连接、编辑或删除，编辑操作复用同一会话编辑器。

选择 **File > Import from Clipboard** 可导入版本化 AxSSH 配置；默认快捷键在 macOS 上为
`Cmd+Shift+I`，在 Windows/Linux 上为 `Ctrl+Shift+I`。先在展开或收起侧栏中选中 Group
或服务器，再选择 **File > Export Selected to Clipboard**；默认快捷键在 macOS 上为
`Cmd+Shift+E`，其它平台为 `Ctrl+Shift+E`。两组快捷键都可在 **Settings > Shortcuts**
修改。没有选中持久化 Group 或服务器时，导出只显示状态提示，不修改剪贴板。

**Copy Server** 与 **Copy Group** 会把版本化 AxSSH JSON 写入剪贴板；单次限制为 256 KiB
和 128 台服务器，并移除 profile identity、已记住密码的引用和已信任主机指纹。导入只新增
配置：重新生成 UUID、处理名称/Group 冲突，并在写盘前再次清除凭据引用和主机信任。密码、
保险库口令、私钥 passphrase 或已信任主机决策都不会被导入。

修改 SSH host 或 port 会清除
已确认的主机密钥指纹，下次连接必须重新明确确认；把 SSH profile 切换为 Telnet 或 Serial
会移除其已记住的 SSH 凭据引用。会话编辑器不会显示或修改已保存密码；要记住新密码，请在
SSH 连接弹窗操作。修改 **Settings > General** 的默认后端只影响之后新记住的密码，不会
迁移或破坏既有 SSH profile 所引用的后端。

选择 **Pane > New Local Shell** 或 Local Shell 控件可打开独立的本地终端。通过 Tab
关闭控件或 **Window > Close Current Tab** 关闭终端。

活动 Tab 是 SSH 或 SFTP 时，选择 **Pane > Open SFTP** 可为该服务器新建独立 SFTP Tab。
默认快捷键为 `Ctrl+M`，可在 **Settings > Shortcuts** 修改。SFTP Tab 会重复正常的主机密钥和认证流程，随后只打开
`sftp` subsystem，绝不申请 PTY 或终端 shell。上方区域将远端和本地文件浏览器并排显示，并使用
一致的名称、大小和修改时间列。远端输入绝对路径、相对当前目录的路径或 `~` 后选择 **Open**；双击
文件夹可进入该目录。本地栏默认打开平台 home 目录，可输入其它本地目录，且只读取有界文件元数据。
**Hidden** 显示点文件，**More** 请求下一页远端目录。底部传输队列在传输尚未实现时只显示状态，不能发起操作。
关闭 SFTP Tab 会关闭它的 subsystem 和 SSH transport。第一阶段仅支持只读浏览，尚不提供上传、下载、删除、
重命名或远端编辑同步。

## 工作区与终端操作

展开的会话导航以可折叠的 Group 行组织服务器；右击列表空白区域可在没有 profile 时创建
空 Group 或 Ungrouped 服务器。展开后的 Group 行显示名称、数量和
居中的绘制下尖角；收起后显示对应的绘制上尖角，避免名称与文字徽标重复。点击 Group
行，或将焦点移到该行后按 Enter/Space，可切换状态。每台可见服务器仍只占一行：左侧为
名称，右侧为遮蔽后的 endpoint。**View > Toggle Session Sidebar** 可在此视图和紧凑
Activity Bar 之间切换；紧凑栏默认使用 Group 名称前 1-4 个字符作为文字徽标，也可在
**Settings > Workspace** 选择 **Full name** 显示完整组名。Full name 模式会将收起栏加宽到
180px，并对长名称换行而不是截断。打开 Group 时会同时展开侧边栏和该 Group，紧凑栏行也提供相同的右键菜单。删除 Group 会把其中
服务器移入 Ungrouped。最近选中的 Group 或服务器会在展开与收起侧栏中持续高亮，hover 和
键盘焦点仍使用独立反馈。删除 profile 也会删除记住的密码，但不会关闭已经打开的终端 Tab。

侧边栏默认遮蔽用户名与 IPv4 地址：用户名尽可能保留前后各两个字符，
`192.168.1.202` 显示为 `192.*.202`。可在 **Settings > Workspace** 中修改单个遮蔽
字符。主机名保持可见，便于快速区分目标服务器。新建会话编辑器与终端共用工作区
Tab 条，Settings 则打开为独立的工作台页面。Tab 条最右侧的 `+` 会列出全部已保存连接，
选择后连接对应 profile；**File > New Server** 和侧栏列表空白区域的右键菜单
仍只打开会话编辑器。
可拖拽工作区 Tab 调整顺序。前置数字会随当前位置变化，而 `#1` 这类实例后缀保持不变。

终端支持有界回滚、ANSI 颜色、文本选择、原生输入法、F1-F12 和常见 xterm 风格
控制/导航序列。全屏程序的 application-cursor 模式会正确影响 Home 与 End。普通
`Ctrl+C` 会作为中断信号发送给活动终端。默认剪贴板快捷键在 macOS 上为
`Cmd+C` / `Cmd+V`，在 Windows 和 Linux 上为 `Ctrl+Shift+C` / `Ctrl+Shift+V`；
这些快捷键可以在 Settings 中修改。
默认 **New Server** 快捷键在 macOS 上为 `Cmd+N`，其它平台为 `Ctrl+N`。
File 菜单导入默认使用 `Cmd/Ctrl+Shift+I`，导出所选 Group 或服务器默认使用
`Cmd/Ctrl+Shift+E`。菜单命令会把当前配置显示为原生 accelerator；录制快捷键或处理安全
提示时会暂时禁用这些 accelerator。

macOS 默认让 Option 输入原生字符、死键和 IME 文本。只有需要将 Option 组合键作为带
Escape 前缀的终端 Meta 输入时，才在 **Settings > Terminal** 开启 **Option acts as Meta**。
Windows/Linux 继续保持 Alt 作为终端 Meta 输入；本地键盘布局的 AltGr 字符则通过文本输入
路径提交。

macOS 的 Settings 与 About 位于标准 AxSSH 应用菜单，Settings 项会跟随其配置快捷键；
Windows 和 Linux 分别在 Edit
和 Help 菜单中提供 Settings 与 About。Settings 包含 General、Appearance、Terminal、X11、
Workspace、Shortcuts 和 About 页面；修改会立即作用于当前应用，关闭 Settings Tab 的 `x` 后才会持久化。
About 标明 AxSSH 使用 `GPL-3.0-only`，并包含 Slint 标准的可点击署名组件。
**Settings > General** 还负责选择以后连接时主动记住密码的默认后端。

在 **Settings > Appearance** 中，Font family 只修改应用界面字体，不改变 Terminal 字符格度量；
Display mode 单独选择 **Follow system**、**Light** 或 **Dark**；Color palette 单独选择
**AxSSH**、**Solarized**、**Arctic**、**Tokyo**、**Ember**、**Forest** 或 **Custom**，因此
所有固定配色都能同时用于浅色和深色。Arctic 偏冷色技术感，Tokyo 偏夜间，Ember 偏暖色，
Forest 使用绿色高对比。在 Dark 模式中，每个固定配色也会选择相应的 Terminal ANSI 色表；
Light 模式保留可读的浅色 ANSI 色表。Custom 会展开 Light/Dark 两套语义色。即时预览和持久化时，
无效十六进制值或会让文字、必要边框、焦点/状态及终端文字看不清的颜色，会回退到对应明暗侧的
可读默认。

**Settings > Terminal** 独立控制 Terminal 字体、字号、行高、亮度、粗体亮 ANSI 色、
scrollback、鼠标行为以及平台相关的 Option-as-Meta。两个字体列表都先显示软件自带字体，
再显示自动发现的系统等宽字体。

**Settings > X11** 控制当前平台的本机 X server provider、连接时启动和显式的 loopback-only
no-auth 兼容模式。已知 provider 会自动定位；应用路径只在 Custom 时显示，且必须指向可执行文件。
这些设置都不是秘密；X11 cookie 仍只短暂存在且永不保存。

## 本地数据与凭据

AxSSH 把 profile、非敏感 Group 名称和设置写入平台本地应用数据目录中的版本化
`sessions.json`。每个 profile 明确包含一份 SSH、Telnet 或 Serial 配置；只有 SSH 可以
包含已确认的主机密钥指纹、私钥路径、指向已记住密码后端的非敏感引用，以及非敏感的 X11
forwarding 开关；X11 cookie 永远不会保存。Serial 可以保存
用于稳定匹配的非敏感 USB 身份元数据。profile 不包含密码、保险库口令、私钥 passphrase、
私钥内容、终端输出或运行中的进程状态。

选择 **System credential store** 后，记住的密码通过 macOS Keychain、Windows Credential
Manager 或 Unix Secret Service 保存。选择 **Encrypted application vault** 后，应用会在
配置目录中为每个 profile 保存一个私有加密记录，保险库口令不会保存。每日日志写入同一
应用数据目录的 `logs` 子目录，最多保留 15 个文件。`RUST_LOG` 可以覆盖默认的
`ax_ssh=info,russh=warn` 过滤规则。凭据和终端内容不得写入日志。

已连接的终端可以合法地长时间没有 shell 输出。AxSSH 使用 SSH 传输层的 keepalive 和
inactivity 策略，而不是 shell 输出计时器，来判断连接何时不再存活。

## 当前限制

共享的 OpenSSH 兼容 known-hosts 存储、主机密钥撤销、SFTP 文件传输/修改/编辑同步、
SSH agent、自动重连、持久化工作区恢复和完整的全屏终端鼠标上报仍属于后续工作。
Serial 的端口可见性、权限和参数支持取决于目标操作系统与硬件；Telnet 不提供加密或自动登录。
