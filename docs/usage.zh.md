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
   username 和密码、私钥或 **SSH agent** 认证；**Forward X11 applications** 只适用于 SSH，新 profile
   默认开启；Telnet 使用 host、port，并显示未加密警告；Serial 使用
   端口名、baud rate、data bits、stop bits、parity 和 flow control。会话编辑器进入 Serial
   模式时才列出已检测端口；设备插入后可点 **Refresh**，也可以手工输入端口路径或名称。SSH-only 的
   **SFTP directories** 区域可以设置新 SFTP Tab 首次打开的远端和本地目录；默认分别是 `~` 与平台 home 目录。
3. 保存会话并在会话导航中选择它。新建 SSH profile 时，也可以点击 **Save & connect**，
   保存成功后立即进入正常的主机密钥流程。编辑器密码是可选项：留空可只保存 profile，不保存密码；
   非空值可只供本次连接使用。勾选 **Save password (optional)** 才保存密码。未提供保险库口令时，
   即使选择了加密保险库，也会保存到系统凭据库；提供保险库口令才创建加密保险库记录。重复打开同一个已保存会话会创建彼此独立的终端
   Tab，每个 Tab 都有自己的连接与输出。每个 SSH Tab 都可以独立等待主机密钥确认或认证；
   安全提示始终对应活动 Tab，切换 Tab 后其它等待提示仍会保留。
4. 使用 SSH 首次连接时，先通过可信来源核对界面显示的 SHA-256 主机密钥指纹，再确认信任。
   主机密钥发生变化时必须再次明确确认，并应在接受前调查原因。
5. 使用 SSH 密码或私钥认证时，根据提示输入临时密码或私钥 passphrase；私钥 passphrase 永远不会持久化。若要记住
   SSH 密码，先在 **Settings > General** 选择 **System credential store** 或
   **Encrypted application vault**。密码弹窗会以该设置为初始值，也可以在 **Credential
   storage** 菜单中只为本次提示改选后端，再勾选 **Save password (optional)**。只有认证成功后才会
   保存密码；请求加密保险库但保险库口令留空时会改存系统凭据库，提供非空保险库口令才会创建
   加密保险库记录。之后使用既有保险库记录仍需输入其保险库口令解锁已保存的 SSH 密码。
   密码、保险库口令和私钥 passphrase 字段不能复制、剪切或选择；应用接收已提交的秘密后
   或用户取消提示时会清空它们。
   关闭正在探测或等待认证的 Tab，只会取消或丢弃该 Tab 自己的连接流程。
6. 使用 **SSH agent** 时，AxSSH 在连接开始时访问当时可用的 agent，不显示密码或 passphrase
   弹窗。Unix/macOS 读取当前 `SSH_AUTH_SOCK`；Windows 优先使用该变量，未设置时使用 OpenSSH
   agent 默认 named pipe。profile 只保存认证方式，不保存 socket 路径、identity 注释、公钥、私钥或
   passphrase。AxSSH 在 30 秒 agent 认证总上限内最多尝试 5 个 identity，认证完成或取消后立即释放
   agent 连接；锁定的 agent 仍可能显示自身的系统确认。agent 不可用、没有 identity、超时或所有
   identity 被服务端拒绝时，解锁或更新当前 agent 后重新连接该 Tab。首次或主机密钥变化后的连接仍
   必须先完成明确的主机密钥确认。

在 **Settings > X11** 选择本机 server。macOS 提供 Auto、XQuartz、MacXServer、Custom；
Windows 提供 Auto、VcXsrv、Xming、Custom；Linux 提供 System DISPLAY、Custom。已知 provider
由 macOS 应用数据库或 Windows executable 搜索路径与 Program Files 自动定位，只有 Custom
会显示检测到的安装位置；选择 Custom 后可自行提供 executable 路径。首个 X11 application 时启动
默认开启：打开 SSH shell 只向服务端申请 forwarding，不会启动本机 X server。安全默认仍要求
`xauth` 中存在精确的 `MIT-MAGIC-COOKIE-1`，XQuartz 和系统 X.Org/Xwayland 应使用该模式。
**Allow local connections without X authority** 默认关闭，仅在 MacXServer 或由 AxSSH 启动的
VcXsrv/Xming 确实需要时开启；这些兼容启动只连接 loopback，Windows server 也只有在用户明确
选择后才接收 `-ac`。

