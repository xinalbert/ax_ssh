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
新临时 SSH 默认 X11 Off，E7X 提供 Off/-X/-Y 选择与本机设置。E7A 增加默认关闭的连接级
麦克风选择，仅当前 SSH 及其重试沿用，不进入 profile/工作区，替换/主动断开清除。
保存后的 PCM 采样率/声道经 settings watch 用于新 worker；既有 SSH/audio owner 继续负责
信任、认证、录音需求、设备权限及有界取消/join，远端音源准备失败仍可使用 shell。
真实设备/权限验收继续开放，图片入口见 E7I。

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
原写 worker 继续执行远端冲突检查与原子替换。E7T 已接通编辑大小设置，默认 4 MiB，范围 1–64 MiB。
系统关联/实时编辑的原生验收和拖放对照继续保留。

## E6 有界拖放

实验界面只在可见 Remote files 区接收系统文件/目录，支持同 pane 本地到远端、远端到本地
复制。payload/intent 带 pane、连接和目录身份；单个保留 blocking 任务准备递归上传，
入 transport 前复验，关闭取消并 join。共享扫描保留相对路径，跳过链接与过滤名。
原生坐标查询留在 egui 适配；无坐标或旧坐标不回退活动 Tab，Wayland 要求当前帧指针事件。
选中行现在会把当前有界选择交给既有批量上传/下载意图；原生拖出 file promise、完整 SFTP 对照与原生拖放验收保留。

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

设置提供 System/Light/Dark、六种共享调色板及 Custom 的 Light/Dark 两套调色板、终端字号/行高/光标闪烁、新本地 Tab 的
shell、侧栏/Tab 宽度及三项可编辑应用快捷键。草稿预览只在 UI；Save 经 owner 的
有界版本/请求事务和既有 ConfigStore 私有原子写入，失败/过期保留草稿及保存值。
Revert/Discard 恢复已保存呈现；关闭脏 Tab/窗口提供 Save/Discard/Cancel，打开其它
工作区前先处理草稿。未开放设置域完整保留。其余界面翻译、媒体设置与多窗口
继续作为后续 E7/E8；原生菜单、布局和焦点仍需用户验收。

E7T 增加 Terminal 的历史行数、默认列/行数和备用屏滚轮 Home 策略，及 SFTP 的每连接并发、
单文件大小/带宽、外部编辑上限和名称过滤；复制/粘贴快捷键也可编辑并检查语义冲突。
草稿仅预览外观，行为设置在成功写盘后由 workspace 通过只保留最新值的 watch 通知 worker。
默认尺寸/shell 用于新终端；历史上限和滚轮策略用于现有终端，历史调整撤销旧选区/待消费复制。
SFTP 并发/大小/带宽在下次连接捕获，保存的 profile override 优先；过滤和编辑上限应用到新操作，
旧上传准备取消并 join。编辑任务在 blocking worker 接纳时捕获上限，回传仍按当前上限复验。
其余设置继续由 E7R 跟踪，OSC 52/Option 见 E7P，全局上传数见 E7U，本地打开见 E7O。

E7B 增加 Appearance 的文字亮度（60–120%，步长 5）与粗体亮色控件。
两前端共享既有 HSL 明度算法；egui 在反显后、隐藏前调整文字/下划线/删除线，
不改变 alpha、背景、选区或光标。粗体只提升低 ANSI 前景索引，粗体字体独立保留。
设置纳入行缓存键，静止终端也可即时预览/还原；保存失败保留草稿与旧保存值。
其它显示设置继续由 E7R 跟踪，GUI 另行验收。

E7R-Theme 增加 Custom 选择器和 Light/Dark 各 13 个字段的草稿编辑器。egui 使用稳定角色 ID
和有界十六进制输入，预览前把临时 `ThemeSettings` 交给 config 规范化，再用
`Context::set_visuals_of` 应用；十六进制解析、明暗方向、WCAG 对比度回退和持久化仍由 config
拥有。保存失败时两侧草稿和已保存值都保持不变。

