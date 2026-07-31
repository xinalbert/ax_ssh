[English](architecture.md) · [文档导航](README.zh.md)

# AxSSH 架构说明

## 边界

AxSSH 是一个独立的 Rust 二进制项目。`third_package/axshell` 仅用于参考，
故意排除在构建图之外；可以参考它的产品行为和评审问题，但不得导入其中的
源码、类型或依赖。

当前实现将 UI、应用、持久化、传输和进程服务拆成独立所有权边界：

```text
Slint UI（.slint）
       │ 生成的 callback / property
       ▼
应用控制器（src/app.rs）
       │ Tab ID + 领域值 + UI event loop 调度
       ├──────────────► 配置存储（src/config.rs）
       │                 版本化设置/profile JSON + 原子替换
       ├──────────────► 系统凭据（src/credentials.rs）
       │                 阻塞式平台 keyring API
       ├──────────────► 终端模型（src/terminal.rs）
       │                 有界 vt100 网格 + scrollback
       ├──────────────► 本地 PTY（src/local_shell.rs）
       │                 有界线程 + portable-pty 子进程
       └──────────────► SSH 边界（src/ssh.rs）
                         Tokio task + russh handle/channel + 私钥加载

进程启动（src/main.rs）
       └──────────────► 日志生命周期（src/logging.rs）
                         滚动 writer + flush guard
```

## 模块职责

| 区域 | 负责 | 不得负责 |
| --- | --- | --- |
| `ui/` | 主窗口组合、功能组件、Settings 分类页面、视觉状态、用户手势和生成的 callback 契约 | 文件系统、Tokio task、russh handle |
| `src/app.rs` | 生成 Slint 类型的声明、进程级 UI 启动和顶层 callback 编排 | 功能实现、SSH 协议细节或 JSON schema 细节 |
| `src/app/macos_window.rs` | 主线程 AppKit 标题栏设置和标准应用菜单 action 绑定 | 生成的 Slint 类型、持久化设置、SSH 或 worker 状态 |
| `src/app/{workspace,connection,connection_monitor,terminal_bridge,settings_bridge,view}.rs` | 私有 application bridge 功能接线、worker 事件消费和 Slint model/snapshot 映射 | 生成类型声明、传输实现或持久化 schema |
| `src/app/state.rs` 与 `src/app/state/` | 与 UI 无关的工作区 Tab、逐 Tab 终端/worker 状态、attempt 转换及测试 | Slint component/model 类型或 russh 协议细节 |
| `src/app/{input,session_groups,terminal_render,credential_tasks}.rs` | 可测试的输入/分组/渲染映射、主题化终端默认色和阻塞式凭据 task 边界 | 窗口所有权、传输 handle 或可变 UI 状态 |
| `src/config.rs` | `SessionProfile`、持久化 Group 名称、版本化 `AppSettings`/`ThemeSettings`、校验、旧配置迁移、JSON 持久化和原子替换 | Slint 类型、网络连接、明文密码存储 |
| `src/credentials.rs` | 按 profile 访问平台系统凭据库 | UI 状态、明文配置、SSH 传输 handle |
| `src/terminal.rs` 与 `src/terminal/input.rs` | 有界 vt100 网格、字符格样式、光标/scrollback 状态、选区提取和终端按键编码 | Slint 类型、网络 handle、凭据 |
| `src/local_shell.rs` | 跨平台 shell 发现，以及每个 Tab 一个由有界 worker 独占的本地 PTY 子进程 | Slint 状态、SSH 信任、持久化终端内容 |
| `src/ssh.rs` | russh handler、主机密钥决策、认证、shell channel 边界 | 窗口更新、持久化会话修改、UI 格式化 |
| `src/ssh/private_keys.rs` | 本机 `.ssh` 私钥发现和阻塞式密钥加载 | passphrase 持久化、UI 状态、主机信任决策 |
| `src/ssh/worker.rs` | 有界 shell 输入命令、合并式 resize 状态、批量输出事件、取消和关闭 | UI 状态或 profile 持久化 |
| `src/logging.rs` | 全局 tracing subscriber、日志目录、按日滚动、保留和 flush guard | 凭据、功能状态、UI 或 SSH handle |
| `src/main.rs` | 进程启动和日志 guard 生命周期 | 功能逻辑 |

## 事件流