远端 SSH server 还必须允许 X11 forwarding，通常需要 `X11Forwarding yes` 和可用的服务端
`xauth`；AxSSH 不修改 `sshd_config`。远端 `DISPLAY` 为空表示 forwarding request 没有建立，
常见原因是 `sshd` 拒绝。如果远端图形应用打开时本机准备失败，AxSSH 只拒绝该图形 channel；
Terminal shell 仍保持连接并提示 X11 不可用；关闭 Tab 会取消全部活动 X11 relay。

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
会移除其已记住的 SSH 凭据引用。会话编辑器不会显示已保存密码，密码留空会保留原凭据；
新输入的密码可以由 **Save & connect** 使用一次，也可以勾选 **Save password (optional)** 并选择后端后保存；
保险库口令留空时会改存系统凭据库。
修改 **Settings > General** 的默认后端只用于初始化之后的保存选择，不会迁移或破坏既有 SSH profile
所引用的后端。

选择 **Pane > New Local Shell** 或 Local Shell 控件可打开独立的本地终端。通过 Tab
关闭控件或 **Window > Close Current Tab** 关闭终端。

活动 Tab 是 SSH 或 SFTP 时，选择 **Pane > Switch SSH/SFTP Tab**。默认快捷键为 `Ctrl+M`，
可在 **Settings > Shortcuts** 修改。从尚未配对的 SSH Terminal 触发时，会紧接其后新建独立
SFTP Tab；从独立 SFTP Tab 触发时，会紧接其前新建 SSH Terminal。新 Tab 使用自己的 SSH
transport，并重新走正常的主机密钥和认证流程。配对建立后，在任一端再次触发只会激活另一端。
关闭任一 Tab 会保留另一端并清除配对，之后再次触发可重新创建对应 Tab。

SFTP Tab 随后只打开 `sftp` subsystem，绝不申请 PTY 或终端 shell。上方区域将远端和本地
文件浏览器并排显示，并使用一致的名称、大小和修改时间列。拖动纵向分隔条可调整两栏宽度，双击可恢复
等宽；拖动横向分隔条可调整文件区与 Transfers 高度，双击可折叠或展开 Transfers。两个分隔条都可通过
Tab 获得焦点，并接受对应方向键、Home 和 End；Enter 或 Space 执行其双击动作。当前应用运行期间切换
Tab 不会丢失布局，应用重启后恢复默认。本阶段不支持单独拖动文件表格列宽。

两个目录标题栏中的复制图标可将对应远端或本地栏当前路径写入系统剪贴板。

远端输入绝对路径、相对当前目录的路径或 `~` 后选择 **Open**；双击
文件夹可进入该目录。远端栏首次打开 profile 中设置的默认目录。本地栏首次打开设置的目录；为空时使用
平台 home 目录，且只读取有界文件元数据。
**Hidden** 显示点文件，**More** 请求下一页远端目录。文件行优先显示目标平台提供的文件类型图标；
平台无法解析时使用内建的目录、链接或通用文件图标。

双击本地栏中的 regular file 会使用平台默认程序打开当前快照条目的只读副本。目录仍用于导航，符号
链接不会打开。AxSSH 会在 UI 线程外核对已打开文件的平台 identity，从该精确 handle 复制到私有有界
缓存，完整发布后再请求操作系统打开；验证后替换原路径不能重定向这次打开请求。

右击远端文件或目录后选择 **Download**。右击未选中的行会先把它设为唯一选中项；右击已选中的行会保留
当前多选集合，因此菜单动作作用于这组选择。每个文件会写入当前 **Local files** 目录；下载目录时会
递归保留所选目录树。远端符号链接和非 regular 条目会被跳过或拒绝，绝不覆盖已有本地文件；单个文件最多
512 MiB。递归发现限制为最多扫描 4,096 个条目，并最多接受 512 个文件、256 个目录、16 层、512 KiB 路径文本和 1 GiB 总大小。

Transfers 区分 **Transferring**、**Failed** 和 **Success** 三个页面。可用勾选框选择活动行并批量暂停、
继续或取消，操作位于 **Transferring** 页签栏右侧，不再单独占用一行。暂停/继续会由仍存活的 worker
保留已下载前缀并从该 offset 续传；仅在当前应用和 SFTP worker
仍运行期间可用。每个 SFTP Tab 最多同时运行或打开两个下载。取消会删除该任务的部分内容，包括刚发布但
取消已生效的文件；失败会删除 `.part` 文件，成功文件保留在所选本地目录。关闭 SFTP Tab 会先取消并 join
待发现、待打开 subsystem 和活动下载，再关闭浏览器和 SSH transport。

