# 当前项目实施记录

## 当前目标

- 目标 ID：20260823-terminal-dirty-region-backend-bypass
- 目标：绕过 Slint 1.17.1/winit-software/macOS softbuffer 的 dirty-region 提交限制，让现有终端分区真正进入持久 framebuffer 和分块 CoreAnimation 提交路径。
- 交付物：本地锁定 backend patch、持久 macOS framebuffer、实际多矩形 damage 提交、非 macOS 行为保持兼容、backend focused 回归和完整离线门禁。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`vendor/{softbuffer,i-slint-backend-winit}`、`Cargo.toml`、`Cargo.lock`、第三方许可说明、renderer/backend 研究和本轮实施跟踪记录。
- 不在本轮范围内：SSH host-key/认证/凭据契约、parser/worker/transport、Slint UI 业务组件、终端模型/设置 schema、专用 Metal 终端 surface、参考工程代码或构建耦合，以及未经用户提供截图的 GUI 视觉结论。

## 当前状态

- 阶段：实现已完成，待目标平台验收
- 开工判定：允许开工
- 是否需要联网：是，已完成
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
| PERF14 | blocked | 同一 release 的 Software/GPU/行缓存 10 秒 A/B 和目标平台 GUI/真实 transport 验收 | 相同窗口、7 pane、负载和 sample/CPU meter | 依次对照旧 item 树、紧凑节点和紧凑节点+静态行缓存；等待用户目标平台验收。 |
| PERF15 | completed | 两项独立、向后兼容的终端渲染性能设置及 Settings 预览/持久化链路 | config normalization/serde 与 Rust/Slint 映射回归 | 紧凑节点默认开启；静态行 layer cache 默认关闭。 |
| PERF16 | completed | 可切换的紧凑 run 节点树与只覆盖静态行内容的 layer cache | Slint 编译、model identity 与终端交互回归 | 光标、选区、IME 和目标高亮保持在缓存层外。 |
| PERF17 | completed | 双语契约、翻译、完整离线门禁和新 release A/B 候选 | fmt、check、Clippy、test、translation、Markdown/tracker、release build、`git diff --check` | 不升级 renderer 或引入参考工程耦合。 |
| PERF18 | completed | 把两个终端渲染性能开关从 Terminal 页移到 Appearance 页 RENDERING 分组 | Slint 编译、Settings 搜索路由回归、完整 Cargo 门禁、翻译/链接/`git diff --check` | 配置 schema、字段名、默认值和持久化不变；只调整展示分组与搜索归属。 |
| PERF19 | completed | 将聚焦/可见未聚焦终端刷新周期改为可配置 FPS，并接入设置预览、持久化和热更新 | FPS 归一化/serde、Slint callback、policy watch、四协议呈现状态机和完整门禁 | 范围 1-120 FPS；默认聚焦 60、未聚焦 4；聚焦 16/33/50 ms 自适应仍作为上限策略。 |
| CURSOR1 | completed | 可配置 Terminal 光标闪烁开关、Settings 预览/持久化和默认兼容 | config serde、Slint 编译、定向测试、完整 Cargo 门禁 | 默认开启；关闭后光标保持显示，不影响输入/IME |
| FOCUS1 | completed | 原生窗口失焦时将所有可见终端切换到 Unfocused FPS 上限，重新激活后恢复 pane 聚焦策略 | WindowActiveChanged、AppKit 激活同步、WindowRouter 路由回归、Slint/Cargo 门禁和双语契约 | 激活状态只在运行时维护；隐藏 Tab 仍不刷新，parser/协议应答/错误/断开/shutdown 不延迟 |
| FOCUS4 | completed | 修正 macOS 原生窗口激活状态同步，增加 `NSWindow.isKeyWindow()` 100ms UI 轮询兜底 | macOS AppKit bridge、WindowRouter 路由回归、Cargo/Slint 门禁 | 事件钩子保留为快速路径；不改变 parser、协议应答或终止路径 |
| MEM1 | completed | 显式限制 Tokio worker/blocking 线程并缩短空闲 blocking 线程保留时间 | runtime 配置单测、完整 Cargo 离线门禁、线程数复核 | 保留至少 2 个 async worker、最多 4 个；blocking 池最多 8 个，空闲 2 秒后允许退出。 |
| MEM2 | completed | 断开 SFTP 后不让图标预热任务强持有 AppState，并记录图标缓存释放数量 | file-icon 生命周期 focused tests、完整 Cargo 离线门禁 | 预热目标使用 `Weak<AppState>`；Fontique、Slint、CoreAnimation 与 macOS allocator 的进程级缓存不承诺立即归还 RSS。 |
| MEM3 | completed | 更新双语架构、环境审计和可重复资源验证说明 | 文档相对链接、tracker/env-audit validator、`git diff --check` | 说明 Rust drop、线程池回收和平台缓存之间的边界，不把单次 sample 当作泄漏证明。 |
| APP1 | completed | 按职责拆出 renderer/Tokio runtime 与启动字体辅助模块 | Rust module check、Cargo check、runtime tests | `src/app/runtime.rs` 负责 renderer、Tokio worker 上限/回收和启动字体读取；生成类型仍留在 app 层。 |
| APP2 | completed | 拆出 detached workspace、窗口激活和窗口动作处理 | Cargo check、workspace/window focused tests | `src/app/window_bridge.rs` 只编排 AppWindow、WindowRouter 和 AppState DTO，不改变 pane transfer、worker shutdown 或窗口生命周期。 |
| APP3 | completed | 拆出平台剪贴板、诊断和 macOS 菜单辅助 | Cargo check、diagnostic/menu tests | `src/app/platform_support.rs` 保持 cfg 隔离；诊断不增加 host/path/password/session 内容。 |
| APP4 | completed | 更新项目地图、架构说明并完成完整离线门禁 | fmt、check、Clippy、test、tracker、`git diff --check` | 不改变 Slint/Cargo/SSH/renderer 行为契约。 |
| MEM4 | completed | 统一释放主窗口和 detached 窗口的 Slint models、文本、图标行与敏感 UI 字段，并在退出前丢弃窗口强引用 | 生命周期 focused tests（如适用）、fmt、check、Clippy、test、tracker、`git diff --check` | Software/GPU 共用同一路径；不承诺平台 allocator/Fontique/CoreAnimation/Metal RSS 立即下降。 |
| MEM5 | completed | 减少 detached 窗口重复持有的 sidebar/settings/editor/font option models，并记录 bundled Fontique 字体的常驻边界 | Slint/Cargo 门禁、tracker/Markdown 检查、静态生命周期审阅 | Terminal/SFTP surface 所需 model 保留；不伪造 Fontique 动态卸载，不改变字体字重或 fallback 行为。 |
| MEM6 | completed | Maple/Iosevka/Monaspace 改用 Fontique 路径源，避免 worker 长期持有完整字体 `Vec<u8>`；JetBrains 保留嵌入 | font_bridge 定向测试、完整 Rust/Slint 离线门禁、同负载 macOS footprint/vmmap 复核 | 代码与离线门禁已通过；保留现有字体选择和 Maple Hani fallback；目标平台仍需确认 Fontique cache 与 RSS 变化。 |
| RZ1 | completed | 记录 resize 根因、外部依据和低风险实施边界 | tracker 规范、现有 resize 路径审计、research 记录 | 采用 resize 前沿尺寸合并与 resize-only 刷新；暂缓平台专用 live-resize surface hold。 |
| RZ2 | completed | resize 成功后只调度当前 terminal pane snapshot | focused 路由/刷新 gate 回归、Slint/Cargo check | 不改变非 resize 的 full refresh 语义。 |
| RZ3 | completed | AppState 在真实模型尺寸变化前去重模型、选区 revision 和 worker 请求 | state resize focused tests | worker 请求失败时保持现有错误优先级。 |
| RZ4 | completed | SSH/Telnet watch 与 Local pending resize 对相同尺寸不发通知/唤醒 | worker/local focused tests | latest-value 合并仍保留，Serial 无 PTY resize 通道。 |
| RZ5 | completed | 完整 Rust/Slint、tracker 和差异门禁并记录平台验收边界 | fmt、check、Clippy、test、tracker、`git diff --check` | macOS 实际拖动流畅度由用户验收。 |
| TILE1 | completed | 设计固定 8 行 tile DTO、行模型所有权和稳定 identity | 现有 view/Slint 边界审阅 | 保留 bounded visible rows；交互覆盖层不进入静态 tile。 |
| TILE2 | completed | 实现 tile 分组、变化 tile 原地更新和 Slint tile 内行绘制 | focused model identity 回归、Slint 编译 | 不改变 terminal parser、worker 或 renderer 依赖。 |
| TILE3 | completed | 补 tile 边界、跨 tile dirty 和行数变化回归 | focused Rust tests | 覆盖最后不足 8 行尾部 tile、tile identity 和 nested row identity。 |
| TILE4 | completed | 完整离线门禁并记录 sample/A-B 验收边界 | fmt、check、Clippy、test、`git diff --check` | tile 不能绕过 macOS software surface 的整帧 present；A/B 留目标平台验收。 |
| DIRTY1 | completed | 增加 tile 级 source/render revision，未变化 tile 跳过行扫描并保持 tile model identity | focused tile revision/model identity 回归 | revision 只作为复用提示；未知/零 revision 回退保守比较。 |
| DIRTY2 | completed | 变化 tile 内只更新变化行，保持光标/选区/IME/目标高亮的全局行坐标和窗口动态几何 | Slint 编译、行坐标/resize focused 回归 | 不改变 terminal parser、worker、transport 或 renderer 依赖。 |
| DIRTY3 | completed | 完整离线门禁并记录 17:32 sample 的可比性与 software full-frame 残余成本 | fmt、check、Clippy、test、tracker、Markdown、`git diff --check` | 短 sample 只能作热点定位，不能宣称 CPU 降幅。 |
| PARTITION1 | completed | 终端分区策略枚举、默认值和 serde/normalize 兼容 | config focused tests | 缺失或非法值回退 `tile-8`，不升级 schema。 |
| PARTITION2 | completed | 分区策略配置/运行时兼容接线 | Slint compile、配置与映射回归 | 预览、保存和持久化链路保留；逐行/8 行/16 行选择器暂时不在 Settings 展示。 |
| PARTITION3 | completed | 运行时 tile-size 接线和回归 | tile-size focused test、Cargo check | 逐行、8 行、16 行保持全局 `start_row` 和 dirty-row 复用。 |
| PARTITION4 | completed | 完整门禁 | fmt/check/Clippy/test/tracker/diff | 全量 Rust/Slint、翻译和 tracking 检查通过。 |

