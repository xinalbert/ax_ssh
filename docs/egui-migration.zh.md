[English](egui-migration.md) · [文档导航](README.zh.md)

# egui UI 迁移计划

## 状态与交付范围

- 目标 ID：`20261022-egui-migration`。
- 工作分支：`codex/egui-ui-migration`，从 `master` 的 `15404e1` 建立。
- 已落地：可选 egui/eframe 0.37.0 独立入口、Rust 1.95.0、隔离日志、本地 PTY/controller 与 Painter 终端，以及 E3 共享终端/视口状态、连接身份与重连策略，以及 E4 临时 SSH/Telnet/Serial、信任/认证提示和有界重连，E5 保存会话/分组、Tab、八 pane 布局及隔离恢复；原生 GUI 与性能验收待完成。
- 目标：保留现有终端、连接、配置和安全行为，逐步用 egui 前端替代 Slint。
- 当前架构以 [架构说明](architecture.zh.md) 为准；tracker 区分已实施的 E1–E5 切片、原生验收和剩余 E3R/E6–E9。

## 架构决策 D1：独立前端，先验证终端

过渡期保留可运行的 Slint 基线，增加独立的 egui 可执行入口。两个前端分别启动，
不在一个进程中同时运行两套 GUI event loop。先交付真实本地 PTY 终端，再抽离并复用
应用控制层，最后迁移完整工作区。共享控制层从实际复用需求中提取，不预先构造通用 UI 框架。

egui 负责布局、局部交互、字体/纹理/GPU 资源和平台窗口适配；控制层负责业务命令、
Tab/连接状态、后台工作、终端解析、持久化与退出清理。Tokio、russh 和
`alacritty_terminal` 继续承担现有职责。不得把网络、磁盘、终端解析或等待任务放进绘制回调。

本决策尚不表示达到切换条件。E1 已同步调整 `AGENTS.md`、项目技能、双语架构和构建检查；
E9 再记录最终切换决策。当前 controller 持有实验临时终端及共享的鼠标编码、输出效果、
视口/选区状态、帧准入、清理逻辑和非秘密的连接身份/重连状态；E4 新增有界临时连接/提示 owner。其余工作区/安全/设置提取随对应功能推进（E3R）。

## 源码复用与重写范围

| 当前路径 | 处理方式 | 注意事项 |
| --- | --- | --- |
| `src/{ssh,sftp,telnet,serial,local_shell}.rs` 及其子模块 | 保留 transport/domain 实现 | 不更换协议框架，不共享 SSH/SFTP 活句柄 |
| `src/{config,credentials,audio,media,logging,x_server}.rs` 及子模块 | 保留领域、存储和服务 | 只调整确有前端语义变化的配置兼容层 |
| `src/terminal.rs`、`src/terminal/`、`src/terminal_dimensions.rs` | 保留解析、快照、选择、编码、尺寸契约 | 终端列宽由模型决定，字体 advance 不得改变逻辑列数 |
| `src/app/state/`、`panes.rs`、`worker_shutdown.rs` | 提取 UI 无关部分 | `state` 仍关联媒体和外部编辑服务，不能直接整目录移动 |
| `src/app/terminal_render.rs` 及子模块 | 复用配色、样式、字体角色和几何规则 | 现有几何输出为路径字符串，需适配为 egui 几何命令，不能视为现成 mesh |
| `src/app/{connection,connection_monitor,workspace,sftp_bridge}*` | 分离业务编排与 Slint 通知 | 保留 attempt/request 校验、事务和取消，不复制第二套协议业务 |
| `src/app/{view,input,font_bridge,window_router,window_bridge,text_editing}*` | 重写或拆分 UI 适配 | 键盘、字体、窗口和测试都含 Slint 专属实现 |
| `src/file_icons.rs`、`src/file_icons/` | 共用平台发现/缓存和有界自有 RGBA | E6 移除 Linux Slint 解码器；两套前端消费同一服务 |
| `ui/` | 按功能重新实现 | 当前 47 个 Slint 文件、16,353 行；不进行语法机械翻译 |
| `build.rs`、`build/`、`translations/`、`scripts/`、CI/打包 | 随阶段迁移 | 继续生成图标、revision、语言资源并校验双语文档和发行元数据 |