1. Slint callback 只产生已保存 profile ID、唯一 Tab ID、终端按键/修饰键、
   草稿字段、信任决策或一次性临时密码等小值。
2. 每次打开 profile 或本地 shell 都会创建新的终端 Tab UUID，即使另一个 Tab 使用
   相同目标。SSH 输入、resize、输出、重试和关闭按 `tab_id + attempt_id` 路由；本地
   操作按 `tab_id` 路由。未知 SSH 主机会启动绑定该 Tab 的可取消探测，但传输仍拒绝。
   工作区 Tab 顺序是仅在内存中的展示状态：拖拽释放会把 Tab UUID 和受限目标位置交给
   `AppState`，它只重排现有 Tab 列表。按住期间 Slint 保留半透明的源槽、高亮目标槽，
   并在指针位置绘制不可交互的 Tab 副本；不会创建第二个运行时 Tab。前置 UI 序号从当前
   列表位置派生，而 `#1` 这类实例后缀仍是稳定标题的一部分。
3. 用户明确确认后，控制器才原子持久化精确指纹。密码 profile 通过 Tokio blocking
   边界读取已记住的凭据或打开密码弹窗；私钥 profile 在 UI 线程外加载所选路径，
   只有加密密钥无法空口令打开时才请求一次性 passphrase。
4. 新建或编辑 profile 时明确保存的密码会先通过 blocking 凭据边界修改，再提交
   profile 事务；profile 写入失败会恢复原凭据。编辑时旧密码绝不加载进 Slint：保持
   已保存标记且密码留空表示继续使用现有凭据。取消保存标记或删除 profile 会删除其
   系统凭据，但删除 profile 不会停止已经打开的终端 worker。在认证弹窗输入的密码只在
   SSH 认证成功后写入。已存凭据缺失或被拒绝时清除非敏感标记，并回退到一次手工密码提示。
5. 终端表面把 Slint 特殊键转换成与 UI 无关的终端键值；平台对 `Shift+-` 仍上报
   `-` 时只在该映射层后备转换为 `_`。`src/terminal/input.rs` 生成控制字节、普通 CSI
   或 application-cursor SS3 方向键，以及带修饰键的 xterm 序列。macOS 在应用边界
   还原 Slint Apple 映射中交换的 Control/Command 语义。一个透明、随光标定位的
   `TextInput` 只作为原生 IME 代理；提交文本进入终端编码器，预编辑保留在 UI 状态。
   普通 `Ctrl+C` 保留为 PTY 输入；终端获得焦点时 Ctrl 组合优先。剪贴板操作在 macOS
   保留 `Cmd+C/V`，其他平台使用 `Ctrl+Shift+C/V`。工作区命令使用平台主修饰键。
   选区复制留在 UI，粘贴内容作为有界 shell 输入发送；可选右键行为根据是否存在选区
   选择复制或粘贴。
6. 认证后每个终端 Tab 持有一个 worker，该 worker 独占一个 PTY shell 及其 russh
   handle/channel。同 profile 的重复 Tab 使用彼此独立的有界命令队列和单槽尺寸状态。
   关闭 Tab 时先移除事件路由，再异步 shutdown 对应 worker，迟到事件不会更新其他 Tab。
7. 本地终端 Tab 改为持有一个 `portable-pty` worker 线程；它在 Tab 生命周期内独占
   child、reader、writer、resize 状态、有界命令/事件队列、取消标记和超时 join。
8. 每个终端 Tab 还持有一个有界 `TerminalModel`。`vt100` 负责行、字符格样式、光标、
   scrollback、宽字符和 application-cursor 模式。仓库内的 `vendor/vt100` 补丁保持锁定
   的 `0.16.2` API 不变；在缩窄列数会移除宽字符续位格时，先清除对应的宽字符首格，且
   同时覆盖普通与备用屏幕。非活动 Tab 的输出留在 Rust 状态，只有活动字符格快照进入
   Slint event loop；更新统一使用
   `slint::invoke_from_event_loop` 和 `Weak<AppWindow>`，避免退出时保活窗口。
   小屏窗口下限为 `520x360`；终端布局、持久化默认尺寸和模型统一使用非零的 `10x3`
   网格下限，既允许窗口紧凑缩小，也不会向 PTY 发出非法的零尺寸 resize。窄屏时可通过
   现有侧栏收起动作优先为终端让出列数。
   `TerminalPane` 会把测得的网格、配置字体度量、活动终端 Tab 身份和连接状态变化合并到
   下一次 UI 轮转后，再请求一次最终 PTY 尺寸。因此 Settings 修改字体后返回已连接终端时，
   与窗口缩放会走同一条当前网格更新路径。
   本地或 SSH worker 接受 resize 请求后，应用会立即调整活动 `TerminalModel` 并安排
   活动终端刷新。该 UI 任务实际执行时才从 `AppState` 复制当前快照，而不应用先前
   worker 事件已捕获的旧快照；因此已经排队的 Output 不会在用户持续拖动窗口时把界面
   恢复为旧网格。worker 随后到达的 `Resized` 仍只作为传输确认。