远端文件行右键菜单支持删除选中条目（目录不递归）；**Download** 和 **Delete** 不再占用目录顶部工具栏。
其余远端控件保留重命名单个条目、有界 UTF-8 在线编辑和 Save As；本地 regular file 可通过上传按钮或
拖放到 Local files 区进入同一个 Transfers 队列。打开编辑器期间会按远端 size/mtime fingerprint 轮询；
发现变化会禁用保存并提示冲突。自动上传需要勾选 **Auto upload**，默认关闭，且会经过 500ms 防抖与 fingerprint
校验。跨进程恢复和更复杂的冲突合并仍未提供。

## 工作区与终端操作

AxSSH 退出时会把打开的工作区保存到独立的私有 `workspace.json`。重新启动
后会恢复 Tab 顺序、活动 Tab、分屏结构、有界终端文本以及 SFTP 浏览路径。
连接会作为新的 worker 重建，不会持久化活动连接；SSH 仍需经过正常的已信任
host key 和认证流程，已删除的 profile 会跳过。

展开的会话导航以可折叠的 Group 行组织服务器；右击列表空白区域可在没有 profile 时创建
空 Group 或 Ungrouped 服务器。展开后的 Group 行显示名称、数量和
居中的绘制下尖角；收起后显示对应的绘制上尖角，避免名称与文字徽标重复。点击 Group
行，或将焦点移到该行后按 Enter/Space，可切换状态。每台可见服务器仍只占一行：左侧为
名称，右侧为遮蔽后的 endpoint。**View > Toggle Session Sidebar** 可在此视图和紧凑
Activity Bar 之间切换；紧凑栏默认使用 Group 名称前 1-4 个字符作为文字徽标，也可在
**Settings > Workspace** 选择 **Full name** 显示完整组名。Full name 模式会将收起栏加宽到
180px，并使用高密度单行列表：侧栏按钮位于标题行末端，Local Shell 显示图标和文字，Group
显示展开尖角与数量，服务器缩进显示全名；长名称会省略并通过 tooltip 提供全文。紧凑栏行也
提供相同的右键菜单。删除 Group 会把其中
服务器移入 Ungrouped。最近选中的 Group 或服务器会在展开与收起侧栏中持续高亮，hover 和
键盘焦点仍使用独立反馈。删除 profile 也会删除记住的密码，但不会关闭已经打开的终端 Tab。

带文字的按钮和按钮型行保持单行；文案显示不下时使用省略号，鼠标悬浮可在有界 tooltip 中
查看完整内容。纯图标控件继续使用已有的用途 tooltip 和无障碍名称说明操作。

侧边栏默认遮蔽用户名与 IPv4 地址：用户名尽可能保留前后各两个字符，
`192.168.1.202` 显示为 `192.*.202`。可在 **Settings > Workspace** 中修改单个遮蔽
字符。主机名保持可见，便于快速区分目标服务器。新建会话编辑器与终端共用工作区
Tab 条，Settings 则打开为独立的工作台页面。Tab 条最右侧的 `+` 会列出全部已保存连接，
选择后连接对应 profile；**File > New Server** 和侧栏列表空白区域的右键菜单
仍只打开会话编辑器。
可拖拽工作区 Tab 调整顺序。前置数字会随当前位置变化，而 `#1` 这类实例后缀保持不变。
可通过 **Window > Previous Tab** / **Next Tab** 按当前顺序循环切换，首尾相接。固定快捷键
在 macOS 上为 `Cmd+Shift+[` / `Cmd+Shift+]`，在 Windows 和 Linux 上为
`Ctrl+Shift+[` / `Ctrl+Shift+]`。至少打开两个 Tab 时才可用；录制快捷键或处理安全提示时
会暂时禁用。
激活已连接的 Terminal Tab 后，应用会在 Tab 布局更新完成后恢复原生输入焦点，
选中的会话可以直接接收下一次按键。
在既有分屏 pane 之间切换时，目标 pane 会立即取得输入焦点。

