# 当前项目实施记录

## 当前目标

- 目标 ID：20260826-macos-software-partial-presentation
- 目标：逐变量评估 macOS CPU-only software presentation 在 `present_tiles -> CATransaction::commit` 路径上的脏区收益，以可回退实验验证普通 `CALayer` backing store 的矩形失效行为，将两条 CPU presentation 路径纳入持久化渲染选项，并降低终端空闲时的周期性 worker/UI 唤醒。
- 交付物：COLOR1 显式 sRGB 变量；COLOR2 同负载 release 采样与热点归因；CELL1-CELL4 终端文字/覆盖层格宽统一、真实行原点和单格拖选；COLOR3 可选 Core Animation 局部 backing store 原型、边界测试和目标平台 A/B；CPU1-CPU5 两条 macOS Software presentation 路径的配置、启动接线、Appearance 选项、默认值和门禁；WAKE1-WAKE4 SSH/Telnet 按需输出 flush、Local PTY 事件唤醒/低频退出兜底、macOS 激活低频兜底和完整门禁；XPLAT1-XPLAT4 跨平台 damage 能力描述、各 backend 映射、winit fallback 和双语契约；保留当前 damage、持久 framebuffer、终端行模型和安全图像所有权边界。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：Software presentation、终端呈现调度，以及本轮复核发现的 profile mutation、凭据落盘、host-key 确认和重连快照并发安全；相关实现位于 `src/config/`、`src/app/`、`src/ssh/`、vendor backend 和 `docs/project-implementation-tracker/`。
- 不在本轮范围内：终端 parser/内容模型、GPU/Metal renderer、`CATiledLayer`、IOSurface、多缓冲交换链、参考工程代码或构建耦合；本轮不改变 SSH 协议或认证算法，只修复其应用侧 profile/trust 生命周期。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| SPR1 | completed | 建立 9 项问题到源码、架构边界和验证命令的映射 | session 原文、project-map/current、Rust/Slint skill 审阅 | 保留用户未提交的 `ui/components/terminal-grid.slint`；不涉及 SSH/trust/凭据。 |
| SPR2 | completed | 修复 cfg 门禁、Software/macOS 限定、scale 事件重新发布和布局注销 | macOS cfg 审阅、布局 registry focused tests、Cargo check | scale 变化重新按逻辑区域换算物理像素；窗口关闭/返回主动注销 registry entry。 |
| SPR3 | completed | 每个 buffer/present 周期只读取一次 layout generation，重建时才 clone snapshot，并移除重复 rows 设置 | backend pure tests、settings bridge 静态审阅 | 不改变 damage、buffer age 或 CoreAnimation image ownership。 |
| SPR4 | completed | 补齐 pane 移动、notice/grid-clip、reset 后的布局重新注册，并限制非 Software renderer 的 UI 回调 | Slint compile、layout callback contract review | presentation timer 与 PTY resize timer 已分离；终端 parser、worker、协议应答和 shutdown 不改。 |
| SPR5 | completed | 用生产 tile geometry/damage 路径覆盖多 pane、裁剪、重叠、generation 和失效行为，并同步文档 | focused tests、Cargo fmt/check/Clippy/test、tracker/Markdown/diff checks | 8 个隔离 vendor 单测、根工程门禁和双语文档已完成；目标 macOS GUI/Retina 视觉仍由用户验收。 |
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
| PERF20 | completed | Software 专属持续输出 20/15/10 FPS 自适应，以及无可见 terminal snapshot 的 UI publication 抑制 | 呈现状态机、控制序列/cursor 回归、完整 Cargo/Slint 离线门禁 | 已由 PERF21 取代 fixed-FPS 部分；不安全地局部写入单个 `CGImage` 不在范围内。 |
| PERF21 | completed | Software 可见输出改为无固定 FPS 的单槽最新快照背压，并合并 UI 消费前的脏行 | 状态机、UI refresh gate、snapshot 合并回归和完整 Cargo/Slint 离线门禁 | 不延迟 parser、协议应答、错误、断开或 shutdown；GPU/其它 renderer 保留现有 FPS 策略。 |
| CURSOR1 | completed | 可配置 Terminal 光标闪烁开关、Settings 预览/持久化和默认兼容 | config serde、Slint 编译、定向测试、完整 Cargo 门禁 | 默认开启；关闭后光标保持显示，不影响输入/IME |
| FOCUS1 | completed | 原生窗口失焦时将所有可见终端切换到 Unfocused FPS 上限，重新激活后恢复 pane 聚焦策略 | WindowActiveChanged、AppKit 激活同步、WindowRouter 路由回归、Slint/Cargo 门禁和双语契约 | 激活状态只在运行时维护；隐藏 Tab 仍不刷新，parser/协议应答/错误/断开/shutdown 不延迟 |
| FOCUS4 | completed | 修正 macOS 原生窗口激活状态同步，增加 `NSWindow.isKeyWindow()` UI 轮询兜底 | macOS AppKit bridge、WindowRouter 路由回归、Cargo/Slint 门禁 | 初始 100ms 周期已由 WAKE3 放宽到 500ms；事件钩子保留为快速路径。 |
| WAKE1 | completed | SSH/Telnet 输出 flush 改为缓冲区非空时才启动的一次性 16ms timer | Telnet loopback、SSH worker 回归、Cargo check/Clippy/test | 16 KiB 上限和 SSH 输入后首输出即时 flush 保持不变；空闲连接不再循环 tick。 |
| WAKE2 | completed | Local PTY reader EOF/错误主动唤醒 owner，空闲 child 检查降为 1s，并限制 EOF 后 25ms 快速确认窗口 | Local PTY 8 项、EOF 通知测试、满事件队列 shutdown/drop 回归 | 输入、resize、shutdown 仍即时唤醒；快速确认最多 40 次，之后回到低频兜底。 |
| WAKE3 | completed | macOS `NSWindow.isKeyWindow()` 激活兜底轮询从 100ms 放宽到 500ms | AppKit cfg 编译、WindowRouter 回归、完整 Cargo 门禁 | `WindowActiveChanged` 仍是快速路径；只降低平台事件遗漏时的兜底响应频率。 |
| WAKE4 | completed | 同步双语架构、项目地图和月度历史并完成质量门禁 | fmt/check/Clippy/test、tracker/Markdown/`git diff --check` | 不改 renderer、队列容量、SSH trust、凭据或终端解析。 |
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
| ROWMODEL1 | completed | 保留单层 `TerminalRenderLine` repeater 和嵌套 model identity 复用 | focused render-line tests、Slint 编译 | 应用层不再引入 tile/partition model；`TermDamage` 和 Slint 内部 dirty region 保留。 |
| ROWMODEL2 | completed | 删除应用层 tile/partition 配置、DTO、回调、测试和重复行 repeater | config/view/Slint focused tests、全量离线门禁 | 旧 JSON 字段作为未知字段忽略，不增加 schema migration。 |
| ROWMODEL3 | completed | 同步双语架构、项目地图、研究和月度记录，复核无残留符号 | tracker validator、Markdown 检查、`git diff --check` | 历史 tile 条目保留为审计记录，当前实现说明改为单层行模型。 |
| ROWMODEL4 | completed | 用 `TerminalSnapshot.dirty_rows` 驱动普通输出的按行增量 render/model 更新 | 终端 damage focused tests、Rust/Slint check、完整 Cargo tests | 首帧、resize、滚动、full damage 和渲染 key 变化保留全量回退；不改变 Slint 内部 item-tree 遍历或 backend present 语义。 |