E6R-Pause 为运行中的下载提供单项 Pause/Resume，并按类型化资格启用 Cancel。上传、
排队/冲突和终态均不新增暂停/继续能力；controller 持有等待状态、连接代次与 worker 回执，
UI 不通过状态文字推断行为，也不把入队视为执行完成。历史使用有界滚动区域保证控件可达。
暂停可能等待未完成读取，只能在当前连接内继续，不保存断点。固定信任 loopback 覆盖字节
保持、旧代次、暂停取消/关闭清理，有界历史/迟到事件及全语言控件回归另行覆盖。批量远端修改、
原生拖出与 GUI/真实服务器验收仍属 E6R；选中多项拖放已复用有界批量意图。

E6R-UploadSelect 接通每 pane 最多 64 个本地文件/目录根选择、Select All/Clear 和
Upload selected；选择绑定连接代次/本地目录/revision，旧选择清空，path 稳定 ID 与每帧准入避免
重复 pass 翻转或重复提交。准备中禁用再次上传，选择不改变现有拖放载荷。

owner 验证唯一本地 UTF-8 路径（每项 4096 字节、总 256 KiB），绑定私有目录快照与远端目录请求，
复用单个保留 blocking 准备任务。扫描前与请求构造后检查来源：普通文件复用打开后的指纹和
canonical parent 校验，目录根观测 no-follow 元数据/类型/长度/mtime/父目录归属，不冻结整棵树或
宣称消除文件系统竞态。目录元数据不进入 UI，local open 仍拒绝目录；本地/远端导航撤销准备，
策略或连接变化拒绝旧结果，退出协作取消并 join。沿用发现硬上限，实验端最终最多 64 文件，
准备失败整批拒绝，worker 接纳前不创建 history。文件共享一个 batch UUID，保留 native identity
复验和逐项冲突确认，不自动覆盖，不加任务类型/channel/持久化/信任路径。egui 已用阻断
Modal 对齐 Keep both/应用冲突到批次；选中多项拖放已复用既有选择与快照复验，批量修改/原生拖出仍保留。

E6R-Select 增加每 pane 最多 64 个远端路径选择、Select All/Clear 和 Download selected，
下载到已加载的本地目录。选择可保留排序/More 分页，消失/链接/类型变化项剔除，断开、连接代次
或目录请求变化清空；稳定 path ID 和独立每帧选择准入避免重复 pass 翻转，worker 意图仍每帧一次。
既有拖放载荷和单项修改目标不受选择影响。

单意图包含 1–64 唯一路径，总 UTF-8 文本最多 256 KiB；owner 在任何发送前复验远端请求和本地
目录/revision，失效/链接/过滤项跳过，逐根入队失败继续。最近一份摘要仅表示根项接纳（最多 64 项
错误，每项 512 字符），导航/断开清除，不据此改选择；完成/失败仍看传输行。沿用根行与发现取消，
首子文件复用 root UUID，空/全过滤目录经 DiscoveryFailed 结束且不创建本地目录；子文件保持实验
64 行/超额取消。共享 OpenSftpFile 入口在启动发现前也检查既有 512 active/opening/queued/
discovery 总槽预算，超限用既有失败事件，不新增 channel、task 类型、wire 协议、持久化或信任路径。
选中多项拖放已由 egui 接通，复用选择与快照复验；批量修改和原生拖出仍保留，冲突选择已与默认 worker 语义对齐。

E6R-Mutate 接通单项 Rename/Delete，冻结 path/type/size/mtime 并复验连接代次与目录请求。
单个保留结果槽覆盖排队、执行和未消费结果，Full/Closed 不占槽；owner 拒绝和原生写回执均按
精确 UUID 留结果，错误最多 512 字符。关闭确认框放弃结果，不回滚已接纳的写入；断开仍取消/
join 原 worker 并清槽。UI 只持模态草稿，失败保留，打开时阻断工作区/终端操作；拒绝链接和
非法名称，Delete 明确确认普通文件或空目录。沿用共享 worker 的 LSTAT/类型/目标不存在检查、
普通 RENAME 和非递归 REMOVE/RMDIR，这些观测不构成远端原子身份保证。成功仅刷新仍可见的
原目录请求，导航不回跳。远端文件批量修改/选中拖放、原生拖出和 GUI/真实服务器验收仍保留。

