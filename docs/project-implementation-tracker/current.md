# 当前项目实施记录

## 当前目标

- 目标 ID：20260814-same-day-release-revision
- 目标：保留不可变的 `2026-08-14` 首发 tag，完成今天第二个发行版本并发布 `2026-08-14-1`。
- 交付物：支持修订 tag 的版本/Highlights 脚本、Create/Retry/Release workflow、与新 tag 一致的包元数据，以及已推送的 `2026-08-14-1` annotated tag。

## 项目边界

- 根目录：`<repo-root>`
- 当前范围：`.github/workflows/{ci,create-dated-release,retry-existing-release,release}.yml`、`scripts/{release_version,generate_release_highlights}.py`、对应 Python 回归、`Cargo.toml`、`Cargo.lock`、`packaging/macos/Info.plist`、发布文档及 `docs/project-{implementation-tracker,env-audit}/`。
- 不在本轮范围内：Slint UI、应用 bridge、SSH host-key trust、凭据、其他 transport、Rust 依赖/工具链、缓存键、Release build matrix，以及参考工程源码。

## 当前状态

- 阶段：已完成
- 开工判定：允许开工
- 是否需要联网：否
- 多 agent：未使用

## 活动计划

| Step | Status | Deliverable | Verification | Notes |
| --- | --- | --- | --- | --- |
| RR1 | completed | 既有 tag 的无写权限 CI 到 Release 重试 workflow | YAML 解析、workflow 静态审阅 | 不创建、删除或移动 tag；仅接受显式输入。 |
| RR2 | completed | 同日修订 tag 的版本、元数据和 Highlights 语义 | Python 回归、`release_version.py verify --tag 2026-08-14-1` | 首发 tag 保持 `2026-08-14`；第二个发行使用 `2026-08-14-1`。 |
| RR3 | completed | Create/Retry/Release workflow 和双语文档支持修订 tag | YAML、Markdown、tracker 检查 | 重试路径仍不创建、替换或移动 tag。 |
| RR4 | completed | 新发布提交、`master` 和 `2026-08-14-1` tag 推送 | 全量门禁、远端 ref 验证 | 既有 `2026-08-14` 保持不变；新 annotated tag 指向本次发布提交。 |
| LS1 | completed | 取消感知的本地 PTY 有界事件投递收敛 | 定向满队列 shutdown 压力回归 | 正常运行保持反压；取消后可丢弃未投递事件。 |
| LS2 | completed | reader/worker/child 生命周期回归 | 重复 `local_shell` 定向测试 | 不遗留阻塞 join 或孤儿 child。 |
| LS3 | completed | 完整 Rust 门禁与实施记录 | fmt/check/clippy/test/tracker/diff | 不修改 UI、SSH trust、凭据或依赖。 |
| MRA1 | completed | 每个 macOS target 的独立 `.app` ZIP 与明确附件名 | YAML/Shell 静态检查 | bundle 仅来自当前 target binary、本仓库资源与许可证。 |
| MRA2 | completed | 从两个独立 bundle 合并的 Universal `.app` ZIP | `lipo`/`codesign` CI 命令审阅 | Universal 只替换可执行文件，资源来自已验证 arm64 bundle。 |
| MRA3 | completed | 双语发布说明、环境/实施记录和全部静态门禁 | Python/YAML/Markdown/tracker/diff | 远端 macOS runner 与 GitHub Release 附件留实际 tag 验证。 |
| MRA4 | completed | arm64、x86_64 与 Universal 三份本机 bundle 回环验证 | `lipo`、codesign、ZIP 解包和资源检查 | 三份 archive 使用 CI 同一组装步骤；Universal 从两份原生 bundle 合成。 |
| TR1 | completed | 有界 download state、选择和暂停/恢复/取消状态转换 | `app::state` 定向回归 | 状态属于 AppState；UI 只接收有界行 DTO。 |
| TR2 | completed | SFTP worker 的递归展开、暂停/恢复/取消与断点缓存协议 | `sftp::transfer`、`ssh::worker::sftp` 回归 | russh session 留在 worker；取消只清理该任务创建的数据。 |
| TR3 | completed | 三个 Transfers 页面、行勾选和批量操作回调 | `cargo check --locked --offline` | Slint 不执行文件系统或网络操作。 |
| TR4 | completed | 双语说明、跟踪资料和完整仓库门禁 | fmt/check/clippy/test/tracker/diff | GUI 交互仍需用户在目标平台验收。 |
| WR1 | completed | worker-owned SFTP 写命令、上传与有界远端文件操作 | focused worker/transfer tests | 上传使用私有远端临时文件和 rename；写操作串行复用独占 stream。 |
| WR2 | completed | 应用状态、在线编辑/Save As、删除/重命名和冲突检测 | app state/bridge tests | 编辑内容有界，保存前重验远端大小 fingerprint。 |
| WR3 | completed | UI 操作入口、拖拽意图、默认关闭的监控/自动上传说明 | Cargo check + user GUI acceptance | 当前 Slint 提供路径意图；自动上传/监控保持关闭。 |
| WR4 | completed | 远端变更监控、显式自动上传、编辑同步提示和拖拽 intent | focused state/worker tests + full gate | 自动上传默认关闭；真实 SFTP 与 GUI 仍需用户验收。 |
| WR5 | completed | 双语契约、回归测试和完整门禁 | fmt/check/clippy/test/tracker/diff | 已恢复下载后打开，保存成功同步 fingerprint，Save As 区分新路径，旧监控按代次退出，拖入路径完成 URI/长度/控制字符校验。 |
| RE1 | completed | 有界重连策略、退避与代次取消状态 | state/connection 定向测试 | 默认最多 5 次，1/2/4/8/16 秒退避，上限 30 秒；主动断开、关闭 Tab 和新 attempt 使旧任务失效。 |
| RE2 | completed | SSH/Telnet/Serial worker 重建与安全认证边界 | connection monitor/authentication 定向测试 | SSH 只复用已验证 host key 和可用私钥/Agent/系统凭据；无密码材料转人工提示。 |
| RE3 | completed | UI 状态提示、终端内容保留与恢复动作 | cargo check + Slint mapping tests | 断线保留 Tab/scrollback，状态文本提供倒计时、重连中、成功及最终失败提示。 |
| RE4 | completed | 双语契约、回归测试和完整仓库门禁 | fmt/check/clippy/test/translations/tracker/diff | 真实网络与目标平台 GUI 断线场景留人工验收。 |
| WS1 | completed | 版本化 workspace snapshot DTO、私有原子存储与校验 | focused config/pane tests | 不保存 worker、连接句柄、凭据或 host-key 临时状态。 |
| WS2 | completed | AppState/PaneTree/WindowRouter 导出与恢复 | state/router tests | 限制 pane 数、路径/终端文本长度和总快照大小。 |
| WS3 | completed | 启动恢复 Tab/连接与退出原子保存 | cargo check + lifecycle tests | 恢复连接重新经过正常认证和 host-key 流程。 |
| WS4 | completed | 双语契约、项目地图和完整门禁 | fmt/check/clippy/test/tracker/diff | GUI 和真实网络行为由用户在目标平台验收。 |
| MP1 | completed | TerminalModel mouse modes、事件 DTO 和 SGR/UTF-8/legacy 编码 | focused terminal tests | 只发送有界坐标/按钮/修饰键；模式由解析后的终端状态决定。 |
| MP2 | completed | Slint pointer routing、selection/scroll fallback 与 worker send | cargo check + UI mapping tests | reporting 开启时 pointer 不被本地选择吞掉；关闭时保持既有行为。 |
| MP3 | completed | 双语契约、tracker/project-map 与完整门禁 | fmt/check/clippy/test/translations/tracker/diff | TUI 真实交互仍需用户在目标平台验收。 |
| KH1 | completed | OpenSSH known_hosts 解析、主机匹配、hashed host 与 @revoked 判定 | `ssh::known_hosts` 定向回归 | malformed/读取失败只收窄信任，不放宽。 |
| KH2 | completed | 将共享 known_hosts 快照接入 probe、连接和 host-key 错误状态 | SSH trust/connection tests | profile 指纹与系统记录冲突时拒绝。 |
| KH3 | completed | 撤销/替换记录的显式、原子、保留其他行的管理能力 | known_hosts file update tests | 不提供普通确认绕过 revoked。 |
| KH4 | completed | 双语契约、项目地图和完整仓库门禁 | fmt/check/clippy/test/translations/tracker/diff | 真实系统文件权限与 GUI 需用户验收。 |
| WD1 | completed | JetBrains Mono 默认字重编译期内嵌与旁置资源兼容 | font bridge focused test | Slint UI 线程仍是字体注册的唯一 owner。 |
| WD2 | completed | TCP/banner-KEX/host-key 决策错误分类与完整错误链 | SSH focused tests + read-only interoperability probe | 未知、变化和撤销 key 仍默认拒绝，不启用弱算法。 |
| WD3 | completed | 双语契约、环境记录和完整仓库门禁 | fmt/check/clippy/test/tracker/Markdown/diff | Windows 新 EXE 留目标机手工复测。 |
| WL1 | completed | TerminalModel 有界 `PtyWrite` 应答并写回当前 Tab transport | terminal protocol focused test | 应答不进入 Slint、日志或持久化。 |
| WL2 | completed | 本地 PTY resize 去重、自动退出 owner 提取和有超时 shutdown | local shell/bridge focused tests | Drop 保留 child killer 兜底；无无限 join。 |
| WL3 | completed | New Session 显式 scroll viewport 高度 | Cargo/Slint check | GUI 滚动仍需用户验收。 |
| WL4 | completed | 双语契约、完整仓库门禁与 Windows release 交叉构建 | fmt/check/clippy/test/tracker/diff/xwin | Windows ConPTY/CPU/进程退出需目标机验收。 |
| TB1 | completed | 统一 TerminalPane/TerminalGrid 内容区原点和小 pane 裁剪边界 | `cargo check --locked --offline` + Slint mapping/tests | 正常高度顶部无额外轨道；低于三行时保留底行锚定。 |
| TB2 | completed | 更新双语契约、环境记忆和实施记录 | tracker/Markdown/diff checks | 不改变终端模型、PTY、worker 或 SSH 安全边界。 |
| TB3 | completed | Rust 侧统一终端尺寸上下限，并兼容现有配置常量路径 | focused dimension tests + `cargo check --locked --offline` | UI 三行保底与 worker/protocol 1x1 下限保持分层语义。 |
| TB4 | completed | 统一网格、预编辑层和 IME 的光标纵坐标来源 | Slint compile + terminal geometry tests | 所有视觉层使用同一 content-origin 计算，父级负责裁剪。 |
| TB5 | completed | 收口双语契约、tracker 与完整仓库门禁 | fmt/check/clippy/test/translations/tracker/diff | GUI 视觉仍需用户在目标平台验收。 |
| TB6 | completed | 固定连续 resize、尺寸变化与输出交错的根因 | terminal/state/view focused regressions | 根因覆盖 SSH PTY 裸 LF 与 resize 后快照边界。 |
| TB7 | completed | 统一 resize 事务与最终尺寸快照，移除重复边界修正 | terminal/state/SSH tests | 共享尺寸先规范化；本地模型和 worker 使用同一最终尺寸。 |
| TB8 | completed | 同步双语契约、项目地图和环境记忆 | tracker/Markdown/diff checks | 记录本地快照先行、worker `Resized` 仅为传输确认。 |
| TB9 | completed | 完成全量构建、测试和静态门禁 | fmt/check/clippy/test/translations/tracker/diff | 目标平台视觉和真实窗口拖动仍需用户验收。 |
| RB1 | completed | 锁定终端核心与主流终端的纵向扩容语义对照 | 上游源码与现有模型路径审阅 | 有历史时恢复历史；无历史时底部补空行。 |
| RB2 | completed | 移除 `TerminalModel` 的后置底部锚定补偿 | 25 项 `terminal` 定向回归 | 仅终端缓冲区层改变，不修改 Slint 或 PTY。 |
| RB3 | completed | 更新双语架构、项目地图、环境与实施记录 | tracker/environment 校验 | 记录外部对照及不变的安全边界。 |
| RB4 | completed | 完成完整离线 Rust/Slint 门禁 | fmt/check/clippy/test/tracker/diff | GUI 视觉由用户在目标平台验收。 |

