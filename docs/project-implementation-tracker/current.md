# 当前项目实施记录

## 当前目标

- 目标 ID：20260821-terminal-render-performance
- 目标：降低高吞吐终端输出期间的主线程 CPU 占用，优先消除重复快照、重复渲染和无差别工作区刷新，再逐步引入有界批处理、增量渲染缓存和可配置的 focused/unfocused FPS 呈现上限。
- 交付物：可复现 release 基线、单一路径 Terminal pane 呈现、按脏终端调度的刷新门控、稳定 Slint pane/model 通知、聚焦自适应/未聚焦低频双呈现策略、增量终端渲染缓存、可配置的紧凑终端节点树与静态行 layer cache，以及完整离线门禁和前后采样记录。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/{state,window_router,view,terminal_bridge,terminal_presentation,connection,connection_monitor,settings_bridge}.rs` 及其现代子模块、`src/config/settings.rs`、`src/{terminal,terminal_dimensions}.rs`、`ui/{app,settings,settings/appearance,settings/terminal,workspace-shell,terminal-pane,components/terminal-grid,theme}.slint`、翻译目录、双语架构/使用说明和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：SSH host-key/认证/凭据契约、russh transport 选择、UI framework/renderer 依赖升级、专用 GPU surface、参考工程代码或构建耦合，以及未经用户提供截图的 GUI 视觉结论。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| PERF1 | completed | 当前 release/debug 环境、采样热点和固定负载基线 | manifest/toolchain 检查、sample 归因、基线命令可复现 | 现有 421 ms sample 用于定位，不作为最终优化比例。 |
| PERF2 | completed | 删除未消费的 active-terminal DTO/render，并确保每个可见 pane 每轮最多生成一次 snapshot | focused Rust 回归、Cargo/Slint check | 不改变 pane UUID、选区、IME 或 worker 所有权。 |
| PERF3 | completed | 刷新门控携带脏 terminal ID，并只对未包含在已取 snapshot 中的后续请求补排 | focused 并发/状态回归、延迟 tracing 审阅 | 保持 bounded event-loop 调度和最新状态读取。 |
| PERF4 | completed | 未变化 pane/divider 不发送父行通知；terminal-only 更新跳过 SFTP 和无关 workspace 属性 | model identity/notification 回归、Cargo check | 结构变化仍回退到完整窗口刷新。 |
| PERF5 | completed | Local/Serial 立即解析输出，但每 terminal 最多每 16 ms 发布一次 UI 呈现请求 | Tokio paused-time/focused worker 回归 | 协议应答、错误、退出和 shutdown 不等待展示 timer。 |
| PERF6 | completed | Terminal dirty generation、稳定行 identity、增量 render/语义高亮缓存和更少 snapshot 字符串分配 | terminal/render focused tests、Cargo check | `TermDamage`、resize/offset/theme/highlight 失效已审查；行 revision 使用 64-bit。 |
| PERF7 | completed | 完整离线门禁、边界审计和可采样的 release 候选 | fmt、check、Clippy、test、translation、Markdown/tracker、release build、`git diff --check` | release 构建复用本机同 target/feature 的 rust-skia 缓存。 |
| PERF8 | completed | 优化前后 debug sample 的结构性对照与 10 秒稳定性复核 | 相同 debug 二进制、renderer、窗口尺寸、7 个可见 Local pane 和持续输出负载 | 只用于判定热点转移与优化方向，不声明总 CPU 降幅。 |
| PERF9 | completed | 优化后 release 10 秒 sample 与 CPU meter 基线 | 已验证 release 二进制，相同 renderer、窗口尺寸、7 个可见 Local pane 和持续输出负载 | release 已 strip Rust 符号；系统级 DisplayLink/Metal 与线程阻塞归因仍可靠。 |
| PERF10 | completed | Local/Serial 持续输出的 33 ms UI 呈现候选 | paused-time 节拍回归、完整 Cargo/Slint 门禁、release build | 首个脏输出仍立即呈现；parser、协议应答和终止路径不延迟。 |
| PERF11 | completed | 聚焦 16/33/50 ms 自适应与可见未聚焦低频双呈现策略 | pure 状态机、焦点路由和 no-output 回归 | 首次脏输出立即呈现；隐藏 Tab 不进入 UI，parser/协议/终止路径不延迟。 |
| PERF12 | completed | Local、Serial、SSH、Telnet monitor 统一接入呈现状态 | 四协议编译、状态机/路由 focused tests | 保留 SSH/Telnet 的 16 ms/16 KiB transport batching，只统一 UI publication。 |
| PERF13 | completed | 双语架构、项目地图、完整离线门禁和 release 候选 | fmt、check、Clippy、test、Markdown/tracker、release build | 不改 renderer 依赖、配置 schema、trust 或 credential 边界。 |
| PERF14 | in_progress | 同一 release 的 Software/GPU/行缓存 10 秒 A/B 和目标平台 GUI/真实 transport 验收 | 相同窗口、7 pane、负载和 sample/CPU meter | 依次对照旧 item 树、紧凑节点和紧凑节点+静态行缓存。 |
| PERF15 | completed | 两项独立、向后兼容的终端渲染性能设置及 Settings 预览/持久化链路 | config normalization/serde 与 Rust/Slint 映射回归 | 紧凑节点默认开启；静态行 layer cache 默认关闭。 |
| PERF16 | completed | 可切换的紧凑 run 节点树与只覆盖静态行内容的 layer cache | Slint 编译、model identity 与终端交互回归 | 光标、选区、IME 和目标高亮保持在缓存层外。 |
| PERF17 | completed | 双语契约、翻译、完整离线门禁和新 release A/B 候选 | fmt、check、Clippy、test、translation、Markdown/tracker、release build、`git diff --check` | 不升级 renderer 或引入参考工程耦合。 |
| PERF18 | completed | 把两个终端渲染性能开关从 Terminal 页移到 Appearance 页 RENDERING 分组 | Slint 编译、Settings 搜索路由回归、完整 Cargo 门禁、翻译/链接/`git diff --check` | 配置 schema、字段名、默认值和持久化不变；只调整展示分组与搜索归属。 |
| PERF19 | completed | 将聚焦/可见未聚焦终端刷新周期改为可配置 FPS，并接入设置预览、持久化和热更新 | FPS 归一化/serde、Slint callback、policy watch、四协议呈现状态机和完整门禁 | 范围 1-120 FPS；默认聚焦 60、未聚焦 4；聚焦 16/33/50 ms 自适应仍作为上限策略。 |

## 已完成

- 已分析用户提供的 421 个 1 ms 样本：主线程 DisplayLink/Slint/Skia/Metal 渲染为主要热点，Tokio、PTY reader 和日志线程大部分时间阻塞。
- 已确认采样进程为 `target/debug/ax_ssh`，空闲时无持续忙循环；高占用与终端输出/工作区更新相关，并受 debug 构建放大。
- 已定位活动终端重复 snapshot/render、全窗口 pane 重建、无条件父模型通知、刷新 follow-up、Local/Serial 逐 read 呈现和全行语义高亮扫描等优化点。
- 已完成 Rust/Slint 架构技能、实施跟踪规则和项目环境快速扫描；本机 Rust/Cargo 1.97.1、rustfmt 1.9.0、Clippy 0.1.97 可用，manifest 保持 Rust 2024、MSRV 1.92.0 和 Slint 1.17.1。
- 已固定 release 对照条件：相同 macOS renderer、窗口尺寸、7 个可见 Local pane 和持续输出命令，各采样 10 秒；debug sample 只作为热点定位输入。
- 已删除 Slint 未消费的 `WorkspaceViewState.terminal` 及其 AppWindow 扁平 render 属性；活动页元数据 snapshot 不再构造终端网格，每个可见 pane 仍生成一次完整 snapshot。
- 已用 `UiRefreshBatch` 请求代次替代单 `AtomicBool`：snapshot 前的请求合并进本轮，snapshot 后的请求才补排；full refresh 覆盖旧 dirty 集合，terminal-only refresh 只构造当前可见 pane tree 中命中的 UUID。
- 已让 nested render-line/cursor model 直接发布变化；浅层 pane/divider 不变时跳过父 `set_row_data`，普通终端 snapshot 不再构造或应用 SFTP rows。
- 已将 Local/Serial 的 parser/协议应答与 UI 呈现解耦：首个输出可立即发布，持续输出按 16 ms、MissedTickBehavior::Skip 合并；错误、断开和 shutdown 保持即时路径。
- 已让 `TerminalModel::snapshot()` 消费上游 `TermDamage` 并复用稳定 `Arc<TerminalStyledLine>`；只有 damage、尺寸或 display offset 需要时才重建有界可见行，删除未消费的扁平 `TerminalSnapshot.text`，并移除逐 cell 临时字符串分配。
- 已让 renderer 按 64-bit 行 revision 和覆盖色表/主题/亮度/粗体/语义设置的 64-bit key 复用 Slint 行；语义高亮改为单次 token 扫描，同时保留 `timed out` 短语和既有优先级。长期运行的行 revision 已从 32-bit 提升为 64-bit，并拆成两个 Slint `int` 防止低位回绕误命中。
- 已生成 `target/release/ax_ssh`（36,360,352 bytes，SHA-256 `45506a8b2c1600d7e1a23e0e32d3cc977a7123f657a103f6df37db4e58a10791`），作为 PERF8 的优化后采样候选。
- 已复核用户提供的 `12:54` sample 和同 PID 的额外 10 秒 sample：PID 89908 仍为 `target/debug/ax_ssh`，与 `11:01` 基线均只能作 debug 结构性对照。按主线程样本归一化，旧样本、`12:54` 短样本和新 10 秒样本的空闲占比分别为 20.0%/68.6%/58.7%，DisplayLink/渲染占比为 59.1%/29.4%/39.5%，Slint item 渲染为 39.9%/20.1%/28.8%，文本渲染为 22.6%/11.9%/16.4%。
- 已确认新 10 秒 debug sample 中 workspace refresh 约为 0.6%、run-model update 约为 0.1%；刷新/snapshot/model 优化已生效，剩余成本主要位于 Slint/Skia/Metal 终端文字绘制。诊断期间 debug 进程约为 28%-35% CPU，但旧基线只有 421 ms 且没有可比 CPU meter，不将该比例解读为总 CPU 降幅。
- 已确认用户 `13:11` sample 来自 PID 91806 的已验证 ARM64 release 二进制，并在同一进程补充 10 秒 sample 和同步 CPU meter。主线程 7,215 个样本中 run-loop 空闲约 67.0%、DisplayLink/渲染约 31.8%、Metal 路径约 31.4%、Slint 组件绘制路径约 24.1%。排除 `top` 首次 0% 初始化读数后，9 次读数平均 27.6% CPU，范围 18.9%-35.6%；physical footprint 约 290.5 MiB，峰值 331.1 MiB。Tokio、日志、7 个 PTY 和 reader 线程大部分时间阻塞。
- 已将 application-owned Local/Serial 持续输出呈现节拍从 16 ms 改为 33 ms；Tokio interval 仍保留立即首 tick 和 `MissedTickBehavior::Skip`。paused-time 回归确认 32 ms 时第二 tick 未完成、33 ms 时完成。SSH/Telnet 16 ms/16 KiB worker 批次、parser、协议应答、错误、断开和 shutdown 路径不变。
- 已生成 33 ms ARM64 release 候选（36,360,352 bytes，SHA-256 `b46058c2c03224449c06ee45a038cc69100d6e665579a70213f59856fb48889d`）。构建时 PID 91806 仍映射旧 inode 16352449，新文件是 inode 16354239；必须重启才会进入新候选。
- 已确认用户 `13:33` software sample 的 Mach-O UUID `9EDF8F81-7062-35AC-820A-221B3CAFEA06` 与当前 33 ms release 完全一致。2,248 个主线程样本中 DisplayLink 占约 67.7%、run-loop 空闲约 31.9%、Core Animation 提交约 36.8%、其中 vImage 颜色转换约 24.9%，Slint software 绘制分支约 25.8%、组件遍历约 24.6%；physical footprint 为 130.3 MiB、峰值 191.3 MiB。日志、Tokio、7 个 PTY 和 reader 线程仍主要阻塞。
- 已从锁定依赖源码确认 macOS `softbuffer` 0.4.8 每帧报告 buffer age 0，`present_with_damage` 忽略 damage 并将完整 `CGImage` 设置为 `CALayer.contents`；因此 Slint 1.17.1 software renderer 每次呈现都走 `NewBuffer` 全窗口重绘/提交。当前 dirty terminal、稳定 model 和增量行缓存仍减少应用快照工作，但不能绕过该 macOS software surface 的整帧成本。
- 已用统一 `TerminalPresentation` 接入 Local、Serial、SSH 与 Telnet monitor：无 dirty 输出时不创建 timer deadline；focused 首个脏更新立即呈现，连续输出前 500 ms/到 2 秒/超过 2 秒分别采用 16/33/50 ms，安静 250 ms 后重置；活动 split tree 中未聚焦 pane 按 Appearance 的 FPS 上限呈现（默认 4 FPS，范围 1-120），隐藏 Tab 无 deadline。`WindowRouter` route revision 和 policy watch 会唤醒有 pending 输出的 monitor，焦点、Tab 或设置变化后立即按新策略重算；SSH 合并批次保留最早 `received_at`，parser、协议应答、错误、断开和 shutdown 仍走即时路径。
- 已生成双策略 ARM64 release 候选（36,376,960 bytes，inode `16356094`，SHA-256 `f419bfbcf7b50e3431062b7b78d5b3053e238265dd6133b5c5a23814ec8d291f`，Mach-O UUID `792FB118-6118-31F4-9359-CA56B5692B8D`）。检查时运行中的 PID 94454 仍映射旧 inode `16354239`，必须退出并重启后才会运行新候选。
- 已将 schema 提升到 25，并贯通默认开启的 `terminal_compact_rendering` 与默认关闭的 `terminal_row_render_cache`：Settings 草稿可即时预览并在关闭时保存，所有存活窗口同步更新；旧文件缺字段时采用默认值。
- 已让 Rust renderer 为每个有界可见行生成合并后的非默认背景 span 和 underline/strikethrough 装饰 span。Slint 紧凑分支直接绘制 Text，旧分支保留为 A/B；可选 `cache-rendering-hint` 只包住静态行内容，选区、光标、目标高亮和 IME/preedit 留在层外。
- 已生成可配置渲染优化的 ARM64 release 候选（36,492,784 bytes，inode `16358516`，SHA-256 `ca1cffe72761baa1c481e9601ff8e07b6f18d5c7f749eaa5c910ad2bcc9a09b6`，Mach-O UUID `8ECE3718-6E3D-370B-94F5-193A455BE533`）。检查时没有运行中的 AxSSH 进程，下一轮可直接启动该候选。
- 已将 `terminal_compact_rendering` 与 `terminal_row_render_cache` 两个开关从 Settings > Terminal 移到 Settings > Appearance 的 RENDERING 分组，与 Renderer 选择同区；Settings 搜索目录和双语 usage 文档同步改为 Appearance 归属，配置字段与默认值不变。
- 已将聚焦与可见未聚焦终端呈现周期改为 `focused_terminal_refresh_fps` / `unfocused_terminal_refresh_fps`，schema v26 默认分别为 60/4 FPS，范围限制为 1-120；Appearance > Rendering 使用 SpinBox，Settings preview/save 同步所有窗口并通过 `WindowRouter` policy watch 立即唤醒 pending monitor，聚焦连续输出的 16/33/50 ms 自适应仍保留。

## 验证

- 已完成：sample/环境基线、PERF2-PERF6 focused 回归、PERF8 debug 结构性对照、PERF9 release 10 秒 sample/CPU meter、PERF10 33 ms paused-time 回归、PERF11 software 短样本归因、双策略状态机与四协议接线、紧凑 span/配置 round-trip/Settings 搜索/nested model identity 定向测试；本轮 FPS 配置的定向测试、`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 199、应用 191、Doc tests 0）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py`（429 条翻译）和 `git diff --check` 均已通过。tracker validator 可执行，但报告 45 条既有旧历史/research 格式问题；本轮新增条目未增加报错。
- 未完成：同一新 release 的旧 item 树/GPU 紧凑/GPU 紧凑+行缓存/Software 紧凑 10 秒 sample/CPU meter，以及目标平台 GUI/Local/SSH/Telnet/Serial 验收。

## 风险与阻塞

- 当前 worktree 已包含用户未提交的终端边距、几何和双语文档改动；本目标必须在其基础上追加并避免覆盖。
- UI 刷新批处理只能延后呈现通知，不能延后 terminal parser、PTY protocol response、错误、退出或 shutdown。
- 脏终端路由必须在 pane 转移、split、关闭和 detached/return 结构变化时回退到完整刷新，避免更新错误窗口或陈旧 UUID。
- release 为 strip-symbols 产物，系统级 DisplayLink/Metal 和线程阻塞结论可靠，但不能从该二进制的 sample 进一步精确拆分应用内部 Rust 函数。
- 双策略候选会让 focused 持续输出在 16/33/50 ms 间逐级降低呈现频率，并按 Appearance 设置限制可见未聚焦 pane（默认 4 FPS）；预期减少实际文字绘制，但聚焦平滑度与后台可读性仍须通过同负载 A/B 和用户视觉验收确认。
- macOS software surface 当前不支持 damage presentation，窗口像素面积、可见 pane 数和呈现频率都会直接放大整帧 CPU 成本；继续降低 software cadence 或尝试另一 CPU renderer 都会引入平滑度、文本质量或依赖行为取舍，必须单独 A/B，不能替代 GPU 推荐路径。
- 双呈现策略必须按当前 `WindowRouter`/`PaneTree` 动态读取焦点，不能在 worker 中缓存 pane 归属；切换焦点后旧后台输出必须在新 deadline 内追上，但隐藏 Tab 仍不应进入 Slint event queue。
- rust-skia release build script 在 release cache 缺失时会绕过 Cargo offline 语义尝试下载预编译包；本轮最终构建复用了本机同名、同 SHA-256 的 debug cache，未把该缓存加入 Git 或项目依赖。
- 当前没有运行中的 AxSSH 进程；下一次采样必须先核对 Mach-O UUID `8ECE3718-6E3D-370B-94F5-193A455BE533`，避免使用旧 release 或其它构建类型。
- Slint 1.17.1 的 `cache-rendering-hint` 只在 Skia/FemtoVG layer renderer 中保留离屏图像；software renderer 不提供等价 layer cache。缓存必须只包住静态终端行内容并默认关闭，避免光标闪烁或选区变化使所有行缓存失效，也避免未测量的 Retina 纹理占用成为默认成本。

## 下一步

- 执行 PERF14：启动 UUID `8ECE3718-6E3D-370B-94F5-193A455BE533` 的 release，在相同窗口、7 pane 和持续负载下依次测量 GPU 旧 item 树（两项关闭）、GPU 紧凑节点、GPU 紧凑节点+静态行缓存，以及 Software 紧凑节点；同时验收光标、选区、IME、彩色背景、underline/strikethrough、focused 响应和后台 4 Hz 可读性。

## 最后更新时间

- 2026-08-21 16:40 +0800