| COLOR1 | completed | macOS CoreGraphics image 使用显式 sRGB 色彩空间，保持既有像素排列与 damage 几何不变 | `cargo check --locked --offline`、Clippy、完整测试、`git diff --check` | 仅改变色彩空间；未同时设置 `contentsFormat`，避免把像素排列和色彩空间两个变量混在一次 A/B。 |
| COLOR2 | completed | 构建 release 采样候选并由目标 macOS 对比 ICC/vImage 与 CPU | release build、Mach-O UUID/SHA-256、同窗口/pane/renderer/负载 sample | 10 秒 sample UUID `75D434E2-B1D0-3C6A-ABFB-7504F41A0663` 与当前 release 一致；仍显示 `CA::Render::copy_image`/ICC/vImage 主导，未提供独立 CPU meter，因此不声明总 CPU 降幅。 |
| CELL1 | completed | 以 Latin monospace advance 作为逻辑单元格宽度，并让选择背景按单元格边界绘制 | Slint 编译、ASCII/CJK/符号静态几何审阅、目标机光标/选区视觉验收 | 中文仍占两个逻辑格；光标、选区、鼠标命中和 IME 共用同一格宽。目标机视觉待用户确认。 |
| CELL2 | completed | software presentation region 从实际首个完整终端行开始 | Slint callback contract、Retina 物理区域换算和 backend partition tests | `grid-top-offset` 顶部余量交给 fallback，不跨入行级 block。目标机视觉待用户确认。 |
| CELL3 | completed | 完整离线门禁、双语契约和目标平台验收说明 | fmt、check、Clippy、test、tracker/Markdown、`git diff --check` | 本轮代码与新增记录通过；validator 仍报告既有历史记录格式债务，GUI 视觉由用户截图确认。 |
| CELL4 | completed | 鼠标拖选使用半开单元格边界，使最小选区精确为一格且绘制与复制一致 | Slint 编译、正向/反向/跨行范围审阅、完整 Cargo 门禁 | xterm mouse reporting 仍使用包含格坐标；双击单词和三击逻辑行继续使用 Rust 包含式范围。目标机拖选视觉待用户确认。 |
| COLOR3 | in_progress | 以可配置方式接入普通 `CALayer` delegate + `setNeedsDisplayInRect` 持久 backing store，并保留现有 `setContents` 回退 | API/objc2 审阅、12 项隔离 vendor tests、Cargo check/Clippy、GUI 视觉、同负载 sample 和残影/撕裂验收 | backing store 已提升为默认，`AXSSH_EXPERIMENT_CA_BACKING_STORE=0` 可回退；`CATiledLayer` 不在本原型内。 |
| CPU1 | completed | 新增稳定的 Software presentation 配置枚举和启动期 backend 选择契约 | config normalization/serde、启动选择单测 | schema v27 保存两条稳定路径；环境变量继续作为显式诊断覆盖。 |
| CPU2 | completed | 将两条 CPU 路径放入 Appearance > Rendering，并标明重启后生效 | Slint 编译、Settings 搜索与草稿/保存映射回归、翻译检查 | 仅 macOS Software 使用该值；不热切换已有窗口或 surface。 |
| CPU3 | completed | softbuffer 接收有界启动配置并选择 image-layer 或 backing-store 路径 | vendor focused tests、macOS cfg Cargo check | 只传一个进程内 bool，不向 backend 传 terminal/session/SSH 状态，不改变 damage 几何与图像所有权。 |
| CPU4 | completed | 同步双语契约、项目地图、月度记录并完成全量门禁 | fmt/check/Clippy/test、tracker/Markdown/`git diff --check` | 自动化门禁通过；目标 macOS 仍需分别验收两条路径的视觉和 sample。 |
| CPU5 | completed | 将 CPU 消耗较低的 damage backing store 提升为缺失/无效配置的默认值，保留显式 layer-image 回退 | config default/normalization/serde、Slint 编译、翻译与双语文档 | 不迁移或覆盖已显式保存的 `layer-images`；只影响 macOS Software surface 的下次启动。 |
| SEC1 | completed | 升级 `russh` 到 0.63.1，并适配 `PublicKeyOrCertificate` 主机密钥回调 | `cargo update -p russh --precise 0.63.1`、locked check、SSH focused tests | 覆盖 0.62.x Curve25519 客户端崩溃/拒绝服务修复，以及 0.63.1 的 channel-ID/MAC-none 稳定性修复；证书显式拒绝，不改变 profile/known_hosts 信任语义。 |
| SEC2 | completed | 将严格 Clippy、MSRV 1.92 和 RustSec 审计纳入普通 CI，并移除 vendor manifest 中已失效的 bench target | workflow YAML 审阅、`cargo fmt --all -- --check`；GitHub-hosted CI 执行 | MSRV 只做 locked Linux check；RustSec action 固定到已验证的 v2.0.0 commit SHA，仅显式忽略已接受的 `RUSTSEC-2023-0071`；不恢复 benchmark 或增加 dev dependency。 |
| SEC3 | completed | 为 Release 增加 Linux 预检和安全审计依赖 | release workflow DAG 审阅 | 跨平台构建只有在 tag 校验、fmt/check/Clippy/test、脚本回归和 RustSec audit 全部成功后才启动。 |
| SEC4 | completed | 同步双语架构、环境审计、研究和月度变更记录 | 文档链接/tracker 校验、`git diff --check` | 记录新 API 的证书拒绝边界、MSRV/CI 事实和上游 advisory 来源；历史 0.62.2 记录保留为审计快照。 |
| SEC5 | completed | 以 AppState 的进程级 Tokio persistence gate 串行化所有 `SessionStore` 写入，并以 per-profile mutation token 保护编辑/删除、认证后凭据保存和 trust 更新 | mutation token focused tests、credential rollback/commit tests | 凭据引用提交只接受当前密码 profile；不持有同步状态锁跨越 await。 |
| SEC6 | completed | 修复 revoked host-key 确认顺序，并将 known_hosts/config I/O 移出 UI 线程 | host-key phase/ordering review、Rust/Slint check | 先清 profile pin 再删 `@revoked`；删除失败保持拒绝，不把两个文件假装成单一事务。 |
| SEC7 | completed | 重连计时器只保存 profile UUID，触发时重新读取当前 profile；worker 启动拒绝陈旧快照 | reconnect/worker snapshot review and focused tests | 手动重试与自动重连都不能使用已编辑或已删除的 profile 快照。 |
| SEC8 | completed | 同步双语安全契约、项目地图、月度记录并完成完整离线门禁 | fmt、check、Clippy、test、Markdown/tracker、`git diff --check` | 保留既有 renderer/性能改动和目标 macOS 视觉验收边界。 |
| SEC9 | completed | 修复凭证 mutation 超时释放持久化闸门的竞态 | credential task soft-deadline regression、完整 Rust 门禁 | 读取保留 20s 硬超时；保存、删除和回滚只在软截止后告警，仍等待 `spawn_blocking` 实际完成再释放 gate。 |
| SEC10 | completed | Telnet/Serial worker 启动使用完整 `SessionProfile` 快照并覆盖异步串口发现窗口 | direct snapshot focused tests、完整 Rust 门禁 | 启动前和 Serial 端口发现后都比较完整 profile；过期尝试清理状态且不启动旧 worker。 |
| SEC11 | completed | 固化设置语言即时保存与预览草稿隔离语义，并同步安全架构契约 | settings regression、双语 docs、tracker/diff checks | 语言保存只写语言字段，避免把未确认的其它预览设置提前落盘；不改变现有 UI 行为。 |