要把已连接 Terminal 及其当前工作区中的 terminal pane 作为独立原生窗口使用，可点击连接
Tab 上的外链按钮，或选择 **Window > Move Current Workspace to New Window**。所有 terminal pane
及其 SSH/SFTP companion 会作为一个工作区组移动，保留已有终端输出、SFTP 目录状态、传输队列、
主机密钥提示和认证阶段。SSH、Telnet、Serial 在非主动断开后会自动重连，最多 5 次，退避为 1、2、4、8、16 秒并封顶 30 秒。detached Terminal 窗口只显示 terminal pane，detached
SFTP 视图只显示 SFTP。macOS 的 detached 窗口原生标题栏匹配当前 Terminal 或 SFTP 客户区表面色；点击同一行的重叠窗口
返回图标可把同一份工作区布局合并回主窗口，悬停时会显示 **Return workspace to main window**。直接关闭 detached
窗口也会执行合并，worker 继续运行。Settings 和会话
编辑器 Tab 保留在主窗口。

终端 Tab 顶部管理栏、保存连接按钮左侧有两个分屏图标；macOS 的独立 Terminal 窗口会将同一组图标
放在原生标题栏、紧邻返回图标左侧，客户区保持全高 pane：左侧为纵向分屏，在右侧新建 pane；右侧为横向
分屏，在下方新建 pane。
它们始终作用于当前活动的终端 pane。分屏不会新增顶部 Tab：一个可见 Terminal Tab 管理整套 pane 布局。
也可在终端中使用
`Alt+H`、`Alt+J`、`Alt+K`、`Alt+L` 聚焦左、下、上、右 pane；
使用 `Alt+Shift+H`、`Alt+Shift+J`、`Alt+Shift+K`、`Alt+Shift+L` 在对应方向创建独立终端会话。
每个 pane 都有自己的 local PTY 或 profile connection；SSH pane 会重新执行正常的信任与认证，
包括可能需要的密码或 passphrase 提示。SFTP 不能拆成 terminal pane，并继续作为独立可见 Tab。
每个非根 pane 的右上角都有一个小型关闭控件；点击后只关闭该 pane 及其会话，并自动折叠剩余布局，
根 pane 不提供独立关闭控件。在子 pane 的 local shell 中正常执行 `exit`，或子 SSH/Telnet 正常断开，
也会执行相同的关闭。连接和认证失败会继续保留在界面上便于排查。关闭可见 Terminal Tab 仍会关闭
其布局中的全部 terminal pane。连接尝试失败或非主动断开达到重试上限时，对应 pane 内显示非阻塞提示，
提供 **Retry** 和 **Close**；本地 shell 失败则提供 **Restart**。host-key、认证和 vault 解锁仍只使用该 Tab
自己的阻塞式安全弹窗。

每个 split 都有可见分隔线。拖动竖线可调整 pane 宽度，拖动横线可调整 pane 高度；双击会恢复
等分。分隔线可通过 Tab 聚焦，并接受对应方向键、Home、End，以及用 Enter 或 Space 复位。
两侧分别限制在该 split 的 10%-90%。比例在当前运行期的 Tab 切换和 detached 窗口往返中保留，
应用重启后恢复等分。即使嵌套分屏让某一侧异常狭小，终端行、光标、预编辑文本和原生输入代理也会
裁剪在各自 pane 内；每个 pane 的最后一个完整行都贴住底边。高度不足一个完整字符格的部分会保留在
第一行上方而不裁切它；超过最大行数后的空间也保留在网格上方。只有高度低于三行保底的 pane 才会
裁切较旧的顶部行。
这种裁剪和底部对齐会在 pane 首次布局时建立，而不必等待第一次窗口或分隔线 resize。鼠标拖动 release 或 cancel 后，
输入焦点会返回当前 focused、connected terminal pane；
键盘和无障碍分隔线操作继续保留分隔线焦点。
窗口客户区不再额外绘制应用框线；单个 Terminal pane（包括分屏 pane）也不绘制自己的框线。