## 建议模块与构建组织

保留一个 Cargo package，避免为迁移建立多 crate 工作区。候选结构如下，具体拆分随 E2/E3 代码审查确定：

```text
src/main.rs              现有 Slint 入口，保持薄启动层
src/app.rs + app/        过渡期 Slint 适配
src/egui_main.rs         egui 入口，只初始化日志并启动前端
src/egui_app.rs          egui 应用组合与启动
src/egui_app/            workspace、terminal、sftp、settings、theme、fonts、platform
src/controller.rs        有意设计的共享应用入口
src/controller/          state、commands、events、connections、presentation、shutdown
src/lib.rs              保留领域导出，显式导出最小 controller facade
```

内部实现默认私有；库边界仅暴露前端确实需要的控制入口、命令和快照，字段继续封装。
Slint 生成类型继续只在 `src/app.rs` 声明；egui 类型只进入新前端和其平台适配。

E1 计划加入默认 `slint-ui` 与可选 `egui-ui` feature，现有 `ax_ssh` binary 要求
`slint-ui`，实验 `ax_ssh_egui` binary 要求 `egui-ui`。两者可一起编译，但独立运行。
显式设置 `default-run = "ax_ssh"`，保证增加第二个 binary 后既有启动命令仍可用。
Slint runtime、`i-slint-core` 和 `slint-build` 需要按 feature 控制；`build.rs` 对
Slint 编译函数本身加编译期 cfg，避免未启用依赖时仍引用它。Windows 图标、revision
和通用语言注册表生成继续执行。仅 egui 构建不能间接编译 Slint 或其 vendor 补丁。

当前可运行的两个入口：

```bash
cargo run --locked
cargo run --locked --no-default-features --features egui-ui --bin ax_ssh_egui
```

两种前端和同时启用的组合都需 CI 检查。实验程序默认使用独立的配置、工作区和日志目录；
测试使用隔离存储及注入凭据。读取既有配置作为只读比较输入时，不得隐式写回或自动连接。

## egui 版本与图形后端

- 当前选用 `egui/eframe 0.37.0` + WGPU，系统字体回退和彩色 Emoji 改进与终端需求相关。
  包 MSRV 和 CI 同步提高至 Rust 1.95.0；本机已安装最低工具链并验证 egui-only check。
- 0.35/Rust 1.92 仅为前期评估的备选，本分支没有采用；默认入口仍是 Slint，Slint 版本未升级。
- 原型优先用 egui Painter、文本布局缓存及几何批次，不提前编写自定义 WGPU renderer。
  只有测得瓶颈后才评估 paint callback/字形 atlas。
- WGPU/OpenGL 是 eframe 的标准路径。现有软件渲染、局部 damage presentation 和 GPU 故障回退
  不能直接迁移。E1 记录支持策略，E9 前必须验证替代或保留受支持路径；不得把软件驱动等同于现有 CPU renderer。
- Windows DX12、macOS Metal、Linux Vulkan/GL 的选择和诊断需原生验证，保留显式诊断 override；
  不直接继承参考项目针对特定驱动的后端黑名单。