9. macOS 应用保留标准原生标题栏，并关闭 AppKit 的整窗背景拖动。窗口移动只由该原生
   标题栏处理；Slint 工作区 Tab 条作为其下方的普通客户端内容呈现，因此原生窗口拖动
   不会再与 Tab 重排手势竞争。
10. 平台菜单的 Settings 和 About 意图分别把同一个单例 Settings 工作台 Tab 打开到
    General 或 About。它与正在运行的 SSH/本地终端 Tab 一起留在可见工作区 Tab model
    中，因此激活 Settings 不会移除返回活动终端的路径。Close 只移除该单例 Tab，绝不
    影响任何终端 worker。页面切换时未保存草稿仍由 Slint 持有；只有标题栏 Save 会跨入
    应用边界。About 展示静态产品用途说明，并只接收编译期 package 版本作为只读 UI
   元数据。会话侧边栏不再重复 Settings/About，并从原生标题栏下方贯穿整个客户端高度；
   工作区 Tab 条只占其右侧的工作区列。`+` 固定在最右边缘，打开由 Slint 本地持有的
    选择器，显示全部已保存 SSH profile 的遮蔽只读快照，选择后只将 profile UUID 传入
    现有连接 callback。File > New Session 与侧栏列表空白区域的右键菜单仍是独立的新建
    会话编辑器动作。
11. 单一声明式 Slint `MenuBar` 持有跨平台业务菜单树。锁定的 winit/muda 后端把它安装
    到 macOS 屏幕顶部和 Windows 原生窗口菜单；没有 native menu 支持的 Linux 后端在
    客户区顶部渲染同一棵树。macOS 的 `src/app/macos_window.rs` 复用后端已创建的标准
    应用菜单，把现有 About 接到内部页面，并插入带 `Cmd+,` 的 `Settings...`。AppKit
    target 只在主线程运行且只捕获 `Weak<AppWindow>`；由于 target 为弱引用，菜单项用
    represented object 保持其生命周期。macOS 的关闭 Tab 菜单项刻意不绑定动态活动
    Tab 状态，因此 Tab 身份或类型变化时 Muda 不会重建原生菜单；Settings/About 只由
    AppKit bridge 安装一次。Windows/Linux 仍保留动态关闭 Tab enabled 状态，并在
    Edit/Help 提供 Settings/About；其他菜单复用已有的新建会话、侧栏、本地 shell、
    关闭 Tab 和快捷键意图。
12. 会话导航持有一个 Slint 侧边栏展开/收起状态，以及应用层所有、仅在内存中的 Group
    展开状态。`AppState` 用 `BTreeSet` 保存规范化的展开名称；持久化 Group 名称改由
    `SessionStore` 持有，因此空 Group 也能跨重启保留。展开态先渲染 Local Shell 卡片，
    再渲染可折叠的 Group 父行及其单行服务器子项；进入 Slint 的 endpoint 仍是遮蔽值。
    展开父行显示名称、数量和居中的绘制下尖角；收起
    父行显示对应的上尖角。只有紧凑栏以 Group 名称的两个字符生成文字徽标而非文件夹图标。
    自定义 Group 行可通过键盘获得焦点，Enter/Space 与点击发出相同的 Group 切换意图；
    只有独立的紧凑面板按钮负责展开或收起侧栏。原生行右键菜单可在 Group 内新增服务器、
    重命名或删除 Group，以及连接、编辑或删除服务器；Ungrouped 只提供新增服务器。右击
    列表空白区域可新建空 Group 或 Ungrouped 服务器。`SessionActionMenu` 把四种菜单形态
    映射为扁平 `ActionMenuItem` 列表；`FlatActionMenu` 只组合一个 `ContextMenuArea`，只发出
    action ID，并暴露 `show-at(Point)`，使同一动作列表也能由按钮触发为下拉菜单。删除 Group
    会把 profile 移入 Ungrouped；删除 profile 只移除持久化定义和凭据。收起态用更大的
    Group 徽标和更小、连续排列的服务器徽标保留层级，Local Shell 保持专用入口。应用层
    formatter 会在数据进入 Slint model
    前遮蔽用户名和 IPv4 的中间段。静态尺寸进入 `ui/theme.slint`，持久化的单字符遮蔽设置
    由 `WorkspaceSettings` 持有。