终端支持有界回滚、ANSI 颜色、文本选择、原生输入法、F1-F12 和常见 xterm 风格
控制/导航序列。普通终端纵向放大时，可用的真实 scrollback 会显示在当前视图上方；没有历史时，
已有输出保持在顶部，新增空行留在底部。全屏程序的 application-cursor 模式会正确影响 Home 与 End。普通
启用 xterm mouse reporting 的全屏程序可以收到按下、释放、滚轮、拖动和 cell motion，编码按程序选择的
SGR、UTF-8 或传统格式发送。默认开启 **Local selection priority**：普通左键拖动使用本地文字选择，按住
`Alt`（macOS 为 `Option`）才把 button 手势交给 TUI。在 **Settings > Terminal** 关闭该选项后进入标准
xterm 模式：普通点击、释放、拖动和 motion 按 reporting 转发，`Shift` 绕过 reporting 进入本地选区，
`Alt`/`Option` 只作为上报的 modifier bit。两种模式下滚轮在 reporting 开启时都交给 TUI，`Shift` + 滚轮
滚动本地历史；reporting 关闭时，直接左键拖选和滚轮仍由本地处理。手势没有交给 reporting 时，左键双击会按终端核心语义选中一个词，
包括宽字符、软换行 cell 和匹配括号；范围只留在 pane，不会发送给远端程序。Shift 绕过该行为，Cmd/Ctrl 目标激活
和远端 mouse reporting 保持优先；既有 copy-on-select 偏好也会对语义选区生效。同一短点击序列内第三次左键点击会选中完整的逻辑终端行，
包括软换行 cell；行边界仍由终端核心计算。行选区留在本地，并沿用语义选区相同的 reporting、修饰键、焦点、刷新和复制规则。
一次指针手势从按下到释放或 cancel 始终沿用按下时确定的 owner，指针移出 grid 后也不会切换链路。release
上报当时的修饰键状态，cancel 使用最近一次 pointer 状态；高频 motion 每个显示帧只保留最新 cell，并在 release
前刷新最后一帧。1007 只启用备用屏滚轮转换，绝不会接管 button 手势。焦点移到另一 pane、
分隔条或其他窗口控件时会清除终端选区；真实字符格 resize 或实际本地滚动会在坐标不再对应同一视口时清除选区。
屏幕输出刷新只更新网格，不取消选区；**Copy** 会按保持不变的选区坐标读取最新 cell。终端上下文菜单会保留选区至
**Copy** 动作完成。reporting 开启时，
标准模式使用 `Shift` + 右键打开本地菜单；本地选区优先模式使用普通右键，`Alt`/`Option` + 右键仍转发远端。
`Ctrl+C` 会作为中断信号发送给活动终端。Terminal Tab 活动时，**Edit > Copy**、**Paste**、
**Select All** 只作用于 focused terminal pane。Copy/Paste 默认快捷键在 macOS 上为
`Cmd+C` / `Cmd+V`，在 Windows 和 Linux 上为 `Ctrl+Shift+C` / `Ctrl+Shift+V`，并可在
Settings 中修改；Select All 固定为 macOS `Cmd+A`、其它平台 `Ctrl+Shift+A`。
Windows/Linux 中普通 `Ctrl+A`、`Ctrl+C`、`Ctrl+V` 继续作为终端输入。detached Terminal
虽然没有客户区菜单，仍可使用相同键盘快捷键。普通非秘密文本字段继续使用原生编辑快捷键和
右键菜单，秘密字段仍不可复制。
在 macOS 按住 `Cmd`，或在 Windows/Linux 按住 `Ctrl`，再点击可见终端目标即可打开。
按住时，完整识别出的 URL 或路径会显示下划线；松开修饰键或开始文本选择时提示消失。AxSSH 识别
`http://` 和 `https://` URL，并交给本机默认程序；不会自行请求这些 URL。
它也识别以 `/`、`./` 或 `../` 开头的 Unix 远端路径，包括
`/srv/app/main.rs:42:7` 这类诊断位置。路径会激活对应 SSH Tab 的 SFTP companion 并导航到该位置。
没有 companion 时，AxSSH 会以该位置作为初始目录新建独立 SFTP Tab，仍执行常规 SSH host-key 与
认证流程。已有 companion 时，相对路径相对于当前 SFTP 目录解释；如果 companion 仍在主机密钥确认、
认证或 browser 启动阶段，路径只保留在该运行时 Tab，正常流程就绪后再处理。
可选语义高亮可为 HTTP 响应类别，以及常见成功、信息、警告和错误状态词使用不同颜色；该功能默认
关闭。URL 和路径仍由上面的 Cmd/Ctrl 目标交互处理：它们保持原有前景色，仅在按住平台主修饰键时
短暂显示下划线。开启语义高亮后，状态颜色默认跟随所选 Terminal 色表，Settings 可分别覆盖各类颜色；
已经指定 ANSI 或真彩色前景的输出不会被语义高亮替换。
**Settings > Terminal** 的 **Local selection priority** 用于选择默认的本地选区优先交互或标准 xterm
鼠标路由。**Copy selection on select** 默认关闭。开启后，完成鼠标选区和
**Select All** 会立即复制，直接右击始终粘贴。
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
Workspace、Shortcuts 和 About 页面。详情区顶部的搜索框可跨所有页面查找分类名、设置标题和说明，
选择结果会打开对应分类；每个分类的详情内容超过窗口时都可独立滚动。除 renderer 外，修改会立即作用于
当前应用，关闭 Settings Tab 的 `x` 后才会持久化；renderer 只在重启后生效。
Settings Tab 已经打开时再次按其快捷键，只会激活这个单例 Tab。
About 标明 AxSSH 使用 `GPL-3.0-only`，并包含 Slint 标准的可点击署名组件。
About 还提供 **Report a bug**、**Open log folder** 和 **Copy diagnostics**：前者打开 AxSSH
issue tracker，中者打开本机滚动日志目录，后者只复制版本、构建 revision、系统、架构和构建类型。
这些操作不会上传数据。
**Settings > General** 可为 AxSSH 界面选择 **Follow system**、**English** 或
**Simplified Chinese**，并负责选择以后连接时主动记住密码的默认后端。语言设置先成功持久化，
再即时同步到主窗口和独立窗口；保存失败时保持原选择。**Follow system** 在中文系统 locale 下
使用简体中文，其它 locale 使用英文。AxSSH 会翻译应用自有的 Slint 界面；远端终端内容、用户提供的
名称/路径、日志和运行时技术错误详情保持原文。