E6R-Batch 增加每 pane 的 UUID 传输选择、Select All/Clear 和批量 Pause/Resume/Cancel。
带 generation 的单条意图包含 1–64 个唯一 ID，owner 对每项当前资格复验，缺失/不适用跳过，
仅发送成功的项目进入等待状态，单项失败仍继续处理其它项。一份有界最新汇总只报告入队/跳过/
失败，不表示执行完成；最多 64 项失败，每项错误最多 512 字符。断开/替换清除汇总。
UI 剔除终态/消失选择，换代次或断开清空；标准 selectable button 每帧只更新一次选择，
重复 pass 不反复切换，worker 意图另有每帧准入。不增加队列、任务、信任、持久化或原生 worker
协议。上传暂停、跨连接继续仍不支持；远端文件批量修改/选中拖放与原生验收独立保留。

E7R-Transfer 接通保存会话/分组 JSON 交换与最后两项快捷键编辑。File > Import from
Clipboard 打开显式粘贴/确认的模态框；File > Export Selected to Clipboard 提供保存会话/分组
选择和 Copy，优先当前聚焦的保存 profile。默认键为 Cmd/Ctrl+Shift+I 与 Cmd/Ctrl+Shift+E。
解析/序列化/原子持久化复用 controller 有界 revision/回执与 join 的 blocking 任务，格式及
脱敏规则已从默认前端提取共享。导入增补会话/分组，不替换设置或活动 pane；无效输入、旧版本
和失败写盘保留配置/草稿。导出不修改配置，关闭/迟到/失焦结果不写剪贴板。沿用 256 KiB/
128 profile 协议上限，保留空组；UI 草稿限 256Ki 字符，提交按字节检查。实验界面须显式粘贴，
区别于默认前端自动读取剪贴板；文件对话框、定向分组导入及原生验收仍保留。

E7R-SFTP-Key 增加 View > SFTP 和第六项可编辑快捷键（默认 Ctrl+M），定位当前
聚焦的已保存 SSH pane 现有 companion 的 Remote path，不自动连接或列目录。本地、
Telnet、Serial、临时 SSH、Settings 与模态状态禁用；禁用时不吞终端按键。UUID 复验、
非 sizing pass 单次消费与同帧终端不抢焦点防止迟到/重复路由。八项持久化快捷键均参加
解析后别名/顺序冲突与保留键检查（含 Windows/Linux 的 Cmd/Meta 与 Ctrl 等价）；
会话导入/导出尚缺 controller action，暂不增加编辑入口。
保存成功后更新路由，预览/失败保存保留保存值。原生菜单/焦点验收仍待用户确认。

E7R-Shells 在自定义可执行路径旁增加已保存/发现的 shell 下拉框。启动时单个 blocking 任务
在 GUI 前 join，结果保留为 Context 局部元数据；沿用 32 项/256 字符限制，当前选择优先并保留
失效路径。打开设置/预览不改保存值，Save 将缓存合入提交副本，失败保留原草稿；后续新建本地
终端读取提交值，现有终端保持运行。控件复用已有 PO。旧折叠组名长度字段在当前 Slint/egui
均无 UI 消费，作为兼容数据等待紧凑侧栏契约，不增加无效控件；原生键盘/PATH 体验另行验收。

E7R-Icons 在 Appearance 增加本地 shell 图标选择（Automatic 与既有目录）及系统图标跟随主题
开关。草稿预览/还原即时更新 Local Tab 图标与系统图标颜色，Save 沿用原子事务，失败保留草稿。
Sidebar 使用已保存覆盖/有效缓存元数据，分屏 Tab 跟随聚焦 pane。共享本机身份服务在启动
blocking worker 读 Linux 来源，仅主来源缺失才回退。锁定 resvg 作为 egui 可选 build 依赖生成
有界 48px 像素，UI 回调不解 SVG；原色/mask 纹理保留 alpha，清缓存/销毁时释放。
本切片不新增远端探测、会话图标编辑器或原生标题栏集成；未知目标回退与原生/高 DPI 验收保留。