## 本轮实施计划

- `BACKEND1`：固定上游证据和本地补丁边界，确认 1.17.1 的 `region.iter()` 与 macOS softbuffer age/present 行为。
- `BACKEND2`：引入本地 `i-slint-backend-winit` patch，提交实际多矩形 damage，而不是 bounding box。
- `BACKEND3`：引入本地 `softbuffer` CoreGraphics patch，复用持久 framebuffer，并按 damage 影响的分块 CALayer 更新；resize/首帧/全量 damage 回退整帧。
- `BACKEND4`：补 backend focused 回归、第三方许可和双语架构说明，执行 locked/offline 完整门禁并记录平台验收边界。

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| BACKEND1 | completed | 锁定 Slint/winit/softbuffer 证据与 patch 边界 | 本机 crate 源码、PR #12758、macOS sample 对照 | Slint 1.17.1 的 winit bbox 与 macOS softbuffer age=0/full CGImage 已确认。 |
| BACKEND2 | completed | winit software 多矩形 damage forwarding | vendored backend compile、locked check | `PhysicalRegion::iter()` 逐项映射为 softbuffer Rect。 |
| BACKEND3 | completed | macOS 持久 framebuffer 与 tiled CoreAnimation present | macOS 条件编译、locked check | 首帧/resize/full damage 全量；后续只更新相交 512x64 tile。 |
| BACKEND4 | completed | 文档、许可、focused/完整门禁和平台验收说明 | fmt/check/Clippy/test/translation/diff、vendor rustfmt | 代码和静态门禁已完成；GUI sample、CPU/footprint A/B、DPI/resize/残影仍待用户目标平台执行。 |