## 已完成

- 已完成施工前环境预检：项目保持 Rust 2024、MSRV 1.92.0、Slint 1.17.1 与锁定离线 Cargo 门禁；本机 `cargo fmt` 和 `cargo clippy` 可用。
- 已确认项目地图已覆盖 SFTP transfer、worker、application bridge 和 Slint 组合路径；本轮只需在收口时更新其传输语义摘要。
- 已确认既有能力仅为 private-cache 的单文件 download-to-open，取消会删除未发布的部分文件；它不提供暂停、续传或目录递归。
- 已完成 TR1-TR3：AppState 以有界活动/失败/成功传输行和勾选状态表达操作权限；worker 递归展开远端目录，并在存活期间暂停/从当前 offset 继续；Slint 只呈现三页与批量意图。
- 已完成本地目标与清理收紧：下载保留当前 Local files 目录中的相对树，以安全 `.part` 文件 fsync 后无覆盖原子发布；递归扫描限制为 4,096 个条目；失败、取消及发布后观察到的取消都会由 worker 清理其任务创建的内容。
- 已完成 worker 写命令、上传临时发布、删除、重命名、有界 UTF-8 编辑/Save As、远端大小冲突拒绝、Local files 单文件上传和 UI 操作入口；中英文使用/架构契约已同步。
- WR4 已完成有界远端 fingerprint 监控、默认关闭的自动上传开关、编辑同步提示和基于现有选中项的拖拽 intent；WR5 已修复新增路径与既有下载后打开能力之间的回归边界，不在 UI 线程执行文件 I/O。
- 已完成 Windows/跨平台连接诊断收紧：SSH 将 TCP 建连与 russh banner/KEX 分开并保留完整错误链，仍维持未知/变更 host key 默认拒绝；本地 ConPTY/PTY reader 错误不再静默丢弃，而是通过有界失败事件结束 worker。JetBrains Mono 四个默认字重改为编译期内嵌，单独运行 Windows EXE 不再依赖旁置字体资源；其余字体仍使用既有有界运行时资源路径。
- 已确认 Windows 本地 Shell 无输出的直接协议缺口：锁定的 `portable-pty` 以 inherit-cursor 模式创建 ConPTY，而既有 `TerminalModel` 使用 `VoidListener` 丢弃光标位置等 `PtyWrite` 应答。当前实现以有界私有队列收集应答并写回当前 Tab worker；同时保留自动退出 worker owner、去重相同 PTY 尺寸，并移除 shutdown 超时后的无限等待。
- New Session 的 `ScrollView` 现在由编辑内容 preferred height 显式驱动 viewport height，被窗口遮盖的字段可滚动访问。
- 已确认截图中的顶部空带来自 `TerminalModel::resize` 在上游 resize 后再次 `scroll_down` 并强制将 cursor 设为新底行，不是 Slint 布局偏移。
- 已完成外部语义对照：Alacritty、xterm.js、WezTerm 与 kitty 的增长路径都只使用真实 scrollback 填充新增区域；无历史时保留内容顶部并在底部补空行。
- 已完成 RB2：删除 `Term::resize` 后的二次网格滚动和 cursor-to-bottom 修正；25 项 `terminal` 定向回归通过。
- 已完成 RB3：同步中英文架构/使用说明、项目地图、环境预检、外部来源和实施记录。
- 已完成 MRA1-MRA2：两个 macOS target 均使用 `packaging/macos/build-app.sh` 将当前 target binary 和本仓库资源封装为独立 `.app` ZIP；Universal job 解包两个 bundle、以 `lipo` 替换可执行文件后重新 ad-hoc 签名并发布第三个 ZIP。
- 已完成 MRA3-MRA4：本机 arm64、x86_64 与 Universal release bundle 均已通过 `lipo` 架构验证、ad-hoc 签名、ZIP 打包/解包、资源存在性与解包后二次签名验证；YAML/Shell、Python release 回归和完整离线 Cargo 门禁均通过。
- 已复现依据：GitHub-hosted macOS `cargo test --locked` 中，`local_shell::tests::shutdown_terminates_a_running_shell_with_a_full_event_queue` 在满事件队列时超过 5 秒；本机单次定向测试通过，故按间歇性竞态处理。
- 已完成 LS1-LS2：取消现在在每次 event delivery 前优先终止未投递事件；有效 process group 被终止后不再等待 child-killer 锁和 SIGHUP 宽限期。新增取消时不入队回归，满队列 shutdown 定向测试连续 20 次通过。
- 已完成 RR1-RR3：既有 `2026-08-14` 保持不可变；新修订 tag `2026-08-14-1` 的 Cargo/lockfile/macOS 元数据、Create/Retry/Release workflow、Highlights range 和双语文档已同步。