## SSH 安全契约

`russh::client::Handler::check_server_key` 是信任边界。未知和不匹配的主机密钥都在
认证前拒绝。首次拒绝握手可以把 SHA-256 指纹交给确认 UI，但只有用户明确决定后，
该精确指纹才进入 profile；密钥变化需要再次明确确认。密码只作为 callback 的临时
输入，不进入 `SessionStore`。profile 只包含 `credential_stored` 标记；密码本身以稳定
profile UUID 为键存入平台系统凭据库。私钥 profile 只持久化路径；私钥内容和可选
passphrase 只在一次 blocking 加载/认证任务中短暂存在，不进入配置、tracing 字段或
UI model。

认证后连接遵循以下生命周期：

- 每个终端 Tab 有唯一运行时 UUID，并由一个 worker 在完整生命周期内独占 russh handle；
- 有界命令 channel 传递 shell 输入、断开和取消意图；watched terminal size 合并
  高频 resize 更新；
- 终端输出按批次限制大小，并通过有界事件 channel 反压后进入有界终端模型；
- worker 事件报告 connected、resize、output、disconnected、host-key rejection、
  凭据失败或截断后的错误；
- 取消既能中断连接/认证，也能断开已建立会话；
- 20 秒 keepalive 和三次未响应上限保持健康空闲会话，同时保留 90 秒 inactivity 边界；
- 关闭 Tab 先使 Tab/attempt 路由失效，再请求 worker shutdown；
- 窗口退出对所有剩余 worker 请求断开，在超时边界内逐个等待 join，最后再关闭 Tokio。

## 日志生命周期

`src/main.rs` 在创建 UI 前建立唯一的 `LoggingGuard`，并保持到 Slint 与 Tokio 生命周期
结束之后。`src/logging.rs` 通过有界无损队列写入按 UTC 日期滚动的文件，最多保留
15 个，同时把 `INFO` 及以上事件镜像到 stderr。guard 释放时先写退出事件，再排空
队列、刷新当前文件并 join writer 线程。运行字段可以包含 session ID、host、port 和
主机指纹；禁止记录凭据和终端内容。

## 持久化设置与字体资源

`assets/fonts/JetBrainsMono-Regular.ttf` 是由 Slint 编译器注册的项目自有静态资源，
同目录保留 OFL 许可证和作者声明。构建和运行时都不会从 `third_package/axshell`
加载字体。Slint 测量配置字体，并用测得的字符格宽度和配置的行高百分比统一计算
渲染、选区、光标和向下取整的 PTY 尺寸；终端只会在这些度量和布局稳定后合并发送 resize。

`SessionStore` 在现有私有 `sessions.json` 中写入版本化 profile、非敏感 Group 名称和
`settings` 对象，包括经过约束的字体、字号、行高、终端亮度、粗体亮色和右键行为、
scrollback、默认 PTY 尺寸、
本地 shell 选择和有上限的发现缓存、侧栏/Tab 宽度、会话遮蔽字符、快捷键及
`ThemeSettings`。显示策略独立保存为 System、Light 或 Dark，配色方案独立选择 AxSSH、
Solarized 或 Custom。Custom 分别保存 Light/Dark 两套 13 个语义 UI/终端默认色，并规范化为
`#RRGGBB` 或 `#RRGGBBAA`。schema 版本 11 会拆分旧的组合模式：Solarized Dark 迁移为
Dark + Solarized；旧 Custom 按背景亮度进入对应的一侧，另一侧使用安全 AxSSH 默认。
主题规范化会保证 Light 表面保持浅色、Dark 表面保持深色；正文、焦点/强调和状态色至少
4.5:1，必要边框至少 3:1，不安全的终端前景/选区组合回退到相同明暗侧的安全默认。
schema 版本 10 会把旧 profile 中的 Group 值提升为规范化、去重后的 Group 列表，从而
持久化空 Group 和重命名结果。schema 版本 9 会把旧终端配色迁移到对应的固定主题，以保持
升级前的外观。启动时会验证已有 shell 缓存并只追加新发现项；更早的迁移继续只将 schema
版本 7 的旧默认 260px 侧栏改为紧凑 220px，并增加 schema 版本 8 默认 `*` 的遮蔽设置，
不覆盖用户自定义值。密码、passphrase、私钥内容、终端输出、Tab 运行时 ID、子进程和
worker 永远不会序列化。