在 **Settings > Appearance** 中，Font family 只修改应用界面字体，不改变 Terminal 字符格度量；Renderer 可为
下一次应用启动选择 **Automatic**、**GPU** 或 **Software**：Automatic 在 macOS 使用 GPU/Skia，在
Windows/Linux 使用 software；GPU 使用 Skia，Software 使用 software renderer。当前进程设置
`SLINT_BACKEND` 时会覆盖这个已保存选择。Display mode 单独选择 **Follow system**、**Light** 或 **Dark**；Color palette 单独选择
**AxSSH**、**Solarized**、**Arctic**、**Tokyo**、**Ember**、**Forest** 或 **Custom**，因此
所有固定配色都能同时用于浅色和深色。Arctic 偏冷色技术感，Tokyo 偏夜间，Ember 偏暖色，
Forest 使用绿色高对比。在 Dark 模式中，所有配色共用 axshell 的高饱和 ANSI-16 色，同时保留
各自的终端背景、默认前景和选区色；Light 模式保留可读的浅色 ANSI 色表。Custom 会展开
Light/Dark 两套语义色。即时预览和持久化时，
无效十六进制值或会让文字、必要边框、焦点/状态及终端文字看不清的颜色，会回退到对应明暗侧的
可读默认。

**Compact terminal rendering** 默认开启，会移除多余的 run 包装和默认背景 item；只在对照旧 item 树时关闭。
**Cache unchanged terminal rows** 默认关闭；在 GPU/Skia 下可复用静态行图层，但会增加图形内存，Software
模式没有等价缓存。两个选项都会立即预览，并在关闭 Settings Tab 时持久化。

同一 RENDERING 分组中的 **Focused refresh rate** 和 **Unfocused refresh rate** 用帧率设置终端呈现上限，范围为
1-120 FPS；聚焦窗格默认 60 FPS，可见但未聚焦窗格默认 4 FPS。聚焦终端的持续输出仍按 16/33/50 ms 自适应，
更低的设置值会进一步限制这些阶段。隐藏 Tab 不发布终端帧，parser 应答、错误、断开和 shutdown 仍立即处理。
修改会即时预览，并在关闭 Settings Tab 时持久化。