- `PARTITION1`：增加终端分区策略枚举、默认值和 serde/normalize 兼容。
- `PARTITION2`：保留策略在配置、预览、保存和运行时的兼容接线；逐行/8 行/16 行选择器暂时不在 Appearance 展示。
- `PARTITION3`：按设置选择 1/8/16 行 tile，保持 dirty-row 更新和动态行位置契约。
- `PARTITION4`：执行 focused、翻译、tracker 和完整离线门禁。

- `DIRTY1`：为 tile 保存有界的 source/render revision 摘要；revision 一致时直接复用现有 tile，不扫描其行模型；历史或未知 revision 保守回退。
- `DIRTY2`：revision 变化时仅在对应 tile 内按行更新，继续由 Slint 根据 pane 高度、字体行高和 `start_row` 计算每行位置；交互覆盖层保持全局行语义。
- `DIRTY3`：执行 focused 与完整离线门禁，记录 17:32 sample 仍属于 software renderer 整帧绘制的证据边界。

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
- 已从锁定依赖源码确认原版 macOS `softbuffer` 0.4.8 每帧报告 buffer age 0，`present_with_damage` 忽略 damage 并将完整 `CGImage` 设置为 `CALayer.contents`；本轮本地 patch 已改为持久 framebuffer，首帧/resize/失效缓冲全量更新，后续只替换相交的 512x64 CoreAnimation tile layer。winit patch 同时保留 Slint 产生的每个独立 damage rectangle，不再先合并 bounding box。
- 已用统一 `TerminalPresentation` 接入 Local、Serial、SSH 与 Telnet monitor：无 dirty 输出时不创建 timer deadline；focused 首个脏更新立即呈现，连续输出前 500 ms/到 2 秒/超过 2 秒分别采用 16/33/50 ms，安静 250 ms 后重置；活动 split tree 中未聚焦 pane 按 Appearance 的 FPS 上限呈现（默认 4 FPS，范围 1-120），隐藏 Tab 无 deadline。`WindowRouter` route revision 和 policy watch 会唤醒有 pending 输出的 monitor，焦点、Tab 或设置变化后立即按新策略重算；SSH 合并批次保留最早 `received_at`，parser、协议应答、错误、断开和 shutdown 仍走即时路径。
- 已生成双策略 ARM64 release 候选（36,376,960 bytes，inode `16356094`，SHA-256 `f419bfbcf7b50e3431062b7b78d5b3053e238265dd6133b5c5a23814ec8d291f`，Mach-O UUID `792FB118-6118-31F4-9359-CA56B5692B8D`）。检查时运行中的 PID 94454 仍映射旧 inode `16354239`，必须退出并重启后才会运行新候选。
- 已将 schema 提升到 25，并贯通默认开启的 `terminal_compact_rendering` 与默认关闭的 `terminal_row_render_cache`：Settings 草稿可即时预览并在关闭时保存，所有存活窗口同步更新；旧文件缺字段时采用默认值。
- 已让 Rust renderer 为每个有界可见行生成合并后的非默认背景 span 和 underline/strikethrough 装饰 span。Slint 紧凑分支直接绘制 Text，旧分支保留为 A/B；可选 `cache-rendering-hint` 只包住静态行内容，选区、光标、目标高亮和 IME/preedit 留在层外。
- 已生成可配置渲染优化的 ARM64 release 候选（36,492,784 bytes，inode `16358516`，SHA-256 `ca1cffe72761baa1c481e9601ff8e07b6f18d5c7f749eaa5c910ad2bcc9a09b6`，Mach-O UUID `8ECE3718-6E3D-370B-94F5-193A455BE533`）。检查时没有运行中的 AxSSH 进程，下一轮可直接启动该候选。
- 已将 `terminal_compact_rendering` 与 `terminal_row_render_cache` 两个开关从 Settings > Terminal 移到 Settings > Appearance 的 RENDERING 分组，与 Renderer 选择同区；Settings 搜索目录和双语 usage 文档同步改为 Appearance 归属，配置字段与默认值不变。
- 已将聚焦与可见未聚焦终端呈现周期改为 `focused_terminal_refresh_fps` / `unfocused_terminal_refresh_fps`，schema v26 默认分别为 60/4 FPS，范围限制为 1-120；Appearance > Rendering 使用 SpinBox，Settings preview/save 同步所有窗口并通过 `WindowRouter` policy watch 立即唤醒 pending monitor，聚焦连续输出的 16/33/50 ms 自适应仍保留。
- 已新增默认开启的 `terminal_cursor_blink` Appearance 设置，贯通 serde、Settings 预览/保存、主窗口与 detached 窗口；关闭后停止光标闪烁 Timer 并保持光标显示，重新开启时立即恢复可见，不影响终端 cursor visibility、IME 或选区。
- 已将原生窗口激活纳入终端呈现路由：Slint `WindowActiveChanged` 事件作为快速路径，macOS UI 线程每 100ms 读取每个 `NSWindow.isKeyWindow()` 兜底，并通过 `WindowRouter` route revision 唤醒 pending monitor；窗口失焦时该窗口所有可见 pane（包括最后保持焦点的 pane）使用 `unfocused_terminal_refresh_fps`，重新激活后恢复 focused/unfocused pane 分类，隐藏终端和 parser/协议即时路径不变。
- 已将 Tokio runtime 改为显式有界配置：按 `available_parallelism` 取 2-4 个 async worker，blocking 池最多 8 个，blocking 线程空闲 2 秒后允许退出；启动日志记录实际 worker 上限。SFTP 图标预热目标改为 `Weak<AppState>`，最后一个 SFTP Tab 清理扩展 icon 时记录释放数量，迟到 generation 不再持有强状态引用。
- 已复核用户 2026-08-22 19:40 sample：PID 83798 的 Mach-O UUID `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC` 与当前 `target/debug/ax_ssh` 一致，启动日志确认 `worker_threads=4`、`max_blocking_threads=8`；旧样本的 10 个 `tokio-rt-worker` 降为新样本的 4 个 `axssh-tokio`（栈显示为 Tokio blocking pool）。但新样本仍是 `WinitSoftwareRenderer`，本机配置为 `renderer_preference=software`、`terminal_compact_rendering=false`、`terminal_row_render_cache=false`，且旧样本包含本地 PTY 线程、新样本没有，不能作为同负载 renderer/刷新 A/B。新旧 footprint/peak 分别为 128.0/176.7 MiB 与 150.2/183.2 MiB；新样本主线程仍出现 42 个 DisplayLink、35 个 software render 样本，说明软件 surface 整帧路径仍是主要瓶颈，不表示 runtime 生命周期优化未生效。
- 已复核用户 2026-08-22 22:24 sample：PID 89392 的 Mach-O UUID `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC` 与当前 debug 二进制一致；physical footprint/peak 为 229.4/267.2 MiB。主线程 2,254 个样本中约 1,952 个在 CoreFoundation 等待，272 个进入 DisplayLink，271 个进入 Slint Skia `MetalSurface::render`，223 个执行 dirty-region `draw_contents`；4 个 `axssh-tokio` 线程均主要阻塞或等待任务。该样本确认当前运行的是 GPU/Skia/Metal renderer，不是 SoftwareRenderer。
- 内存/线程生命周期机制不按 renderer 分支：Tokio async/blocking 上限、blocking 空闲回收、SFTP 图标缓存清理、`Weak<AppState>`、session/PTY/SSH/SFTP worker shutdown 以及 Rust/Slint 对象 drop 都应对所有 renderer 生效。区分 Software 与 GPU/Skia/Metal 仅用于解释 renderer 自身的 framebuffer、Skia surface、CAMetalLayer drawable、Metal command buffer、Fontique、CoreAnimation 和 allocator 缓存；这些平台级缓存即使 Rust 对象已 drop，也不保证 RSS 立即下降。
- 最新 sample 的 footprint 高于旧 Software sample，不能直接归因于生命周期失效：两者运行时长、负载、renderer 和可见线程条件不同；最新 sample 还是 debug 构建。单次 footprint/peak 不能证明泄漏，必须用同一 release、同一 renderer、同一窗口/pane/负载重复至少三轮，并结合 `vmmap -summary`、线程数和打开/关闭 Settings/Terminal/SFTP 前后对照。
- 已复核用户 2026-08-22 22:38 Software sample：PID 90375 的 UUID 仍为 `8178A1BD-6EA8-39C3-94A9-0A12E9AE24AC`；physical footprint/peak 为 120.4/175.1 MiB。主线程 2,033 个样本中约 1,322 个进入 CoreFoundation source、1,320 个 DisplayLink、1,185 个 `WinitSoftwareRenderer`、1,172 个 `draw_contents`，966 个进入 software `render_buffer_impl` 的 buffer 遍历；4 个 `axssh-tokio` 仍存在，且样本也包含 SSH session/monitor 与 UI refresh 调度。与 22:24 GPU sample 相比，Software 的 CPU 整帧路径明显更热、footprint 反而更低；由于启动时长、窗口/pane 和输出量未完全固定，这只能确认 renderer 行为差异，不能作为严格内存 A/B 或泄漏结论。
- 已确认应用退出与 detached 窗口关闭前需要显式清空 Slint model、编辑器文本、SFTP/Terminal 行和安全提示字段；这些应用拥有对象由 Software/GPU 共用的窗口清理函数释放，renderer surface 随 window adapter drop 释放，平台级缓存的 RSS 归还仍不作即时承诺。
- 已实现 `release_window_resources`、`release_detached_windows` 和退出前显式 drop：Return/Close 立即清空 detached UI 并移除 map 强引用；退出先停止 worker，再清理 detached/main window、图标缓存和 UI callback 引用，最后 shutdown Tokio。
- 已完成字体资源审计：JetBrains Mono 四字重约 1.1 MiB 且内嵌；Iosevka Term 四字重约 18.8 MiB、Maple Mono NF CN 两字重约 40 MiB、Monaspace Neon Variable 约 1.6 MiB，均按选中 family 懒加载。Fontique shared collection 没有可靠的运行时卸载契约，已将其视为进程级缓存边界，不做伪释放。
- 已将外部自带字体从 `fs::read`/memory `Blob` 改为经过大小校验的 `PathBuf` source；Fontique 注册阶段使用 mmap，字体数据按需由路径 source/cache 提供，JetBrains Mono 仍使用嵌入 bytes。Maple 注册后通过 family ID 保持唯一 Hani fallback，避免约 39.3 MiB Maple heap 副本由应用加载任务长期持有。
- 已完成本轮 resize 路径审计：Slint pane 已有 16ms latest-size timer，但 resize callback 仍触发完整 active/workspace snapshot；AppState 与 SSH/Telnet/Local worker 对同一尺寸缺少统一入口去重。
- 已完成 resize-only 刷新：成功的 `resize-terminal` callback 通过 `dispatch_terminal_snapshot` 进入现有 dirty terminal gate，只更新当前可见 pane；结构变化仍走 full refresh，隐藏 pane 不进入 UI event loop。
- 已完成尺寸去重：`TerminalModel::size()` 提供当前规范化尺寸，AppState 在 worker 请求前短路同尺寸请求；selection revision 只在真实模型变化时推进，worker 请求失败不改变本地模型。
- 已完成 worker 去重：SSH/Telnet 使用 `watch::Sender::send_if_modified`，Local 使用请求尺寸与 pending latest-value 合并并只在有效变化时唤醒 PTY worker；Serial 继续没有 PTY resize 通道。
- 已完成终端 tile 分组：Rust 以固定 8 行构造 `TerminalRenderTile`，Slint 顶层 repeater 只遍历 tile，tile 内保留有界行绘制；未变化 tile 和 nested row/run model 原地复用，光标、选区、IME/preedit 和目标高亮仍在 tile 外覆盖。
- 已完成 tile dirty-row 精确更新：tile DTO 携带 source/render revision 摘要；已知 revision 相同时直接复用 tile 和行模型，变化 tile 才按行更新，零/未知 revision 保守回退到模型比较。