即使没有已保存 profile，展开会话侧栏也会保留列表空白区域的右键菜单，用于新建空 Group
或添加第一台服务器。用户手动收起后切换为窄栏，窄栏仍保留 Local Shell 和相同的行/列表
右键菜单。Settings/About 只保留在平台菜单和快捷键中，不再进入左侧栏。

`src/app/view.rs` 将已选 palette 经过校验的 Light/Dark 两侧同时送入 `ui/theme.slint`。
System 模式把标准控件 `Palette.color-scheme` 保持为 `ColorScheme.unknown`，由 Slint 跟随
运行时平台 palette；手动 Light/Dark 则显式设置。唯一的 `resolved-dark` 同时选择对应 palette
侧、标准控件方向、AxSSH 自绘表面和终端 ANSI palette。Theme 还显式命名 divider、frame/
control border、focus、hover 和 selected 状态 token，避免共享组件各自重新解释基础色。
原生 `ContextMenuArea` 仍由平台绘制，所以它的具体色值可能不同，但明暗选择保持一致。主题
还统一字号层级、间距、圆角、标准工作区尺寸、Settings 控件尺寸、编辑器宽度和覆盖层尺寸。
`ui/components/themed-combo-box.slint` 统一拥有所有需要 AxSSH 精确配色的应用内选择控件；
控件表面、弹层、hover/选中行、焦点边框、箭头和滚动指示全部消费语义 `Theme` token，不再
使用 Slint 标准控件 palette。组件保留有界字符串 model、current-index、selected callback、
键盘导航、点击外部关闭和 combobox 可访问性契约。其它标准控件继续使用已同步的 Slint
`Palette`，原生 `ContextMenuArea` 菜单仍由平台拥有。
`ui/components/settings-controls.slint` 使用这些 token 提供共享的 Settings 图标、导航、
页面、右对齐紧凑字段、设置行、开关、快捷键和操作标题栏。设置行保持稳定的标题/元数据
列，标准控件统一使用 Theme 配置的高度。`ui/settings.slint` 持有统一草稿和一次 Save
事务，各分类布局拆到 `ui/settings/*.slint`，只接收本分类需要的草稿属性和 callback。
`ui/settings/appearance.slint` 将 Display mode 与 Color palette 分开，并用一个共享
`ThemePaletteEditor` 组件渲染 Custom Light/Dark 字段，避免两套编辑器结构漂移。
`src/app/view.rs` 将保存的主题映射进 Slint global，并在解析色变化时只重新渲染当前终端
快照。终端渲染使用解析后的默认前景、背景和选区色，仍保留既有 ANSI 16/256 色语义。
主题刷新不会 resize PTY、发送 worker 命令或改变 SSH/本地 shell 生命周期。运行时终端
几何与用户选项仍进入版本化 `AppSettings`；Theme global 只作为视觉解析器，不拥有持久化状态。

## 分阶段范围

当前应用可校验并持久化 profile、确认逐 profile 主机指纹、使用临时密码或本机
私钥认证，并持有多个逐 Tab 隔离的 SSH 或本地交互式 PTY shell，相同目标也可重复
打开。新建会话编辑器和单例 Settings 工作台都属于可见工作区 Tab；只有短期信任和
secret 提示保留为覆盖层。
以下内容仍作为独立步骤：

- 共享的 OpenSSH 兼容 known_hosts 存储和主机密钥撤销；
- 与认证策略共享的独立 SFTP worker；
- SSH agent、重连和工作区恢复；
- 更完整的全屏终端兼容和鼠标上报。
