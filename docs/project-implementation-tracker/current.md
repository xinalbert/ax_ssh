# 当前项目实施记录

## 当前目标

- 目标 ID：20261015-windows-wheel-diagnosis
- 目标：定位 macOS 可滚动 Codex 输出、Windows 输出无法滚动但输入记录可滚动的差异，明确原生测试与实际 GUI 复现的证据边界。
- 交付物：原生 CI 证据核对、滚轮链路脱敏诊断、主/独立窗口到真实 SSH 接收字节回归、双语复现方法、验证记录和分组本地提交；最终根因须由故障主机数据确认。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`src/app/terminal_bridge/{native,input}.rs`、`src/app/behavior_tests.rs` 及其终端滚轮测试子模块、双语开发说明、环境记录及 `docs/project-implementation-tracker/`。
- 不在本轮范围内：未经证据确认的滚轮行为修改、SSH trust/认证/凭据、传输协议、配置 schema、依赖/工具链、安装程序替换、推送和发布。
- 所有权：现有原生 hook 记录 Winit 事件，应用 bridge 记录实际终端模式和 worker 入队结果；UI 与终端模型继续决定滚轮行为，不增加跨模块接口。
- 收尾修复所有权：仅由接收测试夹具持有显式关闭标记，服务端结束时采样并返回结果；生产 SSH worker、断开流程及输入编码不变。
- 安全：诊断默认关闭，只记固定阶段、滚轮参数、模式、内部 UUID 和处理结果；不记终端/剪贴板内容、主机、路径或凭据，不增加队列/缓冲。

## 当前状态

- 阶段：验证中
- 开工判定：允许开工
- 是否需要联网：是，已完成
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| W1 | completed | 环境与原生 CI 证据核对 | macOS 主机、不可用 VirtualBox、Windows job 日志和实际测试夹具 | CI 运行在 Windows，但鼠标输入回归使用无窗口注入 |
| W2 | completed | 原生事件、模式选择与入队诊断 | 只输出脱敏元数据，不改变分发行为 | 同一目标 `ax_ssh::terminal_scroll` |
| W3 | completed | 定向回归、完整门禁、双语方法与本机构建 | wheel 回归、fmt/check/严格 Clippy/870 项测试/build、链接/tracker/diff 通过 | 本机 macOS ARM64；Windows 新诊断仍需原生门禁 |
| W4 | pending | 故障 Windows 主机与 macOS 的同条件复现 | 同一 AxSSH revision、同一远端 Codex/tmux、原生滚轮事件和处理模式对照 | 用户在另一台 Windows 运行编译产物；当前没有该主机连接信息 |
| W5 | pending | 按原生证据修复并验证 | 先确认首次分歧，再决定是否修改事件转换、模式或兼容行为 | 未确认最终根因，不将 Alternate Scroll 分支等同于根因 |
| W6 | completed | UI 滚轮到真实 SSH 接收端的字节断言 | 主/独立窗口完整矩阵通过；真实 TerminalPane、生产 callback/编码/worker、loopback SSH data 接收 | 固定测试密钥/凭据、有界接收及显式 shutdown，不绕过 host-key 验证 |
| W7 | completed | 新测试完整门禁与双语覆盖说明 | 标准 fmt/check/严格 Clippy/872 项测试、链接/tracker/diff 通过 | 进入既有原生 CI 测试矩阵，操作系统鼠标入口与真实 Codex 仍另验 |
| C1 | completed | 滚轮诊断与配套双语方法提交 | 显式暂存并审阅 staged diff | `c051c05`；不改变滚轮行为，排除测试与归档记录 |
| C2 | completed | SSH 接收字节回归与覆盖说明提交 | 新测试文件、夹具调整和双语说明独立审阅 | `7267e93`；沿用已通过的 872 项测试，不重复修改源码 |
| C3 | completed | 本轮环境和实施记录归档 | 链接/tracker/diff 和提交边界检查通过 | 本记录独立提交后核对最终工作区；W4/W5 和前轮原生验收保持 pending |
| W8 | completed | 接收测试的主动关闭结果边界修复 | 用户 CI 日志、锁定 russh 的 disconnect 队列语义与严格错误分类 | 仅在全部断言通过且显式关闭后接受 ConnectionReset；不改生产 transport |
| W9 | completed | 关闭边界回归、重复运行与完整门禁 | 提前 reset/其它错误仍失败，3 项定向和 20 轮重复通过；873 项完整测试及标准门禁通过 | 双语开发说明和地图已同步，保留 Windows 现场根因待确认 |

## 已完成