## 验证

- 已完成：TILE1-TILE4、DIRTY1-DIRTY3、BACKEND1-BACKEND4；`cargo fmt --all -- --check`、vendor backend rustfmt、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 202、应用 197、Doc tests 0）、`cargo build --locked --offline`、翻译检查和 `git diff --check` 均通过。最终 debug 候选 UUID 为 `C3D69932-5675-3AFB-80F3-775D77935230`，SHA-256 为 `a8e42c04a519874f96ef73e67f1bed04eeb80a30dd3512948989cd31cc61249d`。
- 未完成：目标平台 GUI sample/A-B 和视觉验收；需要确认持续输出下 CPU 是否下降，以及 tile layer 是否存在闪烁、残影、resize 花屏或 Retina 坐标偏移。

## 风险与阻塞

- 当前 worktree 已包含用户未提交的终端边距、几何和双语文档改动；本目标必须在其基础上追加并避免覆盖。
- UI 刷新批处理只能延后呈现通知，不能延后 terminal parser、PTY protocol response、错误、退出或 shutdown。
- 脏终端路由必须在 pane 转移、split、关闭和 detached/return 结构变化时回退到完整刷新，避免更新错误窗口或陈旧 UUID。
- release 为 strip-symbols 产物，系统级 DisplayLink/Metal 和线程阻塞结论可靠，但不能从该二进制的 sample 进一步精确拆分应用内部 Rust 函数。
- 双策略候选会让 focused 持续输出在 16/33/50 ms 间逐级降低呈现频率，并按 Appearance 设置限制可见未聚焦 pane（默认 4 FPS）；预期减少实际文字绘制，但聚焦平滑度与后台可读性仍须通过同负载 A/B 和用户视觉验收确认。
- macOS software surface 现在通过本地 softbuffer patch 支持有界 tile damage presentation，但每个相交 tile 仍需复制和创建 `CGImage`，不能等同于硬件 overlay 或任意像素级提交。窗口像素面积、可见 pane 数和呈现频率仍会影响成本；GPU/Skia 路径仍按普通 CAMetalLayer drawable present，需分别 A/B。
- 双呈现策略必须按当前 `WindowRouter`/`PaneTree` 动态读取焦点，不能在 worker 中缓存 pane 归属；切换焦点后旧后台输出必须在新 deadline 内追上，但隐藏 Tab 仍不应进入 Slint event queue。
- 窗口资源清理必须先停止应用 worker，再清空 detached/main 的 Slint model 并移除强引用；不能把清理延迟到 timer 或依赖函数作用域自然 drop，否则窗口 renderer surface 和 model 可能继续存活。
- rust-skia release build script 在 release cache 缺失时会绕过 Cargo offline 语义尝试下载预编译包；本轮最终构建复用了本机同名、同 SHA-256 的 debug cache，未把该缓存加入 Git 或项目依赖。
- resize-only 刷新仍必须在 pane 转移、split、关闭和 detached/return 结构变化时由 WindowRouter 回退到 full refresh；本轮不改变该结构变化语义。
- 应用层 tile/row 更新与本地 backend tile present 现在串联起来，但每个 tile 仍是 512x64 的完整图像更新；不能把它描述为任意像素级零拷贝。升级 Slint/softbuffer 后必须重新核对上游 damage、buffer age、CoreAnimation layer 和 DPI 语义。
- Slint 1.17.1 的 `cache-rendering-hint` 只在 Skia/FemtoVG layer renderer 中保留离屏图像；software renderer 不提供等价 layer cache。缓存必须只包住静态终端行内容并默认关闭，避免光标闪烁或选区变化使所有行缓存失效，也避免未测量的 Retina 纹理占用成为默认成本。