## 验证

- 已完成：默认字体无外部目录回归、SSH banner 后提前断开回归、既有 probe/password loopback 回归、开发机 OpenSSH/AxSSH 对同一目标的只读 host-key 互操作诊断；`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整测试（库 172、应用 162、Doc tests 0）、413 条翻译、46 个 Markdown 相对链接、差异检查及独立 target 的 Windows MSVC release 交叉构建通过。
- 已完成：TerminalPane/TerminalGrid 内容区边界修复；既有 TB1/TB2 的 Cargo、Clippy、格式、测试和差异门禁。
- 已完成：新增 Rust 共享尺寸模块；配置常量保持兼容 re-export；模型、设置控件、local/SSH/Telnet 后端复用最大值；网格、预编辑层和 IME proxy 复用 pane 的 `cursor-cell-y`；设置文案与中文目录已同步。
- 已完成：连续 resize 硬换行列回归、AppState 41 项状态回归、真实 loopback SSH PTY modes 回归、`cargo check`、严格 Clippy、完整 Cargo 测试、翻译检查、tracker 校验和差异检查。
- 已完成：RB4 的 `cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 173、应用 160、Doc tests 0）、tracker/Markdown 链接与 `git diff --check`。
- 已完成：MRA3-MRA4 的 Ruby YAML 解析、`sh -n packaging/macos/build-app.sh`、Python release-version/Highlights 9 项回归、本机 arm64、x86_64 与 Universal macOS bundle 的 `lipo`/codesign/ZIP round-trip（Universal 保留 17 个运行时字体文件）、`cargo fmt --all -- --check`、`cargo check --locked --offline`、严格 Clippy、完整 Cargo 测试（库 177、应用 162、Doc tests 0）、tracker、Markdown 相对链接与差异检查。
- 已完成：本机 `cargo test --lib local_shell::tests::shutdown_terminates_a_running_shell_with_a_full_event_queue --locked --offline -- --exact --nocapture` 单次通过。
- 已完成：LS3 的 `cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、完整 `cargo test --locked --offline`（库 178、应用 162、Doc tests 0）和 `git diff --check`。
- 已完成：`python scripts/release_version.py verify --tag 2026-08-14` 在修复前按预期失败，错误为 Cargo 版本不是 `2026.8.14`。
- 已完成：Python release helper/Highlights 12 项回归、`python scripts/release_version.py verify --tag 2026-08-14-1`、Ruby YAML 解析、`plutil -lint packaging/macos/Info.plist` 和 `git diff --check`。
- 已完成：`cargo fmt --all -- --check`、`cargo check --locked --offline`、`cargo clippy --all-targets --locked --offline -- -D warnings`、`cargo test --locked --offline`（库 178、应用 162、Doc tests 0）、tracker 校验与 Markdown 相对链接检查。
- 未完成：Windows 新 EXE 的字体注册、ConPTY、UI、真实 SSH 连接、正常 shell 的连续纵向拖动、分屏极小尺寸和鼠标/IME 坐标均需目标平台人工验收。
- 未完成：下一次日期 tag 的 GitHub-hosted macOS runner 需实际验证两个 target 链接、ad-hoc 签名、Universal `lipo` 合并以及 Release 页面上的三份 macOS 附件。

## 风险与阻塞

- 自动重连只在当前进程内有效，最多 5 次；跨进程重连、密码明文缓存和未知/变更 host key 自动接受均明确不支持。
- SFTP v3 没有持久化任务恢复协议；本轮的断点继续限定为同一运行期的暂停后继续，partial 文件只属于仍存活的 worker-owned 传输。
- 递归目录下载必须限制深度、文件数、路径文本和总字节，且跳过符号链接；超限或不可读项将以有界失败行呈现。
- 主屏高度增长且存在 scrollback 时，当前提示符下移、顶部露出历史行是预期行为；修复只禁止伪造空白历史行。
- GitHub Release 继续使用 ad-hoc 签名，不包含 Developer ID 签名或 notarization；本轮只确保三个附件的架构和 bundle 内容一致。
- 本地 PTY 的正常输出不可因 UI 消费短暂滞后而无界积压或静默丢失；仅在 owner 已请求取消时允许放弃尚未投递的事件以保证 join 收敛。
- `Create Dated Release` 只用于创建新的日期或同日修订 tag；重试路径不得替换 tag，必须先验证显式 tag 的元数据和 SHA 对应的 CI 成功状态。

## 下一步

- 在 Windows 目标机使用本轮新 EXE 复测 cmd/PowerShell 输出与输入、`conhost.exe` 空闲 CPU、关闭 Tab 后 child 消失、AxSSH 正常退出，以及 New Session 超长内容滚动。
- 在相同构建中连续纵向缩放普通 shell；无 scrollback 时确认已有输出留在顶部、空行只出现在底部，并确认有 scrollback 时顶部显示真实历史行。
- 在下一个日期 tag 检查 Release 是否列出 `AxSSH-<version>-macos-aarch64.zip`、`AxSSH-<version>-macos-x86_64.zip` 和 `AxSSH-<version>-macos-universal.zip`，并在目标 Apple Silicon/Intel 机器启动对应 bundle。
- 在 GitHub 上为 `2026-08-14-1` 运行 **Retry Existing Release**，由其重新 dispatch 同一 tag SHA 的 CI；成功后自动 dispatch Release。

## 最后更新时间

- 2026-08-14：完成并准备发布同日第二发行；`2026-08-14-1` 映射为 Cargo `2026.8.14+1`、Debian `2026.8.14-1`、macOS build `20260814.1`。完整本地门禁、tracker 与链接检查均通过；未使用多 agent。

- 2026-08-13：完成 MP1-MP3 全屏终端 mouse reporting 实施；不联网、不使用多 agent。工作区恢复、SFTP WR1-WR5 和 RE1-RE4 保持完成。真实 TUI 交互仍需用户验收。
- 2026-08-13：启动 KH1-KH4 known_hosts 兼容实施；复用锁定的 `russh`/`ssh-key`，不联网、不使用多 agent。
- 2026-08-13：完成 KH1-KH4；系统 known_hosts 读取、共享追加、撤销拒绝、changed/unknown 确认、changed 替换和原子移除能力接入并通过定向/全量测试。补充非默认端口不误匹配 plain host、`@cert-authority` 不作普通 host key 信任以及保留无关/撤销行的回归。
- 2026-08-13：补充 Windows SSH/本地终端诊断；分离 TCP 与 SSH banner/KEX 超时上下文，保留底层握手错误，ConPTY reader 非 EOF 错误转为有界 worker failure。完整测试与严格 Clippy 通过；真实 Windows ConPTY、安装包字体路径和远端网络仍需目标机验收。
- 2026-08-13：补充 macOS 交叉编译 Windows 文档；固定 `x86_64-pc-windows-msvc`、NASM、Homebrew LLVM/LLD、`rustup target add`、`cargo-xwin`、release 构建和字体/许可证 ZIP 打包步骤。根据 `aws-lc-sys` 实际构建错误补充 NASM、`llvm-lib` 和 `lld-link` 前置条件；未改变 Cargo、CI 或运行时契约。
- 2026-08-13 16:48 +0800：启动并完成 WD1-WD2；内嵌 JetBrains Mono 默认字重，收紧 SSH 阶段错误分类和实际 host-key 接受状态，进入 WD3 完整门禁。未使用多 agent。
- 2026-08-13 16:56 +0800：完成 WD3；完整 Cargo/Slint、翻译、Markdown、差异和 Windows MSVC release 交叉构建通过。tracker validator 仅保留既有旧历史时间错误；目标 Windows 运行时验收待用户执行。
- 2026-08-13 17:53 +0800：启动 WL1-WL4；只读参考同级 `ax_shell` 的 owner/协议应答行为，不引入依赖、源码复制或文档链接；未使用多 agent。
- 2026-08-13 18:07 +0800：完成 WL4；完整 Cargo/Slint 门禁和独立 target 的 Windows MSVC release 交叉构建通过，新 PE32+ GUI x86-64 产物 SHA-256 为 `7cef8594ecedc2531da7b518e14327f35bd5f6d6281763abcdecde4ee5c06ce4`。真实 ConPTY、CPU、退出与滚动行为留目标 Windows 手工验收。
- 2026-08-13：完成 TB1；普通终端网格从内容区顶部绘制，去除根 pane 额外顶部轨道，低于三行保底时才使用负偏移保持底行边界。
- 2026-08-13：完成 TB2；双语架构/使用说明、项目地图和环境记忆同步终端内容区边界语义，完整 Cargo、Clippy、格式和差异门禁通过；未联网、未使用多 agent。
- 2026-08-13：启动 TB3-TB5；按用户确认继续统一边界实现，新增 Rust 终端尺寸共享契约和 Slint 光标纵坐标单一来源；未联网、未使用多 agent。
- 2026-08-13：完成 TB3-TB4；终端尺寸上限、设置 `10x3` 下限和 UI 光标纵坐标已统一，设置翻译、架构说明和项目地图同步；进入 TB5 验证阶段，未联网、未使用多 agent。
- 2026-08-13：完成 TB5；focused 尺寸/几何测试、完整 Cargo 测试（库 170、应用 160、Doc tests 0）、严格 Clippy、翻译 413 条、tracker validator、格式和 `git diff --check` 全部通过；未联网、未使用多 agent。目标平台 GUI 视觉仍需用户验收。
- 2026-08-14 07:31 CST：针对用户提供的 resize 截图重开终端缓冲区语义修复；已完成外部实现对照，不使用多 agent。
- 2026-08-14 07:31 CST：完成 RB1-RB4；移除模型层伪造顶部空白的底部锚定补偿，25 项终端定向回归及完整离线门禁通过；不使用多 agent。