依据：[0.37 manifest](https://github.com/emilk/egui/blob/0.37.0/Cargo.toml)、
[0.35 manifest](https://github.com/emilk/egui/blob/0.35.0/Cargo.toml)、
[0.37 发布说明](https://github.com/emilk/egui/releases/tag/0.37.0)、
[eframe features](https://github.com/emilk/egui/blob/0.37.0/crates/eframe/Cargo.toml)。

## 控制层与 UI 的数据流

1. UI 产生带 `window_id`、`tab_id` 和必要 attempt/request ID 的类型化意图；egui 的多次布局 pass
   不得重复启动连接、保存、发送按键或上传，同一用户操作只执行一次。
2. 控制层通过有界命令入口执行操作；输入过载须显式报告，resize/motion 可按既有契约合并。
   凭据用短期专用载荷，不进入可打印命令、普通快照或日志。
3. transport 字节按序全部进入现有解析/协议响应链；不可用“只保留最新项”丢弃原始输出或用户输入。
4. 只允许合并已解析的展示状态和刷新通知；每个 Tab 保留一个最新有界快照，唤醒按窗口合并。
   跳过中间快照后需比较行 revision、合并变化行或完整刷新，不能只消费最后一批 `dirty_rows`。
5. UI 通过快照接口读取拥有的数据，释放同步锁后绘制；不接触 worker 句柄、Tokio receiver 或持久化事务锁。
   前台/后台/隐藏窗口策略继续独立于协议解析和输入处理。
6. egui 唤醒回调由前端安装，controller 只持有可注销的通知接口及窗口标识，不持有 `egui::Context`。
   UI 本地状态使用稳定 UUID 派生 ID，保持焦点、选择和编辑草稿。
7. 关闭先撤销安全/媒体/剪贴板意图、断开并登记 cleanup，再撤除 Tab 路由。
   退出继续 drain 既有清理注册表及工作区写入，不用直接结束进程跳过清理。

SSH 主机信任维持默认拒绝和显式确认；迟到结果继续核对 Tab/profile/attempt/phase。
SFTP 目录 request、编辑冲突和上传确认沿用现有控制逻辑。安全回归不得用禁用校验来通过。

## 终端实现方案

E3 首切片已由两套前端复用。共享 `TerminalOutput` 收集协议响应、标题/铃声/剪贴板效果和
同步输出等待状态，不执行平台操作；共享 `TerminalFrames` 保留准入帧并合并待消费脏行，
各前端继续拥有自身调度策略。共享 `WorkerShutdowns` 在 Tab 或完成通知被丢弃后继续持有
清理任务，退出期限内排空，超时则 abort 并 join。鼠标事务使用相同模型模式及 256 条滚轮
上限。Slint 保留标题/剪贴板身份和权限校验，egui 继续丢弃这些效果。其余应用状态仍待 E3 提取。

E3 第二切片共享 `TerminalViewport`，把 resize、历史滚动和返回实时输出时的旧帧与选区坐标
失效统一为一个事务。重复尺寸与零位移不推进选区代次，普通输出和单纯展示失效也保留代次；
两个前端使用同一非负代次并显式回绕到 1，帧缓存收为 controller 内部实现。
PTY/controller 去重只记录成功接纳的 resize，失败后同尺寸仍可重试，模型不提前变化。
真实 PTY 回归覆盖 resize 后携带旧代次的 Copy/selection 请求被拒绝。

E3 第三切片提取协议/profile/attempt 身份和五次重连策略，由现有应用适配层消费，
egui-only 库可独立编译和测试。状态变化前仍先定位 Tab 并核对 profile/attempt；取消或手动
重试推进代次，应用层继续撤销媒体和剪贴板意图。controller 类型不持有 worker、信任决定、
秘密或 UI 值。这一切片为 E4 提供状态基础，尚未在 egui 增加远端连接控件。

- 绘制只遍历可见网格；保留模型 scrollback，禁止把全部历史构造成 egui 长文本或逐格 widget。
- 以窗口 DPI、字号和经过测量的等宽字体计算 cell；绘制、命中测试、选择、IME 和远端 resize 共用坐标原点。
  保留当前完整行、顶部余量、小窗口裁剪和最大尺寸规则。
- 按行 revision、主题、字号、字体代次和 DPI 缓存文字布局/绘制数据；限制缓存大小，隐藏和关闭时释放。
  初版以样式 run 批次绘制文字和背景，几何符号用 Painter 路径；光标、选择、链接下划线和预编辑为覆盖层。
- 字体继续使用现有 JetBrains Mono、Maple、Iosevka 及许可证；后台读取/发现，UI 注册。
  中文双宽、零宽组合、ZWJ Emoji、框线、斜体和未知字体回退不改变模型 cell 占位。
- 建立专用终端输入适配：分别处理物理键、文本提交、粘贴和 IME preedit；仅 commit 发送文本。
  校验 macOS Option/Command、Windows Control/Alt、keypad、撤销和菜单消费，避免双发及密码字段泄漏。
- 保留应用/普通光标键、bracketed paste、焦点、鼠标协议、OSC 颜色/尺寸/标题、2026 同步输出和 OSC 52 权限流程。
- 静止时按需重绘；光标 blink 和可见状态变化使用定时请求；持续输出沿用有界刷新节流。

## E4 临时远端连接

实验终端可通过明确的 SSH、Telnet 或 Serial 草稿替换启动时的本地 shell，复用原有 worker，
不增加协议实现。`TerminalSession` 持有当前接收器/worker、解析器、prompt 身份、临时秘密和
可取消的重连期限。替换前丢弃旧接收器并 join 旧 worker；输入和复制/选区结果绑定当前画面
的连接代次。重置清除旧帧并推进选区代次，不跨连接复用坐标。

SSH 认证前先 probe，未知密钥提供仅本次信任，认证 transport 再核对固定指纹。
known_hosts 变更、撤销和 probe/worker 期间换钥继续阻断。prompt token 精确且单次使用，
确认和认证重试重新索取凭据。不写入 profile、known_hosts 或凭据。
临时 SSH 在设置/权限界面迁移前关闭 X11 和远端麦克风初始化。

可选重连共用五次预算，延迟为 1/2/4/8/16 秒。SSH 密码/私钥口令重试暂停并索取新输入，
agent 和直连协议可不保留秘密而重试；主动断开/替换撤销期限。真实 loopback SSH/Telnet
覆盖信任、认证、输入/输出/协议响应、像素 resize、取消、替换和重试；串口失败测试不使用
真实硬件。原生服务器/agent/私钥提示、串口设备和 GUI 验收保留为 E4V。
E5 已增加保存会话、侧栏/编辑器、Tab/pane 和工作区恢复，见下节；不将 E2V 或完整应用提取
记为完成。

## E5 保存会话与工作区

默认应用和 egui 共用 `PaneTree`，保留方向焦点、分隔比例、八 pane 上限和恢复校验。
实验工作区 owner 增加 64 命令 facade、目录版本/请求回执、原子后台写入、关闭/join 所有权，
32 个终端的资源预算包含关闭中任务。egui 组合隔离的会话/分组侧栏、不含凭据的 profile 编辑器、
Tab 排序、分屏及逐 UUID 终端/输入缓存。密码控件按 pane 隔离；编辑器/模态禁用状态不向终端
发送文本，重复布局 pass 只接纳一次用户动作。

实验使用独立 ConfigStore 和工作区 schema，每两秒检查变化布局并在退出保存；`.axssh`
导入/导出复用有界原子文件 helper。无效/不支持的导入保留当前工作区，损坏启动文件保留。
所有恢复终端，包括本地 shell，先 idle。保存会话引用预填连接字段，不保存临时目标/草稿、
终端输出/历史、凭据或仅本次信任的密钥。原生文件选择器/最近菜单、凭据存储、完整旧工作区
功能对照及设置/翻译仍属后续界面；E5V 跟踪原生焦点/布局/恢复验收。
本轮交付 E5I，不将最终兼容/性能切换门禁记为通过。

## E6 平台文件图标

两套前端共用 `src/file_icons.rs` 与平台 resolver；Slint adapter 只 re-export 服务。
实验 companion 给 remote/local 条目加类型 key，发布最多 64 个共享 24×24 RGBA 图标。
单个 blocking 预热任务由 companion 持有，完成后更新当前目录快照，关闭时 join。
provider 保留 128 项缓存、受限扩展名、platform/theme identity 失效和固定 fallback。

egui 每 pane 最多 64 个纹理，只更新像素 allocation 改变的图像，在 pane 隐藏/关闭或
窗口最小化时释放。Linux SVG 使用固定 resvg 0.48.1，关闭默认 features 和外部/内嵌图片
resolver，alpha 转为 straight RGBA。host-key receiver 在图标/命令唤醒间持续借用，
结束时取消/join task。原生主题/视觉和完整拖放对照仍为 E6 门禁。

## E6 外部编辑

实验界面复用共享编辑状态/私有副本服务，提供 Edit、Reopen、Upload changes、Stop 和
自动回传保存开关。单个保留任务轮询稳定文件变化，快照仅有元数据，不传正文。缓存独立，
关闭保留副本，未发布副本清理。精确 edit/operation ID 和连接代次拒绝迟到任务/回执，
原写 worker 继续执行远端冲突检查与原子替换。E7 再迁移编辑大小设置，egui 当前 4 MiB。
系统关联/实时编辑的原生验收和拖放对照继续保留。

## E6 有界拖放

实验界面只在可见 Remote files 区接收系统文件/目录，支持同 pane 本地到远端、远端到本地
复制。payload/intent 带 pane、连接和目录身份；单个保留 blocking 任务准备递归上传，
入 transport 前复验，关闭取消并 join。共享扫描保留相对路径，跳过链接与过滤名。
原生坐标查询留在 egui 适配；无坐标或旧坐标不回退活动 Tab，Wayland 要求当前帧指针事件。
原生拖出 file promise、选择批次、完整 SFTP 对照与原生拖放验收保留。

## E7/E8 菜单、Tab 与设置基础

按用户优先级，本切片先于剩余 E6 完整功能对照实施。先对照 Slint 的
File/Edit/View/Pane/Window/Help 和单例 Settings Tab。macOS 使用 AppKit 应用菜单
（Settings、About、Hide、Quit）并注册 Window/Help 菜单；Windows 将 Win32 菜单
挂到 eframe 的 HWND。两者固定使用 [muda 0.19.3](https://docs.rs/muda/0.19.3/muda/)，
关闭 GTK/default features。Linux 在现有 winit 的 X11/Wayland 窗口内使用
[egui MenuBar](https://github.com/emilk/egui/blob/0.37.0/crates/egui/src/containers/menu.rs)；
GTK 原生菜单需要 GTK 窗口宿主，本切片不宣称 Linux 原生菜单。各平台保留原生窗口边框。

单一 action 表驱动菜单、禁用状态和快捷键。AppKit 自有 accelerator；Windows/Linux
精确匹配 egui 非重复组合键，保留文本编辑器的复制粘贴事件。32 项有界事件队列唤醒 UI，
不持续轮询；菜单在窗口销毁前解除，Quit 使用正常清理链。Tab 支持稳定 ID、横向滚动、
拖动排序、中键/按钮关闭、右键操作及新建菜单。Settings 唯一，恢复身份/顺序，
不分配 transport，也不能进入终端分屏。

设置提供 System/Light/Dark、六种共享调色板、终端字号/行高/光标闪烁、新本地 Tab 的
shell、侧栏/Tab 宽度及三项可编辑应用快捷键。草稿预览只在 UI；Save 经 owner 的
有界版本/请求事务和既有 ConfigStore 私有原子写入，失败/过期保留草稿及保存值。
Revert/Discard 恢复已保存呈现；关闭脏 Tab/窗口提供 Save/Discard/Cancel，打开其它
工作区前先处理草稿。未开放设置域完整保留。语言/字体族、SFTP/媒体/X11 设置与多窗口
继续作为后续 E7/E8；原生菜单、布局和焦点仍需用户验收。

## FigCraft 参考定位

用户指定的本地 FigCraft checkout 仅只读参考；记录版本为
`a86897dc919b8ae7fb0142fe7e0034c10715f7d9`，依赖 egui/eframe 0.36、Rust 1.95。
不建立 path dependency，不复制品牌/字体资产，不把参考工程的工作区布局引入 AxSSH。
参考源码与目标 egui 版本存在差异，接口以最终锁定版本为准。

| 参考文件 | 借鉴内容 | AxSSH 的适配 |
| --- | --- | --- |
| `apps/figcraft/src/main.rs` | `eframe::App`、后台唤醒、仅在标题变化时发 viewport command | 保持薄启动层，使用既有日志和有界 Tokio runtime |
| `apps/figcraft/src/graphics.rs` | 后端选择与环境覆盖独立封装 | 根据 AxSSH 原生测试决定后端策略 |
| `crates/ui-egui/src/{lib,theme,widgets}.rs` | 薄 UI、集中 token、可访问控件 | 映射现有 ThemePalette 和快捷键/焦点规则 |
| `crates/ui-egui/src/windows.rs` | 稳定 viewport ID、各窗口局部 UI 状态、deferred viewport | 保留 Tab UUID 和任务生命周期，不复制全局可变状态交换方案 |
| `crates/ui-egui/src/render_worker.rs` | 展示结果合并、定向唤醒 | 使用有界通道；不照搬其无界 channel，不丢弃终端字节 |
| `crates/ui-egui/src/canvas.rs` | 自定义内容与 UI 控件分离、纹理生命周期 | 其 tiny-skia 画布纹理路径仅作备选，终端先直接绘制 |
| `crates/ui-egui/src/{i18n,a11y}.rs` 及行为测试 | 静态译文查找、控件语义、无窗口交互测试 | 保留 AxSSH PO/catalog 及上下文、复数和占位符校验 |

可追溯来源：[FigCraft 固定版本](https://github.com/storytold/figcraft/tree/a86897dc919b8ae7fb0142fe7e0034c10715f7d9)。
FigCraft 的截图验收和 macOS `process::exit` 做法不适用于 AxSSH；这里继续采用用户视觉验收和显式 shutdown。

## 实施阶段

阶段依次推进，每阶段形成可检查的变更；状态以 [实施记录](project-implementation-tracker/current.md) 为准。

| Step | 交付物 | 完成条件 |
| --- | --- | --- |
| E0 | 分支、双语计划、架构方向、参考与追踪 | 文档/skill/tracker 检查及基线 Cargo check 通过 |
| E1 | 版本/MSRV 决策、feature、实验入口、隔离存储、构建矩阵 | 原有 Slint 与仅 egui 构建分别通过；新入口可运行；更新工程约束及校验脚本 |
| E2 | 真实本地 PTY 的单 pane 终端 | 字符网格、IME、选择、粘贴、滚动、resize、同步输出和退出验证通过；收集首轮性能数据及用户视觉反馈 |
| E3 | 最小 controller facade、状态/呈现/cleanup 提取 | Slint 经新边界仍通过既有回归，egui 复用相同控制逻辑；无双份业务状态 |
| E4 | SSH/Telnet/Serial、信任/认证/重连 | loopback 安全与实际字节链路回归；凭据注入；真实服务器/设备单独验证 |
| E5 | 会话侧栏、分组/编辑器、Tab、最多 8 pane、工作区恢复 | 稳定 UUID、原子保存、拆分/关闭、恢复不绕过认证；焦点与键盘操作通过 |
| E6 | SFTP 两栏、图标、传输、编辑、拖放和 companion | 目录过期响应、冲突、取消、清理和独立 SSH/SFTP 生命周期回归通过 |
| E7 | Settings、主题、字体、快捷键、翻译、音频/媒体/X11 入口 | 既有设置语义/preview/save、全部 locale、权限和取消链路保持；无静默功能丢失 |
| E8 | 独立窗口、原生菜单/标题栏、DPI、可访问性和平台收口 | 五个 CI target 完整检查；各平台原生 GUI/输入/拖放；关闭重开资源稳定 |
| E9 | 功能对照、性能结论、默认切换及 Slint 清理 | 全部切换门禁满足，双语文档/发布/许可证同步，主干可用；删除旧 UI 后仍有独立校验 |

当前 E2 已有普通/粗体/斜体文字、ANSI 色、光标、拖选复制、双击词选/三击逻辑行选、基础 IME、
滚动、SGR 像素鼠标、resize 和 TextInput 辅助功能名称；真实 PTY 输入/尺寸/清理及无窗口回归可运行。
ASCII/框线 batch 在分数 DPI 下按模型列定位，每帧重新取得 egui 文本布局以避免字形图集重置后保留旧 UV。
滚轮批次由模型按当前 mouse/alternate-screen 模式编码，包括横向滚轮报告。
完整排字（ZWJ/几何/回退字宽）、keypad/AltGr、完整辅助树、原生 GUI 和性能比较仍未完成，
不把初版当作 E2 全部验收通过。

E2 为第一个可体验里程碑。E8 前跨平台构建持续跟进，不能把 Windows/Linux 问题全部留到收尾。
E2 只增加本地终端所需的最小实验适配，直接复用现有模型/PTY API；E3 将其接入共享控制层，
不保留两套输入编码、协议响应或 shutdown 规则。
E9 删除 Slint 前需完成 PO 键提取和静态表生成迁移；当前翻译工具从 `.slint` 提取文案，直接删除会清除有效译文。
`scripts/check_repository.py` 及其测试当前强制唯一 Slint build/type 入口，E1 改成分别校验两套入口，E9 改成最终 egui 边界。

## 验证与切换门禁

每阶段先运行相关行为回归，再运行仓库 fmt/check/严格 all-targets Clippy/test 和 diff 检查。
文档/规范变更运行 Markdown 链接、项目技能与 tracker validator；依赖和 build 变化验证 lockfile、feature 隔离和真实编译。
所有既有五个目标保留 target-specific check、Clippy 和 build；测试只在原生 runner 运行。

性能对比固定同机、release 构建、字体、DPI、网格和输入负载，使用合成内容；分别测 Slint GPU、Slint 软件和 egui。
场景覆盖空闲/闪烁、ASCII 与 CJK 持续输出、Vim/tmux、滚动、resize、8 pane 和多窗口。
每个固定负载至少重复 5 次，记录 CPU、RSS/GPU 内存、UI 帧时间 P50/P95、接收至绘制提交延迟、可用的功耗数据及关闭后资源。
绘制提交不等同屏幕显示或远端回显，分别标记指标。60 FPS 的 16.7 ms 是帧预算目标，不是已测结果。
明显退化必须说明并修复或形成明确取舍，不能以 framework 名称宣称收益。

切换前逐项完成：现有功能/设置/翻译对照表、确定的 renderer fallback 支持策略、loopback 安全与生命周期回归、
五目标构建门禁、原生 GUI 验证、旧配置读取/写入兼容，以及可审查的性能报告。
视觉验收由用户确认；代理不自行截图或检查应用截图作为验收证据。
真实 SSH、串口、麦克风和平台行为未验证时须明确记录，不能用编译通过代替。

## 回退与下一步

E9 前默认入口仍为 Slint，实验数据目录独立，失败时继续使用既有入口；不混用两套前端同时写同一个 workspace。
控制层提取按阶段保持原有测试，避免到最终切换时才发现行为变化。删除 Slint 依赖和 vendor 前核对使用图和独立许可证。

下一步实施 E6 SFTP 浏览/传输与独立 companion 生命周期，按需提取 E3R 编排。
E2V/E4V/E5V 原生验收继续开放；E9 最终切换默认入口并移除 Slint。