## 下一步

- 用相同窗口、pane 数、renderer 和持续输出对照旧行模型、8 行 tile、8 行 tile + dirty revision、16 行 tile 与 GPU/Skia。
- 在目标 macOS 上验收光标、选区、IME、彩色背景、underline/strikethrough、focused 响应和后台 FPS；若热点仍是 `WinitSoftwareRenderer`/CoreAnimation，再单独立项评估 GPU 或平台 backend。

## 最后更新时间

- 2026-08-24：暂时隐藏 Appearance 中的逐行/8 行/16 行分区选择器；配置字段、预览/保存接线和运行时默认 `tile-8` 保持兼容，目标平台 software/GPU A/B、连续拖动和 GUI 视觉仍待用户验收。
- 2026-08-23 22:50 +0800
- 计划状态变更：BACKEND1: pending -> completed; BACKEND2: pending -> completed; BACKEND3: pending -> completed; BACKEND4: in_progress -> completed
- 验证结果：本地 backend patch 已清理死代码并通过 vendor rustfmt、locked/offline Cargo 全量门禁、翻译检查和 `git diff --check`；实际 macOS GUI/A-B 仍待用户执行。
- 计划切换：PARTITION1-PARTITION4 已完成；分区策略运行时仍默认使用 `tile-8`，选择器暂时隐藏，目标平台 GUI/A-B 仍留用户执行。
- 影响文件：`src/config/{settings,tests}.rs`、`src/app/{settings_bridge,view/settings,view/terminal,view/tests}.rs`、`ui/{app,settings,settings/appearance,workspace-shell}.slint`、`translations/zh-CN/LC_MESSAGES/ax_ssh.po`、双语架构/使用文档和 tracker。
- 计划状态变更：PARTITION1: pending -> completed; PARTITION2: pending -> completed; PARTITION3: pending -> completed; PARTITION4: in_progress -> completed
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 202、应用 197、Doc tests 0）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py`、tracker validator 和 `git diff --check` 通过。
- 对 plan 的更新：设置默认 `tile-8`；逐行/8 行/16 行只改变 UI tile 分组，保留 dirty-row revision、动态 `start_row` 几何以及终端 parser/worker/输入边界。