## 9 项复核映射

| # | 问题 | 处理安排 | Step |
| --- | --- | --- | --- |
| 1 | Retina/跨屏 scale 变化后仍使用旧物理区域 | `ScaleFactorChanged` 按窗口重新发布逻辑区域，backend 同时核对 layer scale 并使旧 framebuffer 失效 | SPR2 |
| 2 | 每帧重复锁布局 registry 并 clone 多份 region `Vec` | 应用发布前释放 `MutexGuard`；backend 每个 buffer/present 周期只读一次 generation，只有 generation/scale 变化时 clone snapshot 并重建 layer | SPR3 |
| 3 | 应用状态通过 softbuffer 全局旁路耦合 backend | 将内容收窄为 32 窗口、每窗口 64 region 的短生命周期 opaque presentation hint；只在 macOS Software 启用，窗口释放时注销，不传 terminal/session/SSH 状态 | SPR1、SPR2 |
| 4 | `Hash`/`Hasher` 在非 macOS 构建可能成为未使用 import | 对 import 和 native window key 实现使用 `cfg(target_os = "macos")` | SPR2 |
| 5 | registry 没有窗口注销，依赖容量淘汰遗留旧 entry | 新增 `remove_presentation_layout`，覆盖 detached 恢复失败、返回、关闭、批量释放和主窗口退出 | SPR2、SPR5 |
| 6 | pane 移动、notice/grid clip、scale 与 reset 的布局失效不完整 | pane model revision、独立 notice/layout timer、scale refresh、reset 后逐 pane 重注册共同覆盖 | SPR2、SPR4 |
| 7 | 非 Software renderer 仍执行 presentation callback/timer/layout 工作 | AppWindow 显式下发 macOS Software enable，Slint revision/timer/callback 与 Rust bridge 均先门控 | SPR2、SPR4 |
| 8 | block-row 默认值/范围和 settings `set_rows` 重复 | 配置规范化作为应用唯一默认/范围来源，bridge 不再自建默认或 clamp；softbuffer public API 仅保留边界防御，Settings 统一通过 open-window apply 一次更新 | SPR3 |
| 9 | 测试只覆盖旧固定辅助函数，没有覆盖生产 damage/tile 路径 | production `dirty_tile_mask_for_tiles` 直接由 backend 与测试共用，并新增多 pane、裁剪、重叠、generation 去重和注销回归 | SPR5 |

## 本轮实施计划

- `BACKEND1`：固定上游证据和本地补丁边界，确认 1.17.1 的 `region.iter()` 与 macOS softbuffer age/present 行为。
- `BACKEND2`：引入本地 `i-slint-backend-winit` patch，提交实际多矩形 damage，而不是 bounding box。
- `BACKEND3`：核对 macOS `softbuffer` 的 buffer age、Core Animation layer 和 present 语义；实现 vendor-owned 持久 framebuffer、失效状态和固定物理像素 tile 的 damage-aware present。
- `BACKEND4`：补 backend focused 回归、第三方许可和双语架构说明，执行 locked/offline 完整门禁并记录平台验收边界。
- `BACKEND5`：以 Slint pane 几何替换终端区的固定 tile，按可配置终端行高倍数划分 presentation layer；sidebar/tab 保持 fallback grid。
- `XPLAT1`：定义 `Surface::damage_support()` 能力 DTO 和 helper，不改变 `present_with_damage` 的兼容契约。
- `XPLAT2`：为 Win32、Wayland、X11、KMS、Web、Android、Orbital 和 Core Graphics 实现运行时能力映射。
- `XPLAT3`：让 winit software bridge 在 full-frame/lock-time backend 上直接调用 `present()`，并补能力分类回归。
- `XPLAT4`：同步双语架构、项目地图、月度历史和验证边界，完成静态、离线和可用目标检查。

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| BACKEND1 | completed | 锁定 Slint/winit/softbuffer 证据与 patch 边界 | 本机 crate 源码、PR #12758、macOS sample 对照 | Slint 1.17.1 的 winit bbox 与 macOS softbuffer age=0/full CGImage 已确认。 |
| BACKEND2 | completed | winit software 多矩形 damage forwarding | vendored backend compile、locked check | `PhysicalRegion::iter()` 逐项映射为 softbuffer Rect。 |
| BACKEND3 | completed | macOS softbuffer 持久 framebuffer 与 damage-aware CoreAnimation presentation tiles | macOS 条件编译、buffer-age/invalidate tests、locked check | 固定 256×128 物理像素 tile；有效帧 `age() == 1`；首帧/resize/Retina/restore 强制重新绘制和全 tile 提交；不引入应用层 tile/partition。 |
| BACKEND4 | completed | 文档、许可、focused/完整门禁和平台验收说明 | fmt/check/Clippy/test/translation/diff、vendor rustfmt、用户 macOS 视觉确认 | 已撤销会造成坐标错位的 tile backend；CPU/footprint A/B 仍单独评估。 |
| BACKEND5 | completed | 每窗口有界 pane geometry DTO、1-16 行设置和 row-aligned Core Animation presentation layer | config normalization、Slint 编译、backend partition test、Cargo check/Clippy/test、翻译和 diff | 默认 4 行；终端 layer 横跨 pane，不跨 pane；sidebar/tab/空白区使用 256×128 fallback；新布局视觉验收待用户执行。 |
| XPLAT1 | completed | 定义 `DamageSupport` 枚举、partial/full helper 和 `Surface` 查询入口 | softbuffer focused unit test、Rust API 审阅 | `present_with_damage` 保持向后兼容；能力只描述当前 surface 的消费方式。 |
| XPLAT2 | completed | 实现各平台 backend 的矩形、bounding、tile、driver-dependent 和 fallback 映射 | macOS Core Graphics 编译；可用目标的 cross-check | 非当前目标平台由对应 cfg/CI runner 编译；KMS 运行时收益仍由驱动决定。 |
| XPLAT3 | completed | winit 按能力选择 damage 提交或完整提交 | winit/softbuffer compile、能力分类测试 | full-frame 与 lock-time 不构造无效的局部提交；空 damage 和 age=0 首帧语义不变。 |
| XPLAT4 | completed | 双语契约、项目地图、月度记录和质量门禁 | tracker、Markdown、fmt/check/Clippy/test/diff 检查 | 目标平台 GUI 与真实 driver 行为仍属于平台验收，不由离线构建替代。 |
| WS1 | completed | 为有界 workspace 快照增加菜单保存/打开、路径弹层和异步替换流程 | workspace persistence tests、Slint 编译、完整 Rust/Slint 离线门禁、目标平台菜单与多窗口视觉验收 | 快照只含 Tab/layout/有限终端文本，不含凭据或 live handle；打开前先停止旧 worker，文件 I/O 不阻塞 UI。目标平台菜单、路径输入和 detached 多窗口视觉仍由用户验收。 |
| FONT1 | completed | 修复自带终端字体异步注册后粗体布局仍复用普通回退字体的问题 | Fontique 字重注册回归、Slint 编译、完整 Rust/Slint 离线门禁、`git diff --check` | 字体注册代次驱动终端 `font-weight` 绑定和工作区刷新，同时同步主窗口与 detached 窗口；不改变字体族选择或字体资源。 |
| KPAD1 | completed | Windows 物理数字小键盘映射、终端 application-keypad 模式读取和 DEC/xterm 应用小键盘编码 | 输入/编码/终端模式定向回归、host Cargo check；Windows target 仍待 CI/目标机 | 补充目标 ID：`20260901-windows-keypad-input`；只在远端 `ESC =` 模式且无修饰键时截获，普通 NumLock/IME 路径保持不变。 |
| KPAD2 | completed | 双语输入契约、项目地图、月度记录和离线质量门禁 | fmt/check/Clippy/test、tracker/Markdown、`git diff --check` | 不改变 SSH transport、host-key trust、凭据或持久化。 |
| SHORT1 | completed | 在设置页展示所有应用层快捷键，包括固定的平台快捷键 | Slint 编译、设置搜索回归、翻译检查和完整 Cargo 门禁 | 可配置快捷键保持现有保存契约；固定的 Terminal Select All、Previous Tab、Next Tab 只读展示。 |
| INPUT1 | completed | 统一普通输入框的复制/粘贴入口，并为密码输入提供安全的粘贴菜单 | Slint 编译、输入组件静态审阅、完整 Cargo 门禁 | 普通文本/路径/编辑器支持系统 Copy/Cut/Paste/Select All；SecretTextInput 仍禁止复制，仅允许粘贴，不改变凭据生命周期。 |
| CRED1 | completed | 禁止加密保险库保存缺少口令时静默降级到系统密钥库，并同步认证与会话编辑器入口 | 定向凭据回归、Slint 编译、翻译检查、fmt/check/Clippy/完整 test、`git diff --check` | 所选加密保险库后端缺少非空口令时直接拒绝保存；不迁移或删除已有系统凭据。 |

