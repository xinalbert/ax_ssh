# 当前项目实施记录

## 当前目标

- 目标 ID：20261022-egui-migration
- 目标：按独立分支计划完整用 egui 替代 Slint，保留既有 Rust 领域、transport 和安全行为；过渡期默认 Slint，E9 移除旧 UI/依赖/专用构建路径。
- 交付物：`codex/egui-ui-migration`、双语计划、egui-ui binary、UI 无关终端/连接 controller、Painter/IME 与临时 Local/SSH/Telnet/Serial、信任/认证及重连、会话/分组、Tab/八 pane 与独立工作区恢复、构建/行为测试及阶段追踪。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：按用户优先级实施 E7 设置和 E8 原生菜单首切片，完善 Tab 交互；先对照 Slint，再核对上游跨平台菜单标准；E6 核心实现已完成，原生交互验收保留。
- 不在本轮范围内：E2V/E4V/E5V 原生验收、E6 完整拖放/交互对照、E7 完整功能及 E8 多窗口、E9、正式切换、安装/推送/发布、真实凭据存储迁移、参考工程写入、自主应用截图验收。
- 后续实施范围：E3R 后续共享应用状态及工作区/传输/设置/平台迁移；E2 完整排字、原生输入、GUI 和性能验收作为 E2V 保留。
- 所有权与安全：前端只消费快照并发出意图；控制层保留解析、业务、持久化、取消和清理，SSH 继续默认拒绝，凭据短期化、队列/缓存有界。`third_package/axshell` 保持只读参考边界。

## 当前状态

- 阶段：实施中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| E0 | completed | 分支、双语计划、架构决策方向、FigCraft 定位及追踪 | Markdown/入口、skill、tracker、基线 Cargo check、diff 已通过 | 本轮交付；基线 master `15404e1` |
| E1 | completed | 版本/MSRV、renderer 策略、feature、实验入口和隔离目录 | 最低工具链两前端、组合/egui-only 图、strict 与本机链接通过 | 0.37 + Rust 1.95；其它平台交既有原生 CI |
| E2I | completed | 真实本地 PTY 单 pane 原型实现 | 默认 909、egui 406+15 测试、本机严格门禁、组合/MSRV 和构建通过 | E2 分为已交付实现和独立待验收项 |
| E2V | pending | E2 原生输入/完整字体/性能验收 | 用户视觉确认、原生 IME/keypad/AltGr、可访问性和性能数据 | 不以用户要求下一项代替完整验收 |
| E3 | completed | 独立终端所需的最小共享 controller、状态/呈现/cleanup | 前三切片通过两前端完整回归、本机构建及边界校验 | 最小基础已具备；其余状态提取随 E4/E5/E7 对应功能推进 |
| E3R | pending | 其余工作区/安全提示/连接/设置编排提取 | 随对应功能保留原有回归，无双份业务状态 | 终端/连接与 pane 树已共用；实验 workspace owner 已落地，其余业务编排随 E6/E7 |
| E4I | completed | 临时 SSH/Telnet/Serial 与信任/认证/有界重连实现 | 固定 loopback peer/凭据、真实字节/resize/取消/替换和完整回归通过 | 复用既有 worker；仅本次信任，未知密钥认证前拒绝；连接尝试不保存凭据或仅本次信任 |
| E4V | pending | E4 原生 GUI、SSH-agent/服务器/串口验收 | 真实服务器/agent/加密私钥与串口设备、目标平台 GUI/权限/断开 | 不以 loopback 或编译代替原生验收 |
| E5I | completed | 会话/分组与编辑、Tab/八 pane、独立工作区保存/打开/恢复 | 9 项 workspace、共享 pane、真实 PTY 焦点/禁用输入、多 pass、两前端完整门禁通过 | UUID/版本/请求隔离，恢复 idle，32 活动/关闭终端和 64 命令/回执上限 |
| E5V | pending | E5 原生布局/键盘/恢复与完整功能对照 | 用户视觉、原生拖动/焦点/文件路径、真实多连接关闭恢复 | 不保存输出历史/临时目标/草稿；原生文件选择器/最近菜单、凭据存储和完整旧格式对照待后续界面 |
| E6 | completed | SFTP companion、目录/传输/图标、共享外部编辑与自动/手动回传 | 字节/冲突/迟到回执、任务清理、UI 去重及双前端门禁 | 编辑/栏内拖放/OS 拖入已实现；原生拖出/选择批次/完整交互与原生验收保留 |
| E7 | completed | 设置/主题/字体/快捷键/翻译/媒体/X11 UI | preview/save、全部 locale、权限、取消及功能对照 | 先迁移 PO 键提取再删除 Slint |
| E8 | in_progress | 多窗口、菜单/标题栏、DPI、可访问性和原生适配 | 五目标 check/Clippy/build，原生 GUI、资源生命周期 | 平台编译从 E1 持续验证 |
| E9 | pending | 功能/性能报告、默认切换与旧依赖清理 | renderer 支持策略、全部回归、用户验收、配置兼容、发布/许可 | E9 前默认 Slint；无预设性能收益 |