E7R-FPS 增加前台/后台刷新率控件（1–120 FPS，默认 60/4）。新建/恢复 pane 与成功保存后
配置 controller 的快照 deadline，预览/保存失败保留运行策略。最新值 focus watch 避免命令队列
过载、连接代次重置或隐藏清理失败导致呈现焦点滞后；每个准入 UI 帧刷新提示而不重复发送协议
报告。DEC1004 协议报告仍由代次校验保护。隐藏/非活动终端采用后台频率，
空闲 worker 不周期计时，错过期限不补发旧帧；同步输出 hold 保留。解析/协议响应和状态提示
独立于输出上限，实际原生帧率/焦点/性能仍待验收。

E7Q 增加 Appearance 的语义高亮开关与四类颜色草稿，复用已有 PO 译文和 config 规范化。
两前端共享既有的有界 ASCII 识别规则；终端 controller 发布每 run 的范围/类别，按当前源行
身份复用。Painter 不执行匹配，保留字符位置、程序样式和装饰。开关/颜色纳入行缓存键，
静止终端可即时预览/还原，保存失败保留旧保存值。识别仅限不超过 512 字节的合格 run，
不拼接样式、折行或 Unicode 文字。链接交互、剩余设置和原生 GUI 验收继续开放。

E7H 增加 Workspace 遮罩字符和已保存会话名下的遮罩地址，共享默认前端纯格式化。
用户名和 IPv4 第二段沿用既有规则，域名/IPv6/串口保持可见。预览规范化副本，
不改变 profile/保存值；保存失败保留草稿。完整悬停/辅助技术详情、原始 endpoint
搜索/编辑/连接继续可用，该设置只影响呈现。

E7L 已实现语言基础及应用菜单/设置翻译：继续使用 11 个 PO 译文目录和英文源语言，
原有 675 条键保留，新增 36 条形成 711 条；首批 egui 使用 84 条静态键。Rust 字面量标记
与 Slint/搜索源键合并，同一工具生成并校验 egui 常量表，context/格式和译者注释规则保留。
Language 提供系统/全部已有语言，保存规范 locale；成功写盘后才切换，预览和失败不影响当前语言。
系统 locale 在 GUI 前取得，Context 本地查找无文件读取；菜单、设置及 Settings Tab/窗口标题更新，
原生菜单保留 action ID/accelerator/Window/Help 角色。后续翻译和原生验收仍待 E7R。

E7F 增加独立应用/终端字体选择，共用自带资源目录和最多 256 项的选项，保留不可用的选中名称。
字体发现和必需回退在 GUI 启动前完成；后续选择通过单个保留的 blocking worker 加载，
请求/结果只保留最新值，每文件 24 MiB、每请求 96 MiB，支持取消及退出 join。
预览、保存、还原和放弃共用有效设置路径；缺失字体或实测 Latin 字宽不等时提示并回退
JetBrains Mono，不改写选择。四种终端样式别名支持静态/可变字面并保留 CJK/符号回退。
应用字体后刷新实测字符格、行缓存、选区和 PTY 尺寸；原生排字/IME 与性能仍属 E2V。

E7W 覆盖终端连接字段/信任/认证提示、会话与分组、Tab/pane 操作和工作区文件窗口的全部
11 个目录。复用已有消息，每个语言新增 30 条，达到 741 条 PO 和 145 条 egui 源键。
稳定 ID 保留语言切换时的连接展开状态和窗口身份；用户数据、指纹、路径、协议标识及技术
状态/错误原样保留，精确 prompt token 和 confirmable gate 继续生效。
其余设置和媒体仍属 E7R，X11 入口见 E7X；原生布局/键盘验收单独保留。

E7S 增加 SFTP 目录/传输/冲突/外部编辑固定文案，复用已有消息并每目录新增 15 条，
共 756 条 PO、167 条 egui 键。SFTP 共用终端信任/认证提示，明确显示目标和指纹后
才提供仅本次信任确认，被阻止密钥没有确认按钮。提示/连接代次改变会清除旧凭据，
终端与 SFTP 密码控件作用域独立。按钮每帧只准入一次、跳过 sizing 并反馈发送错误；
传输 UUID 和文件/路径/状态原文稳定，transport 信任、冲突规则和清理保持原 owner。