- `ROWMODEL1`：保持单层 `TerminalRenderLine` 和 nested run/background/decoration model 的稳定 identity。
- `ROWMODEL2`：移除应用层 tile/partition 链路；旧配置字段由 Serde 忽略，不做 schema migration。
- `ROWMODEL3`：完成双语架构、项目地图、研究和月度历史同步，并运行 tracker/Markdown/diff 检查。
- `ROWMODEL4`：使用上游 `TermDamage` 产生变化行号，普通输出只更新对应行；首帧、视口变化和渲染 key 失效回退整屏可见行。
- `CRED1`：加密保险库保存必须提供非空保险库口令；认证弹窗和会话编辑器都拒绝无口令保存，不再把所选后端改写为系统密钥库。

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
- 已完成 CRED1：凭据保存策略不再把缺少保险库口令的加密保险库选择降级为系统密钥库；认证弹窗和会话编辑器均返回明确错误并保留当前 UI 输入。同步简体中文翻译映射与目录；既有 `system-keyring` profile 未被自动迁移或删除。
- 已从锁定依赖源码确认原版 macOS `softbuffer` 0.4.8 每帧报告 buffer age 0，`present_with_damage` 忽略 damage 并将完整 `CGImage` 设置到单个 `CALayer.contents`；本地 patch 现在保留持久 framebuffer、`age() == 1` 和失效传播，并把 damage 映射到有界 Core Animation presentation layer。终端 pane 按设置的终端行高倍数分区，sidebar/tab/空白区使用固定 256×128 物理像素 fallback。winit patch 保留 Slint 产生的每个独立 damage rectangle，不再先合并 bounding box；macOS backend 只替换相交 layer。
- 已用统一 `TerminalPresentation` 接入 Local、Serial、SSH 与 Telnet monitor：无 dirty 输出时不创建 timer deadline；focused 首个脏更新立即呈现，连续输出前 500 ms/到 2 秒/超过 2 秒分别采用 16/33/50 ms，安静 250 ms 后重置；活动 split tree 中未聚焦 pane 按 Appearance 的 FPS 上限呈现（默认 4 FPS，范围 1-120），隐藏 Tab 无 deadline。`WindowRouter` route revision 和 policy watch 会唤醒有 pending 输出的 monitor，焦点、Tab 或设置变化后立即按新策略重算；SSH 合并批次保留最早 `received_at`，parser、协议应答、错误、断开和 shutdown 仍走即时路径。
- 已生成双策略 ARM64 release 候选（36,376,960 bytes，inode `16356094`，SHA-256 `f419bfbcf7b50e3431062b7b78d5b3053e238265dd6133b5c5a23814ec8d291f`，Mach-O UUID `792FB118-6118-31F4-9359-CA56B5692B8D`）。检查时运行中的 PID 94454 仍映射旧 inode `16354239`，必须退出并重启后才会运行新候选。
- 已将 schema 提升到 25，并贯通默认开启的 `terminal_compact_rendering` 与默认关闭的 `terminal_row_render_cache`：Settings 草稿可即时预览并在关闭时保存，所有存活窗口同步更新；旧文件缺字段时采用默认值。
- 已让 Rust renderer 为每个有界可见行生成合并后的非默认背景 span 和 underline/strikethrough 装饰 span。Slint 紧凑分支直接绘制 Text，旧分支保留为 A/B；可选 `cache-rendering-hint` 只包住静态行内容，选区、光标、目标高亮和 IME/preedit 留在层外。
- 已生成可配置渲染优化的 ARM64 release 候选（36,492,784 bytes，inode `16358516`，SHA-256 `ca1cffe72761baa1c481e9601ff8e07b6f18d5c7f749eaa5c910ad2bcc9a09b6`，Mach-O UUID `8ECE3718-6E3D-370B-94F5-193A455BE533`）。检查时没有运行中的 AxSSH 进程，下一轮可直接启动该候选。
- 已将 `terminal_compact_rendering` 与 `terminal_row_render_cache` 两个开关从 Settings > Terminal 移到 Settings > Appearance 的 RENDERING 分组，与 Renderer 选择同区；Settings 搜索目录和双语 usage 文档同步改为 Appearance 归属，配置字段与默认值不变。
- 已将聚焦与可见未聚焦终端呈现周期改为 `focused_terminal_refresh_fps` / `unfocused_terminal_refresh_fps`，schema v26 默认分别为 60/4 FPS，范围限制为 1-120；Appearance > Rendering 使用 SpinBox，Settings preview/save 同步所有窗口并通过 `WindowRouter` policy watch 立即唤醒 pending monitor，聚焦连续输出的 16/33/50 ms 自适应仍保留。
- 实际 `winit-software` renderer 的可见输出不再使用固定 FPS timer 或窗口物理像素/分屏档位：输出立即进入 `AppState` 单槽 refresh gate，UI 取 batch 前的请求合并到同一批，snapshot 构造后才到达的输出最多补排一次。pending snapshot 在 UI 消费时与更晚的 `TermDamage` 合并，保留脏行并让 UI 看到最新状态；无 dirty row 且 cursor/viewport/mouse-reporting 未变的初始 snapshot 不进入 Slint queue。GPU 和其它 renderer 保留 16/33/50 ms 与持久化 FPS 上限。macOS CoreGraphics 现在只为 damage 相交的 presentation layer 创建独立 `CGImage`，各 layer 的 provider 仍需独立所有权以覆盖 Core Animation commit 后异步读取；若要移除这次 layer copy，仍需 IOSurface/Metal 或可回收多缓冲设计。
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
- 已删除应用层终端 tile/partition 链路：`TerminalGrid` 恢复单层 `render_lines` repeater，Rust 只保留按行 revision/render-key 的 outer line 与 nested run/background/decoration model 复用。
- 已将终端选区改为固定逐格 fill；每个选中列严格使用统一逻辑 cell geometry，不从混合 Unicode advance 推导背景边界。固定行 tree、稳定 nested model identity 和行 revision 复用继续由 Skia/Software 共用。
- 已修正 Local selection priority 下的 reporting 单击：普通左键按下先保留为本地拖选候选，若释放前未跨格移动，则在释放时向已启用 mouse reporting 的 TUI 发送 press/release；一旦移动仍保持本地选区，Alt/Option 和标准 xterm 模式的既有路由不变。
- `TerminalModel` 的 `TermDamage` 仍只负责终端 snapshot 的受损行重建；Slint renderer 的内部 dirty region 仍由框架负责，二者不再被包装成应用层 tile partial-present 链路。
- 已实现 macOS Software 持久 framebuffer 与 pane-aware CoreAnimation presentation layer：`TerminalPane` 发布窗口相对逻辑几何和实测行高，Rust 按 Retina scale 转为物理区域，backend 在每个终端 pane 内按 1-16 行倍数分带并让每带横跨 pane；sidebar/tab/空白区域保留 256×128 fallback。`present_with_damage` 仅复制 damage 相交 layer 的行并创建独立拥有的 `CGImage`，未变化 layer 保留前帧图像。有效帧返回 `age() == 1`；首帧、resize、DPI/设置/分屏几何变化、occluded/restore 和 surface invalidate 强制重建或完整更新。
- 已完成 session `01a03ba9-f5bb-7cc0-a7c9-021688da1471` 的 9 项复核修复：Retina scale 重算、布局 registry 主动注销、单周期 generation 读取、macOS cfg/Software 门控、pane/notice/reset 失效、Settings 重复更新清理、生产 damage/multi-region 测试和几何 bridge 边界文档均已落地；未改 terminal parser、worker、SSH trust、凭据或 FPS 策略。
- 已新增独立的 `ax_ssh-crash.log`：进程 panic hook 同步记录 panic 消息、位置、线程/进程/平台元数据、renderer 环境和强制 Rust backtrace，随后仍调用默认 panic hook；文件使用私有权限，与缓冲滚动运行日志分离，以保留 Objective-C callback abort 前的首段报告。
- 已完成 COLOR1：macOS CoreGraphics backend 将 `CGColorSpace::new_device_rgb()` 改为显式 `kCGColorSpaceSRGB`，不改变 BGRA/32-bit bitmap 声明、tile 几何、damage mask 或图像独立所有权；同负载 sample 显示 ICC/vImage 仍存在。
- 已完成 COLOR2 采样：目标 sample UUID `75D434E2-B1D0-3C6A-ABFB-7504F41A0663` 与当前 `target/release/ax_ssh` 一致；主线程热点仍在 `CA::Render::copy_image`、ICC/vImage。当前 release SHA-256 为 `f838a4c3b9893f93b143091622f7e212e534d1438c52b7bab1347eb9addaacd4`；此前 14:20 记录的 `17C09EF9...` 是更早候选，不是该 sample 的进程。
- 已完成 COLOR3 原型代码并将其提升为默认：现有 pane/fallback layer 几何和排序不变；每个 layer 的 retained delegate 持有同步像素 backing，present 只复制 tile-local damage 并按 Retina scale 调用 `setNeedsDisplayInRect`，delegate 只为 CGContext clip 创建独立图像。`setContents(CGImage)` 保留为显式回退；重建/析构前清除 weak delegate，不直接共享可变 provider 内存。
- 已完成 CELL1/CELL2：TerminalPane 使用主字体 50 个 Latin cell 的 advance 作为唯一逻辑格宽，中文和盒线保持独立 run 并在一格或两格内居中；TerminalGrid 选区恢复固定逐格 fill。Software presentation 只注册从 `grid-top-offset` 开始的完整底对齐行，顶部小数余量和不足三行的 pane 留给 fallback 分区。
- 已完成 CELL4：TerminalGrid 的 pointer callback 同时携带所在格与最近插入边界；TerminalPane 用行优先半开索引保存鼠标拖选，并只在绘制/复制前转换为同一包含式首末格。单格、反向和跨行拖选共用该路径；远端 mouse reporting、双击单词和三击逻辑行的既有契约不变。
- 已完成 WAKE1-WAKE3：SSH/Telnet 的 16ms flush 只在输出缓冲区非空时启动一次；Local PTY reader 关闭会通过有界命令通道唤醒 owner，空闲 child 轮询从 25ms 降为 1s，并把 25ms 退出确认限制在最多一秒；macOS 激活事件继续走快速路径，`isKeyWindow` 兜底从 100ms 降为 500ms。
- 已完成 XPLAT1-XPLAT4：`softbuffer::Surface::damage_support()` 将 native presentation 能力显式分类为矩形、bounding rectangle、tiles、driver-dependent、full-frame 和 lock-time；Win32/Wayland/X11/KMS/Web/Android/Orbital/Core Graphics 各 backend 返回运行时能力，winit software bridge 对 full-frame/lock-time 路径直接使用 `present()`，不改变既有 `present_with_damage` 兼容契约。