## 已完成

- 从干净的 `master`（`15404e1aab2963ce452c3da1f634cc6260b1f819`）创建并切换到 `codex/egui-ui-migration`。
- 读取项目规范、双语架构入口、模块及工具链事实；确认现有 47 份 Slint、16,353 行，UI 之外的库领域未直接使用 Slint 类型。
- 初始评估把应用状态细化为局部提取：媒体/外部编辑仍留默认应用，几何为路径字符串；Linux icon decoder 的 Slint 依赖已在 E6 图标切片移除。
- 核对上游 egui 0.37 的 Rust 1.95、系统字体能力与 eframe 后端；0.35 声明 Rust 1.92，后续需验证依赖闭包。
- 按用户补充，只读检查 FigCraft 固定版本 `a86897d` 的 App、主题/控件、窗口、render worker、canvas 和翻译；在双语计划中明确借鉴范围、有界适配和独立清理。
- 写入 D1 方向、E0–E9 依赖顺序、feature/build/翻译校验迁移、快照丢帧正确性、重复 pass 操作去重及阶段验收；旧发布目标保留在本月追加历史。
- 加入默认 slint-ui/可选 egui-ui、独立 ax_ssh_egui binary 和 cfg-gated Slint build；包 MSRV/五平台 CI/README 同步为 1.95，egui/eframe 固定 0.37，默认 Slint 1.18.1 未升级。
- 实验 controller 持有 PTY/parser task、64 命令有界队列、完整可见快照、16ms 展示合并及显式取消/join；不接触保存的会话或凭据，日志目录独立。UI 绘制可见行并按 revision 缓存，基础输入/IME/选择/颜色/滚动/鼠标与尺寸链路完成。
- 真实 `/bin/sh` 输入/resize/清理、过载/失败、跳过快照行 revision、egui 无窗口文字与预编辑/提交、几何/颜色回归已补充。参考 FigCraft 只用于模式核对，无依赖/资产复制。
- 已启动 `target/debug/ax_ssh_egui`；日志未报告错误，进程与其子 `/bin/zsh` 持续存活。未截图或判断视觉正确，窗口留给用户体验。
- 根据用户截图继续 E2：实现 SGR 像素坐标、轮滚批量上限、motion 丢弃保留可靠队列、按住鼠标释放兜底、resize 去重、双击词选/三击逻辑行选和 TextInput 辅助名称。普通/IME 文本同帧或相邻帧时以 IME commit 去重；窗口字体尺寸变动和选择结果均使用精确请求身份处理。截图中连续提示符未作为错误结论，必须由输入字节回归判断。
- 对照 egui-winit、winit 与 Alacritty 的 IME 路径：preedit 只作本地候选显示，commit 是唯一 PTY 写入；egui 输出 Terminal purpose 和光标矩形，由 eframe/egui-winit 启用平台 IME 并更新候选窗锚点。新增真实 PTY 回归覆盖同帧 `Text`、`Ime::Commit` 与 Enter，确认中文只到达 shell 一次。
- 按用户后续截图继续 E2：ASCII/框线按模型列独立布局，覆盖四种 DPI/字形样式；每帧向 egui 重新取得 galley，避免其字形图集回收后缓存旧 UV。滚轮统一经 model 的协议编码，横向轮与 alternate-scroll 共用有界鼠标事务。
- E3 首切片已接通两套前端：公共 controller 不再仅由 egui feature 编译；共用有界鼠标编码、`TerminalOutput`、`TerminalFrames` 和 `WorkerShutdowns`。Slint 保留标题、剪贴板连接/权限代次和调度策略，egui 继续禁用标题/OSC 52 平台动作。清理任务在移除 Tab 时登记，退出统一排空；同步输出与待消费脏行合并规则沿用既有实现。
- 通用滚轮/Home 与 cleanup 回归迁至 controller；修复提取后的常量/测试导入，并将 egui 像素鼠标测试改为单次普通按下、拒绝重复普通按下，符合既有输入入口契约。共享层 15 项聚焦回归和两前端完整门禁通过。
- E3 第二切片已接线：`TerminalViewport` 统一拥有视口事务、非负选区代次与帧准入；两前端的 resize/scroll/return-to-bottom 共用状态规则，`TerminalFrames` 收为内部实现。普通输出及单纯展示失效保留选区身份，字符尺寸/历史位置改变才推进代次并丢弃旧帧。清除 Slint/egui 中重复的代次和失效状态。
- 新增四项共享视口回归，以及真实 PTY 过期 Copy/selection 和 resize 拒绝重试回归。后者定位并修复 Local PTY/controller 在接纳失败前缓存 resize 的问题；失败不提前改变几何或选区身份，重复请求继续返回真实结果。共享层 21 项聚焦回归、默认 411+506=917 项和 egui 420+15=435 项完整回归及两前端构建通过。
- E3 第三切片提取 `ConnectionIdentity` 与 `ReconnectState`：协议/profile/attempt 身份及五次重连预算/代次由 controller 持有，应用层消费接口，清除重复字段。精确 Tab 查找、主机信任/认证、安全提示、worker 所有权、SSH attempt 变化时的延迟重置和媒体/剪贴板撤销仍由应用层保持。
- 新增 7 项纯连接状态回归，覆盖协议隔离、同 profile 重复连接与 SFTP companion 独立、重连在途/worker gate、上限、代次取消及回绕；99 项状态和 19 项连接相关回归通过。默认 418+506=924 项、egui 427+15=442 项完整回归、严格 Clippy/组合检查及两前端可执行文件构建通过。E4 的后续实现见下一条。