**Settings > Terminal** 独立控制 Terminal 字体、字号、行高、文字亮度、粗体亮 ANSI 色、可选语义高亮、
scrollback、鼠标行为以及平台相关的 Option-as-Meta。文字亮度范围为 60%-120%，步长 5%，默认
100%；100% 会逐值保留解析后的 ANSI/256/真彩色前景。渲染先解析程序颜色和反色状态，再选择已启用的
语义前景，最后只对可见文字应用一次亮度调整；`dim` 也合并在这一次最终调整中。背景、选区和光标不调整。
启用语义高亮后，Success、Information、Warning 与 Error 都可填入不透明
`#RRGGBB`；留空时跟随当前 Terminal 色表。URL/路径目标不提供颜色设置，因为提示只使用交互时的
下划线。两个字体列表都先显示软件自带字体，
再显示自动发现的系统等宽字体。选中的 Terminal 字体始终是主字体；当它缺少汉字字形时，
AxSSH 只使用自带的 Maple Mono NF CN 作为唯一汉字回退。切换 Terminal 字体不会改写已保存的
选择，也不会增加第二条回退链路。

**Settings > X11** 控制当前平台的本机 X server provider、首个 X11 application 时启动和显式的
loopback-only no-auth 兼容模式。已知 provider 的检测位置会以只读方式显示；应用路径只在 Custom
时可提供，且必须指向可执行文件。这些设置都不是秘密；X11 cookie 仍只短暂存在且永不保存。

## 本地数据与凭据

AxSSH 把 profile、非敏感 Group 名称和设置写入平台应用配置目录中的版本化
`sessions.json`。Linux 默认会解析到 `~/.config/ax_ssh/sessions.json`，同时遵守
`XDG_CONFIG_HOME`；macOS 和 Windows 使用各自的标准应用目录。每个 profile 明确包含一份
SSH、Telnet 或 Serial 配置；只有 SSH 可以
包含已确认的主机密钥指纹、私钥路径、指向已记住密码后端的非敏感引用，以及非敏感的 X11
forwarding 开关；X11 cookie 永远不会保存。Serial 可以保存
用于稳定匹配的非敏感 USB 身份元数据。profile 不包含密码、保险库口令、私钥 passphrase、
私钥内容、终端输出或运行中的进程状态。

选择 **System credential store** 后，记住的密码通过 macOS Keychain、Windows Credential
Manager 或 Unix Secret Service 保存。选择 **Encrypted application vault** 后，应用会在
配置目录中为每个 profile 保存一个私有加密记录，保险库口令不会保存。每日日志单独写入
平台本地应用数据目录的 `logs` 子目录，最多保留 15 个文件。`RUST_LOG` 可以覆盖默认的
`ax_ssh=info,russh=warn` 过滤规则。凭据和终端内容不得写入日志；运行日志仍可能包含 host、
port、session ID 或主机指纹等连接元数据，附加到 issue 前应先检查。

已连接的终端可以合法地长时间没有 shell 输出。AxSSH 使用 SSH 传输层的 keepalive 和
inactivity 策略，而不是 shell 输出计时器，来判断连接何时不再存活。

## 当前限制

SFTP 远端文件行右键菜单支持下载和删除当前选择（目录删除不递归）；远端工具栏保留重命名单个条目、
有界 UTF-8 文本在线编辑和显式远端 Save As。保存前会比较打开时记录的文件大小，发现远端变化就拒绝覆盖。本地工具栏可上传一个选中的
regular file：内容先写入私有远端临时文件，再通过 rename 发布，不会静默覆盖已有目标。自动上传和外部
文件监控默认关闭；拖拽只复用有界上传/下载意图，文件读取不在 UI 线程执行。
Serial 的端口可见性、权限和参数支持取决于目标操作系统与硬件；Telnet 不提供加密或自动登录。
主动点击 Disconnect 或关闭 Tab 不会触发自动重连。非主动断开会保留 Tab 和终端滚动内容，显示当前倒计时并为每次尝试创建新的 worker。没有可读取凭据的密码 SSH 会停在认证输入；encrypted-vault 需要解锁；未知或变更主机密钥始终进入既有明确确认流程。达到次数上限后 Tab 保持打开并提示手动恢复。AxSSH 同时读取平台用户的 OpenSSH `~/.ssh/known_hosts`（Windows 使用对应用户路径），支持别名、非默认端口、hashed host、多个密钥和 `@revoked`。有效且未撤销的匹配可直接进入认证；变更密钥仍需确认，撤销密钥始终拒绝。明确确认后，观察到的公钥会以原子方式追加到共享文件，不替换其它记录；读取失败和坏行只会收窄信任，普通确认按钮不能绕过撤销记录。