- 已完成 FONT1：保留自带字体族与四种 JetBrains Mono 字重的注册方式；新增字体注册代次，使异步字体加载完成后终端 `font-weight` 绑定失效并重建布局，避免普通回退字体布局缓存吞掉 Bold/Italic；主窗口和已分离窗口同步该代次，字体注册仍不进入终端/SSH 状态边界。

## 验证

- 已完成：SPR1-SPR5；`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 209、应用 200、Doc tests 0）、Retina 换算 focused test、去除未缓存 bench dev-dependencies 的隔离 vendor 单测（8 项）、435 条翻译、Rustfmt 源码差异检查和 `git diff --check` 通过。
- COLOR1 已完成：`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 209、应用 201、Doc tests 0）和 `git diff --check` 通过；`cargo fmt --all -- --check` 仍被仓库既有缺失的 `vendor/softbuffer/benches/buffer_mut.rs` 阻断，已单独用 rustfmt 校验修改后的 backend。
- COLOR3 代码验证：修改后的 backend 已通过独立 rustfmt、`cargo check --locked --offline`、严格 Clippy、隔离 vendor `--lib` 11 项测试、根工程完整 410 项测试和 release 构建；新增测试覆盖跨 tile damage 的局部裁剪、完全无交集矩形、2x Retina point 换算和 macOS tile-local Y 坐标往返。最新 release UUID 为 `3870BAB3-16BE-387A-AAA3-239B2512B595`，SHA-256 为 `7b2bf745ad26f955a90fd2bb31b9ccd67ad7fca7f4b7007a02b6d9aad9df63eb`。隔离完整 doctest 仍被 vendor 既有缺失的 `examples/utils/winit_app.rs` 阻断。
- CELL1-CELL3 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 210、应用 201、Doc tests 0）、Slint `ui/app.slint` 重编译、release 构建和 `git diff --check` 通过。ARM64 release UUID 为 `3FED0FF3-7C78-391C-B769-A04F7EFA6E97`，SHA-256 为 `b16c9387957656b7154dde6c887040ebc5df6b08b449bb6483269344d621745b`；tracker validator 仍报告本月旧历史/research 条目格式债务，目标 macOS 光标、选区、Retina 和 backing-store 视觉由用户确认。
- CELL4 已完成：定向 selection 测试 4 项及 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 210、应用 201、Doc tests 0）和 Slint `ui/app.slint` 重编译通过。
- WAKE1-WAKE4 已完成：Local PTY 定向测试 8 项、Telnet 定向测试 4 项，以及 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 211、应用 201、Doc tests 0）和 `git diff --check` 通过。
- CPU5 已完成：Software presentation 定向测试 3 项、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 212、应用 202、Doc tests 0）、440 条翻译、Markdown 相对链接和 `git diff --check` 通过。ARM64 release UUID 为 `9D2C3BF8-C51E-3E23-940E-F8DC3F49657E`，SHA-256 为 `12593fd84b5bdb361118b01238d8512f78cb3c304deac44ff439543eb189c846`。
- XPLAT1-XPLAT4 已完成：能力枚举/helper、各平台 cfg 映射、winit full-frame fallback、双语架构/开发说明和项目地图已同步；macOS 目标通过本地编译，Windows 交叉检查受本机未缓存 `atomic-waker 1.1.2` 阻断，Linux/Android/Web/KMS/Orbital 需由对应 CI 或目标设备完成编译和运行时验收。
- 已完成 SEC5-SEC11：AppState 的进程级 Tokio persistence gate 串行化 Settings、profile/group/import、认证后凭据与 host-key 等完整 `SessionStore` 写入，per-profile mutation token 保护 profile 专属事务；凭据读取保留硬超时，保存/删除/回滚在软截止后仍等待 blocking 操作完成再释放 gate；凭据引用要求当前密码 profile，revoked 确认先清 pin 再尝试删除撤销记录，known_hosts/config I/O 在 blocking task 执行；自动重连按 profile UUID 触发时读取最新配置，SSH/Telnet/Serial worker 都拒绝陈旧快照，Serial 在异步发现后再次验证。语言即时保存只写语言字段，保持其它 Settings 预览草稿不提前持久化。同步双语契约、项目地图和月度记录。
- 未完成：Windows offline check 因本机未缓存 `atomic-waker 1.1.2` 未执行。tracker validator 本轮 current/新增记录字段完整，仍报告既有 2026-08 历史与 research 条目格式问题。目标 macOS GUI/Retina 拖选视觉和同负载 CPU/footprint A/B 由用户执行。