- E4I 已落地：`TerminalSession` 复用现有 Local/SSH/Telnet/Serial worker，egui 提供临时连接草稿、密码/私钥/SSH agent、信任/认证提示、断开和手动重试。秘密使用最多 16 KiB 的 Zeroizing 负载，不进入快照/日志/持久化，密码编辑 undo 清理；确认未知密钥须重新输入凭据。系统 known_hosts 仅读取，仅本次信任不写入；变更/撤销及 probe 后换钥持续阻断。临时 SSH 禁用 X11/麦克风初始化，默认 Slint 行为保留。
- 可选自动重连复用五次预算，控制器持有 1/2/4/8/16 秒期限；SSH 密码/口令重新索取，agent/直连可无秘密重试。替换先丢弃旧 receiver 并 join worker，重置完整帧及选区，连接代次拒绝旧输入/复制/提示。八项真实远端链路回归覆盖认证前拒绝、精确 prompt、认证重试、换钥、取消 probe、Telnet/Serial 失败、重连和期限撤销；另补视口重置、草稿验证/秘密失败清除/undo 回归。

- E5I 已完成：两前端共用从 `app/panes.rs` 提取的 PaneTree；实验 workspace owner 统一持有 Tab 顺序、逐 pane 终端/关闭清理、目录版本与精确请求回执、独立 ConfigStore 和两秒变化 checkpoint。保存会话/分组复用 domain 校验及原子 blocking 写入，写入成功后发布，失败/旧编辑保留草稿；资源上限包含关闭中的任务，未消费回执有界。UI 增加搜索/分组/编辑/复制/删除、Tab 排序、分屏/分隔拖动/方向焦点和路径导入/导出。
- E5 恢复持久 UUID/根 Tab 标题、顺序、比例和活动焦点；全部终端恢复 idle，无自动 process/network/device/认证动作。导入先校验再取消/join 旧工作区，损坏启动文件保留。快照不保存终端输出、凭据、临时目标、草稿或仅本次信任；源配置/凭据与默认应用隔离。pane 控件/密码 ID 独立，禁用编辑器不把文字发给终端，Tab 切换撤销旧焦点和远端按住状态。增加 9 项 workspace、共享树非法恢复和 3 项 egui 回归；6 项原有 pane 回归从 binary 移入共享 library。

- E6 egui 切片已扩展：远程 pane 创建独立 `SftpSession` companion，使用既有 `SshSessionHandle::spawn_sftp`，不共享终端 russh handle；连接先执行 host-key probe，未知 key 只能在明确 prompt 下本次确认，changed/revoked 仍拒绝，认证重试只接收新的 zeroizing secret。SFTP 远端目录页按当前 request ID 接纳，旧页丢弃；本地目录读取在 companion 的 blocking worker 中执行，仅发布有界 `LocalEntryView`，egui 可浏览子目录并选择正规文件进入上传表单。下载、上传、进度、取消和冲突决策事件有界并由 companion 维护快照。上传源在入队前复验正规文件与大小上限，worker 继续负责冲突探测和原子发布。关闭 pane/tab 时终端和 companion 分别 cancel/join，workspace 退出统一 drain cleanup。

- E6 图标切片：将原平台 provider/cache 和九项回归移入 `src/file_icons.rs`，默认应用保留 re-export adapter；两前端使用同一 bounded key/fallback/platform-theme 缓存。Linux SVG 改用固定 resvg 0.48.1、禁用默认 features 与外部/内嵌 image resolver，UTF-8/2 MiB/512px 源限制，输出 24×24 straight RGBA。companion 给 remote/local 条目发布类型 key 和最多 64 个共享图标，持有/join 单个 blocking 预热任务，完成后刷新当前目录。egui 每 pane 最多 64 个纹理，复用未变像素、隐藏/关闭/最小化释放；未解析类型保留通用 fallback。补缓存 key 复验、SVG 资源拒绝/alpha、纹理复用/上限、真实 local facade 和 probe 取消安全/关闭回归。host-key receiver 在其它事件唤醒间保留，probe task 取消/join，秘密在关闭时清除。