E7M 实现保存后生效的本地选择优先、选中自动复制和右键复制/粘贴。Shift 强制本地选择，
Alt/Option 覆盖本地优先；本地拖选接管手势，取消点击不转发，未形成选择的单击仍可到达 TUI。
复制结果携带连接/视口身份；应用发起的原生粘贴使用短期目标票据，在控件收到内容前检查，
重复 pass 也保留相同判定。普通系统粘贴继续跟随聚焦控件。

E7P 增加保存后生效的 Option/Meta 和 OSC 52 设置。macOS 原生窗口仅在终端拥有焦点时
保留 Alt 原始字符，编辑器/设置继续原生 Option 文字，IME commit 不加 Meta 前缀。
OSC 52 默认关闭；开启后的写入和逐次确认读取均限制 64 KiB，controller 持有精确请求 ID、
formatter、20 秒期限及权限/连接撤销。保留的原生读取服务使用一个 blocking job 和有界
请求/结果槽，保留空文本/CRLF；焦点、视口或导航改变时丢弃迟到结果，普通粘贴不能满足
远端读取。实际 Option/IME/系统剪贴板验收仍开放。

E7U 展示既有全局上传上限（1–32），在工作区成功加载及 Save 后应用。
保存失败、旧编辑和外观预览保留旧限额。所有 SFTP 连接共用进程级 limiter，服务器覆盖
不能替代它；降低上限不撤销运行中的名额，排队/新上传按可用容量执行，无需重连。
其余媒体及原生 SFTP 验收继续独立保留；X11 入口见 E7X。

E7O 共享默认前端目录/文件校验，给本地普通文件增加 Open 及保存后的快照上限。
一个可取消并 join 的 blocking job 从已验证句柄复制到独立实验缓存，再调用系统 opener；
目录 revision、身份、大小和撤销检查拒绝过期/替换目标。副本不修改源文件，编辑不自动回写。
默认 512 MiB、0 取消字节限制、128 文件和过期清理均复用既有缓存引擎。
本地目录导航/设置/断开/关闭撤销未启动任务；关联应用和 GUI 仍需手动验收。

E7X 接通 SSH Off/-X/-Y 选择与本机 provider/自定义路径/按需启动/显式本地无授权兼容设置。
新草稿默认 Off，保存会话保留明确选择；预览和保存失败不改变 worker 配置，新 SSH worker
捕获已保存设置，已有连接待重连应用。已知安装位置在窗口创建前后台读取并 join，UI 不做 I/O。
复用原有 worker 的认证后准备、cookie 验证、受限 -X 授权、有界 relay 与关闭清理，X11 请求被拒
仍可使用 shell。麦克风入口见 E7A；真实设备/X server/GUI 验收继续保留，不据此切换默认前端。

E7I 接通已连接 Local/SSH 的剪贴板图片，复用 media 与同连接 SFTP。单个后台 job 反预乘像素并
编码有界 PNG，保留 join；controller 持有准备任务/文件，UI 票据复核原连接代次、视口和可见
终端焦点后才插入转义路径，不自动回车。新交互、Cancel、连接变化撤销结果，退出等待文件任务。
上限为 4096²/16 MiB/32 文件，单次转换、每终端单次准备。egui 同时有文本/图片时优先文本；
Telnet/Serial 拒绝图片，OSC 52 仍只传文本。无需文件选择器或新依赖；原生剪贴板及视觉验收
继续开放，断网后远端清理尽力而为。
共享 setup 在启用 X11 前等待服务端明确 Success/Failure，期限 15 秒，最多处理 64 消息并保留
64 KiB 早到输出；断开可取消等待，仅请求入队不会启用 dispatcher。

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
E7L 已加入 egui 源键和静态表；E9 删除 Slint 前仍须迁移其余只在 `.slint` 中引用的文案，否则会清除尚需保留的译文。
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