- FONT1 已完成：Fontique 四字重注册测试、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 `cargo test --locked --offline`（库 215、应用 212、Doc tests 0）和 `git diff --check` 通过；目标平台粗体/斜体视觉仍需用户验收。
- CRED1 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 226、应用 216、Doc tests 0）和 `git diff --check` 通过；用户日志对应 profile 当前仍显式引用 `system-keyring`，需重新保存并提供保险库口令才能迁移。

## 风险与阻塞

- 当前 worktree 已包含用户未提交的终端边距、几何和双语文档改动；本目标必须在其基础上追加并避免覆盖。
- UI 刷新批处理只能延后呈现通知，不能延后 terminal parser、PTY protocol response、错误、退出或 shutdown。
- 脏终端路由必须在 pane 转移、split、关闭和 detached/return 结构变化时回退到完整刷新，避免更新错误窗口或陈旧 UUID。
- release 为 strip-symbols 产物，系统级 DisplayLink/Metal 和线程阻塞结论可靠，但不能从该二进制的 sample 进一步精确拆分应用内部 Rust 函数。
- GPU/其它 renderer 的 focused 持续输出仍按 16/33/50 ms 与 Appearance FPS 上限节流；Software 改为单槽 latest-frame 背压，该呈现调度与应用层 tile/partition 已解耦。
- macOS software surface 现在通过 pane-aware CoreAnimation presentation layer 提交 Slint damage 相交区域；CPU framebuffer 持久复用，但每个被更新 layer 都有独立 `CGImage` 后备，不能让 compositor 与下一帧写入竞争。Software 不以窗口像素/分屏数强制限速；GPU/Skia 路径仍按普通 CAMetalLayer drawable present，需分别 A/B。
- 双呈现策略必须按当前 `WindowRouter`/`PaneTree` 动态读取焦点，不能在 worker 中缓存 pane 归属；Software 可见输出立即进入 latest-frame gate，GPU/其它 renderer 在新 deadline 内追上，隐藏 Tab 仍不应进入 Slint event queue。
- 窗口资源清理必须先停止应用 worker，再清空 detached/main 的 Slint model 并移除强引用；不能把清理延迟到 timer 或依赖函数作用域自然 drop，否则窗口 renderer surface 和 model 可能继续存活。
- rust-skia release build script 在 release cache 缺失时会绕过 Cargo offline 语义尝试下载预编译包；本轮最终构建复用了本机同名、同 SHA-256 的 debug cache，未把该缓存加入 Git 或项目依赖。
- resize-only 刷新仍必须在 pane 转移、split、关闭和 detached/return 结构变化时由 WindowRouter 回退到 full refresh；本轮不改变该结构变化语义。
- 应用层 row model 更新只负责减少 Slint item/model 工作；macOS softbuffer 将 winit 物理 damage 映射为 backend presentation tile，但不把 `TermDamage` 变成应用层 tile 或改变 UI 坐标。升级 Slint/softbuffer 后仍需重新核对上游 damage、buffer age、CoreAnimation layer 和 DPI 语义。
- Slint 1.17.1 的 `cache-rendering-hint` 只在 Skia/FemtoVG layer renderer 中保留离屏图像；software renderer 不提供等价 layer cache。缓存必须只包住静态终端行内容并默认关闭，避免光标闪烁或选区变化使所有行缓存失效，也避免未测量的 Retina 纹理占用成为默认成本。
- COLOR1 的 sRGB sample 已完成；`CA::Render::copy_image`/ICC/vImage 仍存在，只能说明色彩空间提示未消除 CA 的内部准备成本，不能据此宣称局部 backing store 无效。`setNeedsDisplayInRect` 方案还需保证 delegate 绘制期间读取不可变帧数据，禁止直接改写仍被 CA 使用的 `CGImage` provider 内存。
- `DamageSupport` 是 presentation capability hint，不是各平台性能保证：KMS 由驱动决定，Wayland/X11 能力随运行时协议/共享内存状态变化，Android 仍要求在 lock 前决定 damage。`present_with_damage` 保持安全的兼容入口；新增 winit fallback 只跳过无法消费的局部 damage 生成，不改变 framebuffer、终端模型、凭据或 SSH trust 边界。