- E6 编辑切片：共享 `remote_edit` 状态机/私有副本与 `file_identity`，默认适配保留动态设置、watcher 和既有行为；实验 companion 增加独立 cache、最多八编辑/128 副本、单 blocking job，500ms 稳定文件观察、自动/手动回传、停止与重开。任务带 generation/edit UUID，旧副本完成只清理不启动应用，停止/断开撤销 launch permit，退出 join 本地工作；已发布副本保留。正文不进快照/日志/持久工作区，SFTP 原有 size/mtime/permission 与 atomic overwrite 冲突保护继续生效。egui 控件按 frame 去重并跳过 sizing，使用默认 4 MiB，E7 再迁移设置。13 项既有编辑回归迁共享层，新增真实 loopback、opener 注入、冲突、停止迟到下载、待处理副本清理及 UI 去重测试。

- E6 拖放切片：本地递归上传发现提取到共享 `local_upload`，默认应用只保留薄适配；egui 增加同 pane 双向栏内复制和系统文件/目录拖入远端区域。载荷携带 pane/连接代次/目录 revision，裁切区域和当前原生指针共同选目标，无坐标拒绝。上传准备单 blocking task，扫描前后复验目标连接/request/目录及内部来源快照；可取消，退出 join，准备结果不因导航/断开而发布。根/实验文件批次最多 64，保留共享 scan/depth/path 上限、默认过滤和 worker 大小/identity 检查，正文不在 UI 读取。补多 pass/禁用/区域外/跨 pane 回归、真实递归上传与拖回本地、导航取消丢弃及关闭 join。修复传输 progress 状态和冲突 marker 清理，保持历史有界。

## 验证

- 已完成：E6 拖放切片后默认 454 library + 476 binary = 930 项、egui-only 498 library + 28 binary = 526 项完整测试通过；默认/egui locked/offline check、严格 all-target Clippy、build、组合 all-features/all-targets check，以及两套 macOS ARM64 显式 target check/Clippy/build 全部通过。递归上传/下载 loopback、导航/取消丢弃、准备任务关闭 join、内部载荷身份与原生区域/多 pass/禁用回归通过。fmt、27 Python、675×11 翻译、图标、56 Markdown、skill、tracker、diff 通过；egui-only 依赖图继续不含 Slint/其 vendor renderer。
- 未完成：E2V 原生 GUI/IME/keypad/AltGr/完整排字/鼠标、可访问性和性能；E4V 真实服务器/SSH-agent/私钥/串口；E5V 原生分屏/焦点/恢复及完整旧功能对照。E3R 其余编排与 E6–E9、原生文件选择器/最近菜单/凭据存储/完整历史恢复仍待后续界面。其它平台交原生 CI，不以本机通过推定。未自主截图或断言视觉正确。

- E7 设置首切片与 E8 菜单/Tab 首切片已实现：AppSettings 由 Workspace owner 以有界 CatalogAction 保存；设置单例 Tab 可预览、保存、还原和关闭确认，草稿不进入快照；Tab 支持拖动排序、中键关闭、稳定 ID 和多 pass 去重。
- 菜单按 Slint File/Edit/View/Pane/Window/Help action 表统一；macOS 接 AppKit、Windows 接 Win32 muda 0.19.3，Linux/X11/Wayland 在现有 winit 窗口中用 egui MenuBar；GTK 未引入。


## 风险与阻塞

- egui WGPU 暂无等价 CPU/damage fallback，保持默认 Slint 作为当前受支持替代；迁移最终切换前需完整 renderer 支持决策。
- 本机已安装 Windows target 不代表拥有对应 SDK/linker；其它平台最终依赖原生 CI，不能以本机编译推定通过。
- 翻译提取和仓库入口检查绑定 Slint；移除前必须迁移并保持安全/模块校验，不得直接删除门禁。
- 终端字体/IME、多窗口平台集成、软件渲染和性能为主要不确定性，E2 优先验证；用户负责视觉验收。

## 下一步

- 完成 E8 菜单首切片后，继续 E6 原生拖出/选择批次；再推进 E7 剩余设置域、E8 多窗口/DPI/可访问性。E2V/E4V/E5V 待原生验收，最终 E9 移除 Slint。
- 完整追踪、环境快速核对，本轮未新增外部调研，无多 agent/Worker；未推送、提交或发布。

## 最后更新时间

- 2026-10-24 11:15 +0800