- 前轮撤销崩溃修复和记录已提交为 `5e0255d`/`4387e56`，本轮开工工作区干净；前轮原生验收事项保留。
- 已核对 Slint/Winit、TerminalGrid、TerminalPane、终端编码与 worker 路由；备用屏未开启鼠标上报时确实会产生上下键，但该规则跨平台共用，尚不能解释实际 Windows/macOS 分歧。
- Windows Winit 使用 LineDelta，macOS 可用 PixelDelta；两者进入 Slint 后统一按 cell 大小累计。参数区别不能直接证明错误模式的原因。
- 读取 GitHub CI run `38677130493` 的 Windows job `116233176549`：commit `25335e0` 的 check、严格 Clippy、build、365 library + 449 binary（814）测试通过，含两项实际 TerminalPane 的无原生窗口滚轮回归；不将其称为实际 Windows GUI/Codex 复现。
- 本机为 macOS ARM64；Windows Rust target 已安装但没有 Windows SDK/执行环境，VirtualBox 只剩失效启动脚本，未发现可用 Windows 虚拟机；连接工具中没有 Windows 桌面执行能力。
- 用户确认在另一台 Windows 主机运行编译产物；未提供该主机访问方式，按既有截图的 SSH 使用场景准备对照，不猜测故障现场模式。
- 已补齐 native-wheel、local-history、wheel-route 和 wheel-enqueue 四个阶段，记录原生 delta/缩放/修饰键、实时模式/有界步数和入队结果；没有新增 API、配置或缓冲，双语开发说明包含两端启动命令和对照方法。
- 新诊断通过本机完整门禁和 debug build，产物为 `target/debug/ax_ssh`；未启动真实会话或替换安装版。
- 用户进一步要求断言终端实际收到的滚轮信号；已选择复用进程隔离的应用测试平台，并增加固定密钥 loopback SSH 接收端，将覆盖延伸到实际 data 字节，不以回调发生或入队成功代替接收确认。
- 主/独立窗口的 UI 到 SSH 接收字节矩阵已通过，覆盖 SGR/传统、上下/水平、重复/上限、60px 与小量累计、备用屏普通/application 方向键、可选 Home、本地历史和 Shift。模式输出先经过真实 SSH 和生产 parser，再由正式 view 映射驱动 UI；用 FIFO 屏障确认零发送。
- 夹具显式拒绝不支持的辅助 exec，防止等待不存在的服务；模拟鼠标从实际 TouchArea 映射，避免落在 pane 标题栏。只在隔离测试中推进布局，不捕获或检查应用图像。
- 标准参数下 392 library + 480 binary（872）测试及 fmt/check/严格 all-target Clippy 全部通过；两项新增接收测试在标准优化级别也通过。前轮两个运行时诊断文件的 SHA-256 与本轮开工基线相同，新 Rust 修改仅限测试。
- 用户授权创建本地提交；索引为空，新增目录仅含真实测试源码，构建产物和缓存已忽略；按诊断、接收回归和项目记录三笔提交，不推送或改写历史。
- 已分别审阅并提交滚轮诊断 `c051c05` 与 SSH 接收回归 `7267e93`，两笔各含配套双语说明；提交后仅剩六份环境/实施记录，源码没有后续修改。
- 归档记录已整理，10 个相对链接和 current/map/本轮新增记录通过；完整 tracker 仍为 84 项历史问题，无新增；本记录独立提交后核对最终工作区。
- 用户追加 CI 失败日志：独立窗口矩阵所有接收/偏移断言已走完，在 `Fixture::finish` 的主动关闭后服务端返回 IO ConnectionReset，任务内 expect panic 再由 join 传播；前轮本机通过未覆盖该收尾时序。W8/W9 仅修复测试夹具的结束判据，不把日志当作 Windows 现场滚轮根因。
- 服务端采样显式关闭标记，仅接受该阶段的 IO ConnectionReset；正常返回、提前 reset、BrokenPipe/UnexpectedEof/InvalidData 和协议错误的边界回归通过，任务 panic 与 timeout 仍由 join/deadline 失败。
- 修复前旧二进制 10 轮未复现该偶发收尾；修复后 3 项定向和 20 轮重复（60 项测试、40 次 SSH/UI 矩阵）通过。标准 fmt/check/严格 all-target Clippy、392 library + 481 binary（873）测试、10 个相对链接及 current/map/本轮新增记录通过，完整 tracker 84 项历史问题无新增。

## 验证

- 已完成：W1–W3 的 870 项门禁及 debug build；W6/W7 的 872 项接收覆盖门禁；W8/W9 收尾边界回归、20 轮重复和标准 fmt、locked/offline check、严格 all-target Clippy、392 library + 481 binary（873）测试通过，无 warning；10 个相对链接、current/map/本轮记录和 diff 通过，完整 tracker 84 项历史问题无新增。
- 未完成：W4 故障主机原生复现；W5 最终根因确认和按证据修复；新代码在 Windows 的原生门禁、前轮待验收事项保留。

## 风险与阻塞

- 当前无法访问故障 Windows 主机；无窗口注入、Windows 原生 cargo test 和实际 Windows 鼠标/Codex 行为是不同证据层，不能互相替代。
- 禁止在没有模式证据时吞掉备用屏滚轮或对 Codex 软件名做特判，避免破坏 tmux/编辑器输入。
- 完整 tracker 已有 84 项历史格式问题；本轮 current/map/新增记录单独验证并比较错误集。

## 下一步

- 在后续原生 CI 中验证收尾修复；故障 Windows 与 macOS 使用同版源码、相同远端条件复现，并以默认关闭的滚轮诊断定位首次分歧。当前仅完成测试夹具修复，不据此宣称 Windows 实际滚动根因已解决。

## 最后更新时间

- 2026-10-15 18:04 +0800