## 下一步

- 已完成终端显式 `Follow`/`Detached`/`AlternateScreen` 视口策略和双宽光标跨度快照；Detached 输出保持历史位置，输入回底，alternate screen 清理本地滚动状态。中文续格光标归一化到首格并按两格绘制。返回底部/未读输出 UI 尚未增加，目标平台视觉验收待用户执行。
- 用相同窗口、pane 数、renderer 和持续输出对照当前单层行模型与 GPU/Skia；不再维护旧 tile/partition A/B 组合。
- 在目标 macOS 上先验收 1/4/8/16 行设置、sidebar 展开/收起与拖宽、横竖分屏、detached 窗口、Retina scale、resize/隐藏恢复的边界和内容顺序，再进行同负载 GPU/Skia 与 Software 对照。
- 构建同一 release 后，以默认 damage backing store 和 `AXSSH_EXPERIMENT_CA_BACKING_STORE=0` 的 layer-image 回退分别验收首帧、1/4/8/16 行、sidebar 状态/拖宽、分屏、Retina、resize、隐藏恢复、滚动、光标和持续输出；后续 release 仍用相同窗口/pane/负载记录 CPU meter 与 10 秒 sample，防止性能回归。
- 若普通 delegate 再次出现整块 CA copy 或视觉回归，先切换到 layer-image 基线分析 sample；`CATiledLayer` 的异步延迟和禁止直接设置 `contents` 使其只能作为后续独立实验。
- 在目标 macOS 上先复验新格宽与 row-origin：长 ASCII、中文/盒线混排、跨行选区、光标闪烁、1/4/8/16 行 block、Retina/resize 和 split 边界；确认无错位后继续 COLOR3 的 backing-store/layer-image A/B。
- 单格拖选需要在目标 macOS 上复验同格跨中线、向左/向右反向拖动、跨软换行与硬换行、双击单词、三击逻辑行，以及 Copy selection on select；视觉与复制内容都正确后再恢复 COLOR3 A/B。
- 使用同一 release 分别记录空窗口、一个空闲 Local、一个空闲 SSH/Telnet 和两个 Local pane 的 Activity Monitor/Instruments Idle Wake Ups；实际系统唤醒会受 Tokio timer 合并和 macOS power management 影响，不用源码 timer 数量直接替代测量值。

## 最后更新时间

- 2026-09-03 10:50 +0800：完成 CRED1；修复选择加密保险库但未填写保险库口令时静默改用系统密钥库的问题。认证弹窗和会话编辑器现在拒绝该保存请求，更新双语架构说明与界面提示；既有 system-keyring 凭据保持不变，完整 Rust/Slint 门禁通过。
- 2026-09-01 21:57 +0800：完成 SHORT1/INPUT1；Shortcuts 展示三个固定平台快捷键，普通文本输入和 TextEdit 使用完整原生剪贴板操作，SecretTextInput 仅新增粘贴入口并保持秘密不可复制。完整 Rust/Slint、翻译和差异门禁通过；目标平台输入法、快捷键和菜单视觉待用户验收。
- 2026-09-01 15:55 +0800：完成 MODAL1 阻塞式 dialog 统一与窗口路由锁定；共享 `ModalFrame`、`OverlayHost` 安全优先仲裁和 Rust 侧 Tab/Pane/workspace 动作复核已接通，完整离线门禁通过，目标平台焦点/菜单验收待用户执行。
- 2026-09-01 18:37 +0800：完成 KPAD1-KPAD2；Windows 在远端 `ESC =` application-keypad 模式下编码无修饰物理数字小键盘，普通 NumLock/IME/快捷键路径不变。host 离线门禁通过；Windows target 构建和实机键盘验收仍待 CI/目标机。
- 2026-09-01：修复异步自带字体注册后的终端粗体/斜体布局缓存失效；FONT1 已完成，主窗口与 detached 窗口同步字体注册代次，完整离线 Rust/Slint 门禁通过，目标平台视觉待用户验收。
- 2026-08-28 11:30 +0800：增加 File 菜单 workspace 保存/打开、非阻塞路径弹层、用户路径有界原子读写，以及打开前 worker/probe 清理、Tab UUID 重映射和 detached route 替换；WS1 已完成，目标平台视觉待用户验收。
- 2026-08-28 14:38 +0800：完成 SEC5-SEC11，收敛 session `01a040ac-d2f1-7fc2-b033-cc1c58f5b4ca` 复核出的 credential timeout、Telnet/Serial 旧快照和设置预览语义问题；完整验证结果见本轮月度记录。
- 2026-08-28 09:18 +0800：完成 XPLAT1-XPLAT4。新增 `Surface::damage_support()` 跨平台能力探针和 backend 映射；winit software bridge 对 full-frame/lock-time backend 直接走 `present()`，并同步双语架构、开发文档、项目地图和月度历史。macOS 本地门禁已通过，Windows offline check 受未缓存 `atomic-waker 1.1.2` 阻断，其他平台交由对应 CI/设备验收。
- 2026-08-27 13:49 +0800：按用户完成采样后的选择，将 macOS Software 的 damage backing store 提升为缺失/无效配置默认值；显式保存的 `layer-images` 和环境变量假值仍可回退，不改变 GPU、FPS、终端或 SSH 边界。
- 2026-08-27 13:05 +0800：完成 CPU1-CPU4。schema v27 保存 macOS Software 的稳定图层图像/实验脏区 backing store 选择；Appearance > Rendering、Settings 搜索/保存、中文目录和启动期 softbuffer 开关已贯通，环境变量保留为进程覆盖。完整 Rust/Slint/翻译门禁及 12 项独立 softbuffer 测试通过，等待目标机分别进行视觉和同负载 sample A/B。
- 2026-08-27 11:04 +0800：SSH/Telnet 空闲输出 timer 改为按需一次性 flush；Local PTY 空闲 child 检查降至 1s，reader 关闭即时唤醒并只短暂快速确认；macOS 激活兜底放宽到 500ms。完整 Rust/Slint 门禁通过，等待同 release Idle Wake Ups 复测。

- 2026-08-27 10:03 +0800：鼠标拖选改为最近插入边界和行优先半开索引，最小选区可精确为一个单元格；绘制与复制只消费同一规范化包含式范围，xterm mouse reporting 和语义/逻辑行选择不变。完整 Rust/Slint 门禁通过，等待目标机拖选视觉确认。

- 2026-08-27 09:47 +0800：修复终端光标/选区与文字的格宽漂移，并校正 macOS Software presentation 的真实首行原点。主字体 Latin advance 作为唯一逻辑 cell width，非 ASCII run 仍在固定 span 内居中；选区按 cell 绘制；presentation region 从 `grid-top-offset` 开始，只覆盖完整终端行，顶部余量由 fallback 接管。完整离线门禁和 release 构建通过，等待目标机视觉复验。

- 2026-08-26 18:06 +0800：修复 COLOR3 实验路径的 macOS tile-local Y 坐标。damage 从左上原点物理像素转为 CALayer 左下原点 point，delegate clip 再执行逆变换；完整 block、顶/底边和 Retina 往返测试通过，等待目标机视觉复验。
- 2026-08-26 16:45 +0800：完成 COLOR3 release 候选。ARM64 UUID `647E1512-E87E-38A9-B28F-3CFC98BA0A7F`、SHA-256 `696523a6a94bd8565861283c13289b7912e369a1991360a91a3a53fa5450f92b`；同一二进制可用环境变量切换默认/实验路径，等待目标机视觉和 sample A/B。
- 2026-08-26 16:21 +0800：完成 COLOR3 可回退原型和静态门禁。`AXSSH_EXPERIMENT_CA_BACKING_STORE=1` 为每个现有 presentation layer 接入 retained `CALayerDelegate`、同步像素 backing 和 tile-local `setNeedsDisplayInRect`；默认 `setContents` 不变。focused 测试发现并修复了无交集 damage 在局部坐标减法前被提前求值的下溢问题；目标机视觉/性能 A/B 尚未执行。
- 2026-08-26 15:00 +0800：完成 COLOR3 方法检索。Apple 公开 API 支持 `CALayerDelegate.drawLayer:inContext:` + `setNeedsDisplayInRect:` 的矩形失效；`CATiledLayer` 支持异步 tile 绘制但官方要求不直接设置 `contents`，且更新可能延迟。结合当前 softbuffer ownership，首选普通 delegate backing store，CATiledLayer 只做独立实验。
- 2026-08-26 14:32 +0800：COLOR2 sample 完成。10 秒目标 macOS release sample 与当前候选 UUID 一致；主线程 DisplayLink/CA 图像准备和 ICC/vImage 仍为主要活跃路径，COLOR3 保持待实施。
- 2026-08-26 14:20 +0800：COLOR2 release 候选完成。`target/release/ax_ssh` UUID 为 `17C09EF9-E362-38C9-9F09-07E3A0DB5F0F`，SHA-256 为 `d993796031b219d7a2af05870ffdd33600384691755db4ce2ffb716eb2c4897c`。
- 2026-08-26 14:00 +0800：COLOR1 完成。macOS CoreGraphics image 使用显式 sRGB 色彩空间，未改变像素排列、damage 几何或图像所有权；check、Clippy、完整 410 项测试和 diff 检查通过。
- 2026-08-26 13:35 +0800：完成 software presentation 9 项复核修复及 SPR1-SPR5。macOS Software 专属 bridge 现在覆盖 scale、pane/notice/reset 和窗口注销；backend 每个 buffer/present 周期只读一次 generation，只有布局/scale 变化才 clone snapshot；生产 damage/multi-region/registry 与 Retina 换算回归通过。Software 仍无固定 FPS 上限，目标平台视觉验收待用户执行。
- 2026-08-26：新增 macOS Software row-aligned presentation layout。每个 `TerminalPane` 把 window-relative 几何与实测行高发布到有界 Rust DTO，backend 按设置的 1-16 行（默认 4）划分横跨 pane 的 layer；sidebar/tab/空白区域保留 fallback grid。设置即时同步主窗口和 detached 窗口，不增加 FPS 上限；Cargo check/Clippy/409 tests、翻译和 diff 检查通过，目标平台视觉/性能 A/B 待用户执行。
- 2026-08-25 16:30 +0800：Software renderer 的可见终端输出取消固定 FPS timer 和物理像素/分屏负载档位，改为 `AppState` 单槽 latest-frame refresh gate；UI 消费 pending snapshot 时合并随后发生的 `TermDamage`，且只有 snapshot 构造后的新输出才补排一次。控制序列等没有可见变化的初始 snapshot 仍不进入 Slint event queue；macOS CoreGraphics 的安全图像所有权边界保持不变。
- 2026-08-25：macOS softbuffer `present_with_damage` 改为固定 256×128 物理像素 CoreAnimation tile。持久 CPU framebuffer 仍由 Slint 重用；仅把 damage 相交 tile 的像素行复制到独立 `CGImage` 并替换对应 layer 内容，旧 tile image 留给 compositor 安全持有。首帧、resize、surface invalidate 和 Retina scale 变化仍完整更新；不增加 FPS 限制。
- 2026-08-24：删除应用层终端 tile/partition 链路及配置接线；旧 JSON 字段作为未知字段忽略，当前 UI 使用单层 `TerminalRenderLine` model，保留 `TermDamage`、行 revision 复用和 Slint 内部 dirty region。
- 2026-08-24：撤销 macOS softbuffer 的 512x64 CoreAnimation tile/slicing；保留持久 framebuffer、`Surface::invalidate()`、`age() == 1` 和 age 0 首帧/恢复后的完整 layer present。生产代码通过 vendor/backend Cargo check；目标机仍需按首帧、resize、Retina、隐藏/恢复、滚动、光标和动画清单进行视觉验收。
- 2026-08-24：为 macOS 偶发键盘/IME abort 增加独立 `ax_ssh-crash.log` 同步 panic 报告，覆盖 panic 消息、源码位置、线程/平台元数据和 backtrace；不改变 renderer、终端、SSH 或凭据边界。
- 2026-08-25：`TerminalSnapshot` 新增 `dirty_rows/full_refresh`；普通输出在应用层只渲染并通知受损行，首帧、视口/尺寸变化、full damage 和渲染 key 变化保留整行回退。Slint item-tree 遍历与 backend 完整 present 语义不变。
- 2026-08-23 22:50 +0800
- 计划状态变更：BACKEND1: pending -> completed; BACKEND2: pending -> completed; BACKEND3: pending -> completed; BACKEND4: in_progress -> completed
- 验证结果：本地 backend patch 已清理死代码并通过 vendor rustfmt、locked/offline Cargo 全量门禁、翻译检查和 `git diff --check`；实际 macOS GUI/A-B 仍待用户执行。
- 计划切换：ROWMODEL1-ROWMODEL4 已完成；ROWMODEL3/4 在本轮完成文档同步、增量模型更新和门禁。
- 影响文件：`src/config/{settings,tests}.rs`、`src/app/{settings_bridge,view/settings,view/terminal,view/tests}.rs`、`ui/{app,settings,settings/appearance,workspace-shell}.slint`、`translations/zh-CN/LC_MESSAGES/ax_ssh.po`、双语架构/使用文档和 tracker。
- 计划状态变更：PARTITION1: pending -> completed; PARTITION2: pending -> completed; PARTITION3: pending -> completed; PARTITION4: in_progress -> completed
- 验证结果：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 202、应用 197、Doc tests 0）、`python3 scripts/build_zh_catalog.py`、`python3 scripts/check_translations.py`、tracker validator 和 `git diff --check` 通过。
- 对 plan 的更新：设置默认 `tile-8`；逐行/8 行/16 行只改变 UI tile 分组，保留 dirty-row revision、动态 `start_row` 几何以及终端 parser/worker/输入边界。
